//! FDF 页/模板/命名页引用/注解视图的**装配**部分：解引用、形态判定、
//! Table 248 → 249 → 250 的逐层展开、Table 249 `/Fields` 复用读端字段树、
//! Table 251 `/Page` 检查，以及 PDF 1.3 版本语义报告与
//! 单数组条目/全局预算两道防护。
//!
//! 产出的形态类型在 [`super::view`]，诊断变体在 [`super::issues`]；本模块
//! 只把不可信输入降级成「类型化 + 诊断 + 原值保留」，任何畸形都不向上
//! 抛错（与 `embedded/expand.rs`、`/JavaScript`、`/Target` 的读端宽容
//! 风格一致）。唯一的硬错误仍来自 [`Document::fdf`] 本身（确认是 FDF 之后
//! 的结构错误）。
//!
//! 环检测口径与 `embedded/expand.rs` 相同：对象号只在所属文件内唯一，
//! 故模板 `/Fields` 数组的 `HashSet` 每次装配一份数组时新建；跨数组的
//! 字段树则共享同一个 [`super::super::parse::Walker`]（节点数与深度上限
//! 约束的是**整份文件**的字段树）。

use std::collections::HashSet;

use crate::core::reader::content::text_value;
use crate::core::reader::navigation::resolve;
use crate::core::types::{Dict, Document, PdfVersion, Value};

use super::PROHIBITED_FDF_ANNOTATION_TYPES;
use super::issues::FdfPagesIssue;
use super::view::{FdfAnnotation, FdfAnnots, FdfNamedPage, FdfPage, FdfPages, FdfTemplate};
use super::{MAX_FDF_PAGES_ARRAY_ENTRIES, PagesBudget};
use crate::core::reader::fdf::embedded::scan::fdf_header_version;
use crate::core::reader::fdf::parse::{self, Walker};
use crate::core::reader::fdf::{FdfDocument, FdfField};

/// 装配 `/Pages` 视图（Table 243 raw L24874–L24877 + Table 248）；条目缺失
/// 返回 `None`。
pub(super) fn pages_view(
    document: &Document,
    fdf: &FdfDocument,
    budget: &mut PagesBudget,
) -> Option<FdfPages> {
    let raw = fdf.fdf.dict.get("Pages")?.clone();
    let reference = raw.as_ref();
    let mut issues = Vec::new();
    push_if_some(&mut issues, pdf13_entry_problem(document, fdf, "Pages"));
    push_if_some(&mut issues, pages_exclusive(fdf));

    let array = match resolve(document, &raw) {
        Ok(Value::Array(items)) => items.0,
        Ok(other) => {
            issues.push(FdfPagesIssue::EntryNotArray {
                key: "Pages",
                found: other.type_name(),
            });
            return Some(incomplete_pages(raw, reference, issues));
        }
        Err(error) => {
            issues.push(FdfPagesIssue::EntryUnresolvable {
                key: "Pages",
                message: error.to_string(),
            });
            return Some(incomplete_pages(raw, reference, issues));
        }
    };

    // 不 with_capacity：数组长度是不可信输入。字段树预算整份文件共享。
    let mut walker = Walker::new();
    let mut entries = Vec::new();
    for (page, item) in array.into_iter().enumerate() {
        if !admits(&mut issues, "Pages", entries.len(), budget) {
            break;
        }
        entries.push(page_entry(document, item, page, budget, &mut walker));
    }
    Some(FdfPages {
        raw,
        reference,
        entries,
        issues,
    })
}

