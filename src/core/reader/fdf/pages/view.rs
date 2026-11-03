//! FDF 页/模板/命名页引用/注解类型化视图的**形态**部分（Table 248、
//! 249、250、251）：只读结构体与 accessor。装配（解引用、降级、防护）
//! 在 [`super::expand`]，shall 违例的类型化诊断在 [`super::issues`]，
//! Annex J 导入合并在 [`super::merge`]，对外入口是
//! [`Document::fdf_pages`]、[`Document::fdf_annots`] 与
//! [`Document::fdf_import_merge`]（定义在 `fdf/pages.rs`）。
//!
//! 与 `embedded/view.rs` 同一策略：**原值永不丢弃**（每个结构都带
//! `raw` 与 `dict`），形态不符一律降级为 [`super::FdfPagesIssue`]，
//! **可选条目的缺省值不物化**（见各字段的文档）。

use crate::core::types::{Dict, Value};

use super::super::FdfField;
use super::issues::FdfPagesIssue;

/// Table 243 的 `/Pages` 条目（Optional; PDF 1.3；raw L24874–L24877）与其
/// 元素（Table 248，raw L25085–L25090）的类型化只读视图。
///
/// 规范说这一数组描述「**shall be added to the target document** 的新页」
/// （L25056），与 12.7.6（L24742–L24749）PDF 名字字典里映射**已有**页的
/// `Pages` 树是两回事。
#[derive(Debug, Clone, PartialEq)]
pub struct FdfPages {
    /// `/Pages` 条目原值（引用形态时仍是 `Value::Ref`，一条不丢）。
    pub raw: Value,
    /// 原值为间接引用时的 `(对象号, generation)`；直接数组形态为 `None`。
    pub reference: Option<(i64, i64)>,
    /// 逐项类型化的 FDF 页字典（Table 248），按数组声明序。
    /// 单次数组展开受 [`super::MAX_FDF_PAGES_ARRAY_ENTRIES`] 与
    /// [`super::MAX_FDF_PAGES_TOTAL_ENTRIES`] 双重约束，超出部分跳过并记
    /// [`Self::issues`]。
    pub entries: Vec<FdfPage>,
    /// 数组级与逐页级的降级与 shall 违例（非致命）。
    pub issues: Vec<FdfPagesIssue>,
}

impl FdfPages {
    /// 已类型化的页字典数（注意：这可能是被上限截断后的数量）。
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 是否没有任何已类型化页字典。
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 按数组下标取页字典（声明序，越界 `None`）。
    pub fn page(&self, index: usize) -> Option<&FdfPage> {
        self.entries.get(index)
    }

    /// Table 249 `/Fields` 展开出的**全部**模板字段（按页序、页内模板序
    /// 与数组声明序）；Table 243 的 `/Fields` ↔ `/Pages` 互斥由
    /// [`Self::pages_exclusive_conflict`] 报告（raw L24865–L24868、
    /// L24876–L24877）。
    pub fn template_fields(&self) -> Vec<&FdfField> {
        self.entries
            .iter()
            .flat_map(|page| page.templates.iter())
            .flat_map(|template| template.fields.iter())
            .collect()
    }

    /// Table 243 的 shall：「The Fields and Status entries shall not be
    /// present together with this entry」（raw L24876–L24877）。
    ///
    /// `/Fields` 一侧复用 [`super::super::FdfDictionary::fields_pages_conflict`]
    /// 的既有口径，`/Status` 一侧是本视图新增的同一 shall 的另一半；
    /// 只作诊断，不阻断（读端宽容）。
    pub fn pages_exclusive_conflict(&self) -> bool {
        self.issues
            .iter()
            .any(|issue| matches!(issue, FdfPagesIssue::PagesWithFieldsOrStatus { .. }))
    }
}

/// FDF 页字典（Table 248，raw L25085–L25090）。
#[derive(Debug, Clone, PartialEq)]
pub struct FdfPage {
    /// 数组项原值（畸形项整体保留，不丢弃）。
    pub raw: Value,
    /// 确证后的页字典本体（无法解引用或非字典时为空字典，原值在
    /// [`Self::raw`]）。
    pub dict: Dict,
    /// `/Templates`（**Required**；array of FDF template dictionaries，
    /// Table 249 + L25058–L25059）。缺失不是 `None` 而是空向量 +
    /// [`FdfPagesIssue::PageTemplatesMissing`]：规范把它列为 Required，
    /// 读端仍要把这一页交出去，不能整页丢弃。
    pub templates: Vec<FdfTemplate>,
    /// `/Info`（Optional；dictionary，Table 248 raw L25089–L25090）原值。
    ///
    /// 「An FDF **page information dictionary**」这一术语在 ISO 32000-1
    /// 全文只出现在 L25089 一处，**没有任何条目表**（实测
    /// `grep -c 'page information dictionary'` = 1），属 Adobe 扩展
    /// （待核），故本切片不发明键、整体按原值保留。
    pub info: Option<Value>,
    /// 本页的降级与 shall 违例。
    pub issues: Vec<FdfPagesIssue>,
}