/// 装配 `/Annots` 视图（Table 243 raw L24885–L24889 + Table 251）；条目缺失
/// 返回 `None`。
pub(super) fn annots_view(
    document: &Document,
    fdf: &FdfDocument,
    budget: &mut PagesBudget,
) -> Option<FdfAnnots> {
    let raw = fdf.fdf.dict.get("Annots")?.clone();
    let reference = raw.as_ref();
    let mut issues = Vec::new();
    push_if_some(&mut issues, pdf13_entry_problem(document, fdf, "Annots"));
    let array = match resolve(document, &raw) {
        Ok(Value::Array(items)) => items.0,
        Ok(other) => {
            issues.push(FdfPagesIssue::EntryNotArray {
                key: "Annots",
                found: other.type_name(),
            });
            return Some(FdfAnnots {
                raw,
                reference,
                entries: Vec::new(),
                issues,
            });
        }
        Err(error) => {
            issues.push(FdfPagesIssue::EntryUnresolvable {
                key: "Annots",
                message: error.to_string(),
            });
            return Some(FdfAnnots {
                raw,
                reference,
                entries: Vec::new(),
                issues,
            });
        }
    };
    let mut entries = Vec::new();
    for (index, item) in array.into_iter().enumerate() {
        if !admits(&mut issues, "Annots", entries.len(), budget) {
            break;
        }
        entries.push(annotation_entry(document, item, index));
    }
    Some(FdfAnnots {
        raw,
        reference,
        entries,
        issues,
    })
}

/// 单数组的两道防护：条目上限 + 全局预算；`false` 表示已记诊断并应停止。
/// 诊断只在**首次**触限时记一次，避免逐项重复。
fn admits(
    issues: &mut Vec<FdfPagesIssue>,
    key: &'static str,
    typed: usize,
    budget: &mut PagesBudget,
) -> bool {
    if issues.iter().any(|issue| {
        matches!(issue,
            FdfPagesIssue::ArrayTruncated { key: k, .. } | FdfPagesIssue::BudgetExhausted { key: k }
                if *k == key)
    }) {
        return false;
    }
    if typed >= MAX_FDF_PAGES_ARRAY_ENTRIES {
        issues.push(FdfPagesIssue::ArrayTruncated {
            key,
            limit: MAX_FDF_PAGES_ARRAY_ENTRIES,
        });
        return false;
    }
    if !budget.admit() {
        issues.push(FdfPagesIssue::BudgetExhausted { key });
        return false;
    }
    true
}

fn push_if_some(issues: &mut Vec<FdfPagesIssue>, issue: Option<FdfPagesIssue>) {
    if let Some(issue) = issue {
        issues.push(issue);
    }
}

fn incomplete_pages(
    raw: Value,
    reference: Option<(i64, i64)>,
    issues: Vec<FdfPagesIssue>,
) -> FdfPages {
    FdfPages {
        raw,
        reference,
        entries: Vec::new(),
        issues,
    }
}

/// Table 248 的一项：FDF 页字典。
fn page_entry(
    document: &Document,
    raw: Value,
    page: usize,
    budget: &mut PagesBudget,
    walker: &mut Walker,
) -> FdfPage {
    let mut issues = Vec::new();
    let dict = match entry_dict(&raw, document, &PageIssue::Page(page)) {
        Ok(dict) => dict,
        Err(problem) => {
            issues.push(problem);
            Dict::new()
        }
    };
    let templates = templates_of(document, &dict, page, budget, walker, &mut issues);
    FdfPage {
        raw,
        dict: dict.clone(),
        templates,
        // Table 248 `/Info`（Optional；「FDF page information dictionary」
        // 在 32000-1 全文只出现这一次、没有条目表，属 Adobe 扩展，
        // 待核）整体按原值保留，不发明键。
        info: dict.get("Info").cloned(),
        issues,
    }
}

/// 把一个数组项解引用成字典；形态不符/引用不可解析时给出诊断（调用方
/// 用空字典继续，原值始终保留在 `raw`）。
fn entry_dict(
    raw: &Value,
    document: &Document,
    problem: &PageIssue,
) -> std::result::Result<Dict, FdfPagesIssue> {
    match resolve(document, raw) {
        Ok(Value::Dict(dict) | Value::Stream(dict, _)) => Ok(dict),
        Ok(other) => Err(match problem {
            PageIssue::Page(page) => FdfPagesIssue::PageNotDictionary {
                page: *page,
                found: other.type_name(),
            },
            PageIssue::Template { page, template } => FdfPagesIssue::PageTemplateNotDictionary {
                page: *page,
                template: *template,
                found: other.type_name(),
            },
            PageIssue::Annotation(index) => FdfPagesIssue::AnnotationNotDictionary {
                index: *index,
                found: other.type_name(),
            },
        }),
        Err(error) => Err(match problem {
            PageIssue::Page(page) => FdfPagesIssue::PageUnresolvable {
                page: *page,
                message: error.to_string(),
            },
            PageIssue::Template { page, template } => FdfPagesIssue::PageTemplateUnresolvable {
                page: *page,
                template: *template,
                message: error.to_string(),
            },
            PageIssue::Annotation(index) => FdfPagesIssue::AnnotationUnresolvable {
                index: *index,
                message: error.to_string(),
            },
        }),
    }
}

/// `entry_dict` 的位置标签：同一份解引用逻辑服务三层数组。
enum PageIssue {
    Page(usize),
    Template { page: usize, template: usize },
    Annotation(usize),
}

/// Table 248 `/Templates`（Required; array of FDF template dictionaries，
/// raw L25087–L25088 与 L25058–L25059）。
fn templates_of(
    document: &Document,
    dict: &Dict,
    page: usize,
    budget: &mut PagesBudget,
    walker: &mut Walker,
    issues: &mut Vec<FdfPagesIssue>,
) -> Vec<FdfTemplate> {
    let Some(entry) = dict.get("Templates") else {
        // Required 条目缺失：不丢弃这一页，只记 shall 违例。
        issues.push(FdfPagesIssue::PageTemplatesMissing { page });
        return Vec::new();
    };
    let array = match resolve(document, entry) {
        Ok(Value::Array(items)) => items.0,
        Ok(other) => {
            issues.push(FdfPagesIssue::PageTemplatesNotArray {
                page,
                found: other.type_name(),
            });
            return Vec::new();
        }
        Err(error) => {
            issues.push(FdfPagesIssue::PageTemplateUnresolvable {
                page,
                template: 0,
                message: error.to_string(),
            });
            return Vec::new();
        }
    };
    let mut templates = Vec::new();
    for (template, item) in array.into_iter().enumerate() {
        if !admits(issues, "Templates", templates.len(), budget) {
            break;
        }
        templates.push(template_entry(
            document, item, page, template, budget, walker,
        ));
    }
    templates
}

/// Table 249 的一项：FDF 模板字典。
fn template_entry(
    document: &Document,
    raw: Value,
    page: usize,
    template: usize,
    budget: &mut PagesBudget,
    walker: &mut Walker,
) -> FdfTemplate {
    let mut issues = Vec::new();
    let dict = match entry_dict(&raw, document, &PageIssue::Template { page, template }) {
        Ok(dict) => dict,
        Err(problem) => {
            issues.push(problem);
            Dict::new()
        }
    };
    let named_page = named_page_of(document, &dict, page, template, &mut issues);
    let fields = template_fields(document, &dict, page, template, budget, walker, &mut issues);
    // Table 249 `/Rename`（Optional; boolean; **Default value: true**，
    // raw L25120–L25123）：缺省不物化成 `Some(true)`，形态不符也不猜值。
    let rename = match dict.get("Rename") {
        None => None,
        Some(Value::Bool(flag)) => Some(*flag),
        Some(other) => {
            issues.push(FdfPagesIssue::TemplateRenameNotBoolean {
                page,
                template,
                found: other.type_name(),
            });
            None
        }
    };
    FdfTemplate {
        raw,
        dict,
        named_page,
        fields,
        rename,
        issues,
    }
}