impl FdfPage {
    /// `/Templates` 是否真的缺条目（Table 248 的 Required 违例）。
    pub fn is_missing_templates(&self) -> bool {
        self.dict.get("Templates").is_none()
    }

    /// 按键取页字典条目原值（含未建模条目与 Adobe 扩展）。
    pub fn entry(&self, key: &str) -> Option<&Value> {
        self.dict.get(key)
    }
}

/// FDF 模板字典（Table 249，raw L25113–L25123；描述作用模板的命名页，
/// L25058–L25059）。
#[derive(Debug, Clone, PartialEq)]
pub struct FdfTemplate {
    /// `/Templates` 数组项原值（引用形态时仍是 `Value::Ref`）。
    pub raw: Value,
    /// 确证后的模板字典本体（无法解引用或非字典时为空字典）。
    pub dict: Dict,
    /// `/TRef`（**Required**；named page reference dictionary，
    /// Table 249 raw L25115–L25116 与 L25097–L25098）。
    /// 缺失为 `None` + [`FdfPagesIssue::TemplateReferenceMissing`]。
    pub named_page: Option<FdfNamedPage>,
    /// `/Fields`（Optional；array，元素是**指向 Table 246 FDF 字段字典的
    /// 引用**，raw L25117–L25119「describing the root fields that shall be
    /// imported (those with no ancestors in the field hierarchy)」）。
    /// 逐项复用读端字段树（`super::parse::parse_field_array`），故
    /// `fully_qualified_name` 与 Annex J 的改名口径可直接比较。
    /// 非字典/缺 `/T`/超深度超节点的项不进本表，另记诊断，原值留在
    /// [`Self::dict`]。
    pub fields: Vec<FdfField>,
    /// `/Rename`（Optional；boolean，**Default value: true**；raw
    /// L25120–L25123）。
    ///
    /// 缺省值**不物化**：条目缺席就是 `None`；生效语义（true）由
    /// [`Self::rename_effective`] 给出。非布尔形态（如写成整数）保持
    /// `None` 并记诊断，原值不丢弃。
    pub rename: Option<bool>,
    /// 本模板的降级与 shall 违例。
    pub issues: Vec<FdfPagesIssue>,
}

impl FdfTemplate {
    /// Table 249 `/Rename` 的**生效**值：条目缺席时按规范的
    /// 「Default value: true」（raw L25122–L25123）取 `true`。
    pub fn rename_effective(&self) -> bool {
        self.rename.unwrap_or(true)
    }

    /// 命名页引用的名字（Table 250 `/Name`）；缺失或形态不符为 `None`。
    pub fn name(&self) -> Option<&str> {
        self.named_page
            .as_ref()
            .and_then(|page| page.name.as_deref())
    }

    /// `/Fields` 是否真的缺条目（Optional，缺失合法、不是违例）。
    pub fn has_declared_fields(&self) -> bool {
        self.dict.get("Fields").is_some()
    }

    /// 按键取模板字典条目原值。
    pub fn entry(&self, key: &str) -> Option<&Value> {
        self.dict.get(key)
    }
}

/// FDF 命名页引用字典（Table 250，raw L25124–L25128）。
///
/// Table 246 的 `/APRef`（PDF 1.3，raw L25020–L25023）里 N/R/D 的值也是
/// 这一形态，但那一条目在字段侧仍按原值保留（见
/// [`super::super::FdfField::appearance_reference`]），本视图只覆盖
/// Table 249 `/TRef` 用法。
#[derive(Debug, Clone, PartialEq)]
pub struct FdfNamedPage {
    /// `/TRef` 条目原值（引用形态时仍是 `Value::Ref`）。
    pub raw: Value,
    /// 确证后的引用字典本体（形态不符时为空字典，原值在 [`Self::raw`]）。
    pub dict: Dict,
    /// `/Name`（**Required**；string，raw L25126「The name of the
    /// referenced page」）。仅字面串/十六进制串解码（与 `/Target` 同一
    /// 口径）；非串形态为 `None` + [`FdfPagesIssue::TemplateNameNotString`]，
    /// 原值保留。
    pub name: Option<String>,
    /// `/F`（Optional；file specification，raw L25127–L25128）原值。
    ///
    /// 规范推定「If this entry is absent, it shall be assumed that the
    /// page resides in the associated PDF file」⇒ 不伪造文件规范，
    /// 缺席即 `None`，判定走 [`Self::resides_in_associated_pdf`]。
    /// 本切片只保留原值：7.11 文件规范类型化需要外层 `/F`（Table 243）
    /// 与外部文件可达性，跨文件展开见模块文档缺口 2。
    pub file: Option<Value>,
}

impl FdfNamedPage {
    /// Table 250 `/F` 缺席 ⇒ 该命名页**在关联的 PDF 文件里**
    /// （raw L25127–L25128 的规范推定）。
    pub fn resides_in_associated_pdf(&self) -> bool {
        self.file.is_none()
    }

    /// 按键取引用字典条目原值。
    pub fn entry(&self, key: &str) -> Option<&Value> {
        self.dict.get(key)
    }
}

/// Table 243 的 `/Annots` 条目（Optional; PDF 1.3；raw L24885–L24889）与其
/// 元素（Table 251 的附加条目 `/Page`，raw L25129–L25132）的类型化视图。
#[derive(Debug, Clone, PartialEq)]
pub struct FdfAnnots {
    /// `/Annots` 条目原值（不丢弃；非数组形态时也在此保留）。
    pub raw: Value,
    /// 原值为间接引用（如 `8 0 R`）时的 `(对象号, generation)`。
    pub reference: Option<(i64, i64)>,
    /// 逐项类型化的注解字典，按数组声明序；受单数组上限与全局预算约束。
    pub entries: Vec<FdfAnnotation>,
    /// 数组级与逐注解级的降级与 shall 违例。
    pub issues: Vec<FdfPagesIssue>,
}

impl FdfAnnots {
    /// 已类型化的注解数。
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 是否没有任何已类型化注解。
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Table 251 `/Page` 序数（0 起，raw L25131–L25132）到该页注解下标的
    /// 倒排；`/Page` 缺失或非整型的注解不进入本表（见
    /// [`Self::issues`]）。
    pub fn pages(&self) -> Vec<i64> {
        let mut seen: Vec<i64> = Vec::new();
        for annotation in &self.entries {
            if let Some(page) = annotation.page
                && !seen.contains(&page)
            {
                seen.push(page);
            }
        }
        seen
    }
}

/// FDF 注解字典：一份标准注解字典（Table 169 的类型 + 12.5.2 的公共
/// 条目）加上 Table 251 的**附加必需**条目 `/Page`（raw L25129–L25132）。
///
/// 12.7.7.3.4（L25099–L25101）说 `/Page` 指示「**the page of the source
/// document** to which the annotation is attached」⇒ 与 Table 248 描述
/// 目标文档新页的 `/Pages` 不是同一个页集合，两者的序数不可直接比较；
/// 该交叉核对只作为 [`FdfPagesIssue::AnnotationPageNotInPagesArray`]
/// 的**提示性**诊断给出（非 shall），理由见 `merge.rs` 模块文档。
#[derive(Debug, Clone, PartialEq)]
pub struct FdfAnnotation {
    /// 数组项原值（畸形项整体保留）。
    pub raw: Value,
    /// 确证后的注解字典本体（形态不符时为空字典，原值在 [`Self::raw`]）。
    pub dict: Dict,
    /// `/Page`（**Required for annotations in FDF files**；integer；
    /// raw L25130–L25132）。非整型或缺失为 `None` + 对应诊断，原值保留。
    pub page: Option<i64>,
    /// `/Subtype` 名字（Table 169 的注解类型）。
    pub subtype: Option<String>,
    /// 本注解的降级与 shall 违例。
    pub issues: Vec<FdfPagesIssue>,
}

/// Table 243 明文禁止出现在 FDF `/Annots` 里的六种标准注解类型
/// （raw L24887–L24889「except Link, Movie, Widget, PrinterMark, Screen,
/// and TrapNet」）。
pub const PROHIBITED_FDF_ANNOTATION_TYPES: [&str; 6] = [
    "Link",
    "Movie",
    "Widget",
    "PrinterMark",
    "Screen",
    "TrapNet",
];

impl FdfAnnotation {
    /// `/Subtype` 是否为 Table 243 允许的标注类型。
    ///
    /// `/Subtype` 缺失时返回 `false`（12.5.2 把它列为注解字典公共条目的
    /// Required 项，缺了就无法核对）。
    pub fn subtype_permitted(&self) -> bool {
        match self.subtype.as_deref() {
            Some(name) => !PROHIBITED_FDF_ANNOTATION_TYPES.contains(&name),
            None => false,
        }
    }

    /// Table 251 `/Page` 是否为合法的**序数**（0 起，raw
    /// L25131–L25132「where page 0 is the first page」）：存在、整型且
    /// 非负。
    pub fn page_ordinal_valid(&self) -> bool {
        self.page.is_some_and(|page| page >= 0)
    }

    /// 按键取注解字典条目原值（含 12.5.2 公共条目与未建模扩展）。
    pub fn entry(&self, key: &str) -> Option<&Value> {
        self.dict.get(key)
    }
}