/// Table 249 `/TRef`（Required；raw L25115–L25116 与 L25097–L25098）
/// → Table 250 的命名页引用字典。
fn named_page_of(
    document: &Document,
    dict: &Dict,
    page: usize,
    template: usize,
    issues: &mut Vec<FdfPagesIssue>,
) -> Option<FdfNamedPage> {
    let Some(raw) = dict.get("TRef") else {
        issues.push(FdfPagesIssue::TemplateReferenceMissing { page, template });
        return None;
    };
    let raw = raw.clone();
    let value = match resolve(document, &raw) {
        Ok(value) => value,
        Err(error) => {
            issues.push(FdfPagesIssue::TemplateReferenceUnresolvable {
                page,
                template,
                message: error.to_string(),
            });
            return None;
        }
    };
    let Some(ref_dict) = value.as_dict() else {
        issues.push(FdfPagesIssue::TemplateReferenceNotDictionary {
            page,
            template,
            found: value.type_name(),
        });
        return None;
    };
    // Table 250 `/Name`（Required; string，raw L25126）：只有字面串/
    // 十六进制串就地解码（与 `/Target`、Table 245 同一口径）；其余形态
    // 记诊断并保留原值。
    let name = ref_dict
        .get("Name")
        .filter(|value| matches!(value, Value::String(_) | Value::HexString(_)))
        .and_then(text_value);
    match ref_dict.get("Name") {
        None => issues.push(FdfPagesIssue::TemplateNameMissing { page, template }),
        Some(value) if name.is_none() => issues.push(FdfPagesIssue::TemplateNameNotString {
            page,
            template,
            found: value.type_name(),
        }),
        Some(_) => {}
    }
    Some(FdfNamedPage {
        raw,
        dict: ref_dict.clone(),
        name,
        // Table 250 `/F`（Optional; file specification，raw L25127–L25128）：
        // 缺席即规范推定「该页在关联的 PDF 文件里」，不伪造文件规范。
        file: ref_dict.get("F").cloned(),
    })
}

/// Table 249 `/Fields`（Optional；元素是「指向 Table 246 字段字典的引用」，
/// raw L25117–L25119）：逐项复用读端字段树，畸形项降级为诊断。
fn template_fields(
    document: &Document,
    dict: &Dict,
    page: usize,
    template: usize,
    budget: &mut PagesBudget,
    walker: &mut Walker,
    issues: &mut Vec<FdfPagesIssue>,
) -> Vec<FdfField> {
    let Some(entry) = dict.get("Fields") else {
        return Vec::new();
    };
    let array = match resolve(document, entry) {
        Ok(Value::Array(items)) => items.0,
        Ok(other) => {
            issues.push(FdfPagesIssue::TemplateFieldsNotArray {
                page,
                template,
                found: other.type_name(),
            });
            return Vec::new();
        }
        Err(error) => {
            issues.push(FdfPagesIssue::TemplateFieldSkipped {
                page,
                template,
                field: 0,
                message: error.to_string(),
            });
            return Vec::new();
        }
    };
    let mut visited = HashSet::new();
    let mut fields = Vec::new();
    for (field, item) in array.into_iter().enumerate() {
        if !admits(issues, "TemplateFields", fields.len(), budget) {
            break;
        }
        if let Some(key) = item.as_ref()
            && !visited.insert(key)
        {
            // 同一数组里重复引用同一字段字典：只类型化一次。
            continue;
        }
        // Table 249 明说这些是「root fields (those with no ancestors in
        // the field hierarchy)」⇒ 全限定名基线是空串、深度从 0 起，
        // `/Kids` 子项（Table 246 允许）照常走同一 walker 的预算。
        match parse::field_entry(document, &item, "", 0, walker) {
            Ok(parsed) => fields.push(parsed),
            Err(error) => issues.push(FdfPagesIssue::TemplateFieldSkipped {
                page,
                template,
                field,
                message: error.to_string(),
            }),
        }
    }
    fields
}

/// Table 251 / 12.5.2 的一项：FDF 注解字典 + 必需的 `/Page` 序数。
fn annotation_entry(document: &Document, raw: Value, index: usize) -> FdfAnnotation {
    let mut issues = Vec::new();
    let dict = match entry_dict(&raw, document, &PageIssue::Annotation(index)) {
        Ok(dict) => dict,
        Err(problem) => {
            issues.push(problem);
            Dict::new()
        }
    };
    let subtype = dict
        .get("Subtype")
        .and_then(Value::as_name)
        .map(str::to_owned);
    // Table 251 `/Page`（**Required for annotations in FDF files**；integer，
    // 「where page 0 is the first page」，raw L25130–L25132）：严格按整型
    // 取值，浮点/字符串都不猜（与 `parse::parse_integer` 口径一致）。
    let page = match dict.get("Page") {
        None => {
            issues.push(FdfPagesIssue::AnnotationPageMissing {
                index,
                subtype: subtype.clone(),
            });
            None
        }
        Some(Value::Integer(ordinal)) => Some(*ordinal),
        Some(other) => {
            issues.push(FdfPagesIssue::AnnotationPageNotInteger {
                index,
                found: other.type_name(),
            });
            None
        }
    };
    if let Some(ordinal) = page
        && ordinal < 0
    {
        issues.push(FdfPagesIssue::AnnotationPageNegative {
            index,
            page: ordinal,
        });
    }
    // Table 243 raw L24887–L24889 的类型白名单核对。
    match subtype.as_deref() {
        None => issues.push(FdfPagesIssue::AnnotationSubtypeMissing { index }),
        Some(name) if PROHIBITED_FDF_ANNOTATION_TYPES.contains(&name) => {
            issues.push(FdfPagesIssue::AnnotationSubtypeProhibited {
                index,
                subtype: name.to_string(),
            });
        }
        Some(_) => {}
    }
    FdfAnnotation {
        raw,
        dict,
        page,
        subtype,
        issues,
    }
}

/// Table 243 的 shall：`/Pages` 与 `/Fields`、`/Status` 不得同现
/// （raw L24865–L24877）。读端宽容，只记诊断。
fn pages_exclusive(fdf: &FdfDocument) -> Option<FdfPagesIssue> {
    let fields = fdf.fdf.dict.get("Fields").is_some();
    let status = fdf.fdf.dict.get("Status").is_some();
    if fields || status {
        return Some(FdfPagesIssue::PagesWithFieldsOrStatus { fields, status });
    }
    None
}

/// 读端版本语义（**不拒绝**）：`/Pages` 与 `/Annots` 都是 PDF 1.3 条目
/// （Table 243 raw L24874、L24885），声明版本低于 1.3 时仍照常类型化，
/// 只额外记一条诊断。声明版本优先序与 `embedded/expand.rs` 一致：
/// FDF 目录 `/Version`（Table 242，raw L24841–L24849）→ `%FDF-x.y`
/// 文件头（12.7.7.2.2）→ [`Document::source_version`]。禁止新增
/// `PdfFeature` 变体，故直接比较 [`PdfVersion`] 常量。
fn pdf13_entry_problem(document: &Document, fdf: &FdfDocument, key: &str) -> Option<FdfPagesIssue> {
    let declared = declared_version(document, fdf);
    if declared >= PdfVersion::V1_3 {
        return None;
    }
    let declared = declared.as_str().to_string();
    Some(if key == "Annots" {
        FdfPagesIssue::AnnotsVersionGate { declared }
    } else {
        FdfPagesIssue::PagesVersionGate { declared }
    })
}

/// 本文件的 FDF 声明版本。
fn declared_version(document: &Document, fdf: &FdfDocument) -> PdfVersion {
    fdf.catalog_version
        .as_deref()
        .and_then(PdfVersion::from_name)
        .or_else(|| fdf_header_version(document))
        .unwrap_or_else(|| document.source_version())
}
