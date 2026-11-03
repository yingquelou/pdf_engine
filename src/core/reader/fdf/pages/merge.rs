//! FDF **导入合并**（import merge）报告：Table 248 `/Pages` + Table 249
//! `/TRef`/`/Fields`/`/Rename` 与 Table 251 `/Page` 的交叉分析，依据是
//! 12.7.7.3.3 的 NOTE（raw L25093–L25096）与**信息性** Annex J
//! 「FDF Rename Flag Implementation Example」（raw L37500–L37516）。
//!
//! 为什么这一半属于本切片：矩阵缺口写作「Table 248–251 的 `/Pages`
//! `/Annots` **与导入合并**」，而 32000-1 里唯一把这几张表和「合并」联系
//! 起来的正文就是这两处：
//!
//! - L25093–L25095（Table 249 所在小节的 NOTE）「The names of fields
//!   imported from a template can sometimes conflict with those of
//!   existing fields in the target document. This can occur, for example,
//!   if **the same template page is imported more than once** or if **two
//!   different templates have fields with the same names**」；
//! - L25096「The Rename flag **does not define a renaming algorithm**
//!   (see Annex J)」；
//! - Annex J.2（**informative**）L37507–L37511「If the Rename flag in the
//!   FDF template dictionary is true, fields with such conflicting names
//!   shall be renamed to guarantee their uniqueness. If Rename is false,
//!   the fields shall not be renamed; this results in multiple fields with
//!   the same name in the target document. **Each time the FDF file
//!   provides attributes for a given field name, all fields with that name
//!   shall be updated**」；
//! - Annex J.2 L37511–L37516 给出一种**实现示例**形态：「prepending a page
//!   number, a template name, and an ordinal number to the field name.
//!   The ordinal number corresponds to the order in which the template is
//!   applied to a page, with 0 being the first template specified for the
//!   page」，示例 `P5.Template_0.`。
//!
//! ⇒ 因此本模块**只产出报告**：冲突、每个模板的生效 `Rename`、以及
//! Annex J 形态的前缀**建议**（[`FdfTemplateImport::rename_prefix`]）。
//! 它不重写字段树、不声称规范性（Annex J 是 informative，且规范自己说
//! Rename 不定义算法）。页序数基同样**不钉死**：J.2 的示例把「fifth
//! page」写成 `P5`，而 Table 251（L25131–L25132）的序数从 0 起算，
//! 加上 `/Pages` 描述的是**要加入目标文档的新页**（L25056）、其在目标
//! 文档里的最终序数并不由 FDF 文件决定，故前缀按 `/Pages` 数组下标
//! （0 起）给出，属**建议值**、待核。
//!
//! `/Annots` 一侧同理：Table 251 的 `/Page` 指的是**源文档**页
//! （L25100–L25101），与 `/Pages`（目标文档新页）不是同一个页集合，
//! 所以「注解指向的页没被 `/Pages` 声明」只作为**提示性**诊断
//! [`FdfPagesIssue::AnnotationPageNotInPagesArray`] 给出，且只在文件
//! 确实声明了 `/Pages` 时才评估。

use crate::core::types::Document;

use super::super::{FdfDocument, FdfField};
use super::expand;
use super::issues::FdfPagesIssue;
use super::{FdfAnnots, FdfPages, FdfTemplate, PagesBudget};

/// Annex J 口径的 FDF 导入合并报告（[`Document::fdf_import_merge`]）。
#[derive(Debug, Clone, PartialEq)]
pub struct FdfImportMerge {
    /// 每个模板一条，按 `/Pages` 声明序 + 页内 `/Templates` 声明序。
    pub templates: Vec<FdfTemplateImport>,
    /// 跨模板（含同名模板重复导入）的字段全限定名冲突，按名字首次
    /// 出现的声明序。
    pub conflicts: Vec<FdfFieldNameConflict>,
    /// 与合并相关的 shall 违例、降级与提示性诊断：来自 `/Pages` 与
    /// `/Annots` 两个视图的全部诊断（含 Table 243 的 `/Pages` 互斥
    /// shall 与 Table 251 的 `/Page` 检查）。
    pub issues: Vec<FdfPagesIssue>,
}

impl FdfImportMerge {
    /// 只取**非提示性**诊断（规范 shall 违例与降级/上限记录）。
    pub fn violations(&self) -> Vec<&FdfPagesIssue> {
        self.issues
            .iter()
            .filter(|issue| !issue.is_advisory())
            .collect()
    }

    /// 只取**提示性**交叉核对（`AnnotationPageNotInPagesArray` 与
    /// `TemplateFieldNameConflict`）。
    pub fn advisories(&self) -> Vec<&FdfPagesIssue> {
        self.issues
            .iter()
            .filter(|issue| issue.is_advisory())
            .collect()
    }

    /// 按页下标取该页全部模板的导入条目。
    pub fn page(&self, page: usize) -> Vec<&FdfTemplateImport> {
        self.templates
            .iter()
            .filter(|entry| entry.page == page)
            .collect()
    }

    /// 某个全限定名是否被任一模板导入。
    pub fn imports(&self, fully_qualified_name: &str) -> bool {
        self.templates
            .iter()
            .any(|entry| entry.fields.iter().any(|name| name == fully_qualified_name))
    }
}

/// 一个模板（Table 249）的导入条目。
#[derive(Debug, Clone, PartialEq)]
pub struct FdfTemplateImport {
    /// `/Pages` 数组下标（0 起）。
    pub page: usize,
    /// 该页 `/Templates` 数组下标，即 Annex J L37512–L37513 所说的
    /// 「ordinal number corresponds to the order in which the template is
    /// applied to a page, with 0 being the first template specified for
    /// the page」。
    pub template: usize,
    /// Table 250 `/Name`（Required 的命名页名字，L25126）；形态不符或缺失
    /// 时为 `None`（此时 Annex J 的前缀无法给出）。
    pub name: Option<String>,
    /// Table 249 `/Rename` 的**生效**值：条目缺席时按规范缺省
    /// 「Default value: true」（L25122–L25123）取 `true`。
    pub rename: bool,
    /// Annex J J.2 形态的改名**建议**前缀
    /// （`P<page>.<name>_<template>.`）：仅当本模板 `rename` 生效且
    /// `/Name` 可解码时给出，否则 `None`。
    ///
    /// **不是**规范输出：L25096 明说 Rename 不定义算法，Annex J 是
    /// informative 的「Implementation Example」。
    pub rename_prefix: Option<String>,
    /// 本模板导入的字段全限定名（Table 249 `/Fields` 的根字段**及其**
    /// `/Kids` 后代，声明序，同模板内去重）。
    pub fields: Vec<String>,
}

/// 一个被多个模板导入的全限定名（Annex J L37504–L37506 的
/// 「name conflicts with existing fields」在 FDF 文件内部可见的那一半）。
#[derive(Debug, Clone, PartialEq)]
pub struct FdfFieldNameConflict {
    /// 冲突的全限定名。
    pub name: String,
    /// 导入该名的模板位置 `(页下标, 页内模板下标)`，按声明序。
    pub sources: Vec<(usize, usize)>,
    /// 是否**所有**涉及模板的 `Rename` 都生效（此时 Annex J J.2 说这些
    /// 字段「shall be renamed to guarantee their uniqueness」，
    /// L37507–L37509）。只要有一个不生效，合并结果就是目标文档里
    /// 多个同名并存（L37509–L37510）。
    pub all_rename: bool,
}

/// 装配导入合并报告；`/Pages` 与 `/Annots` 都缺失时返回 `None`。
pub(super) fn import_merge(document: &Document, fdf: &FdfDocument) -> Option<FdfImportMerge> {
    // 两个数组共享一次调用的全局条目预算（与 `pages.rs` 的
    // `MAX_FDF_PAGES_TOTAL_ENTRIES` 同一动机）。
    let mut budget = PagesBudget::default();
    let pages = expand::pages_view(document, fdf, &mut budget);
    let annots = expand::annots_view(document, fdf, &mut budget);
    if pages.is_none() && annots.is_none() {
        return None;
    }

    let mut issues = Vec::new();
    let mut templates = Vec::new();
    if let Some(view) = &pages {
        issues.extend(view.issues.iter().cloned());
        for (page, entry) in view.entries.iter().enumerate() {
            issues.extend(entry.issues.iter().cloned());
            for (template, item) in entry.templates.iter().enumerate() {
                issues.extend(item.issues.iter().cloned());
                templates.push(template_import(page, template, item));
            }
        }
    }
    let mut named_conflicts = Vec::new();
    if let Some(view) = &annots {
        issues.extend(view.issues.iter().cloned());
        for annotation in &view.entries {
            issues.extend(annotation.issues.iter().cloned());
        }
        // 提示性交叉核对：只在文件确实声明了 /Pages 时评估（否则任何
        // /Page 序数都会被判「未声明」，是噪声而非信息）。
        if let Some(pages) = pages.as_ref() {
            named_conflicts.extend(annotation_page_advisories(view, pages));
        }
    }
    issues.extend(named_conflicts);

    let conflicts = collect_conflicts(&templates);
    // Annex J 的模板重名（同一 /Pages 数组里两次导入同一命名页）也是
    // NOTE 明说的冲突形态（L25094「the same template page is imported
    // more than once」）。
    issues.extend(template_name_repeats(&templates));
    Some(FdfImportMerge {
        templates,
        conflicts,
        issues,
    })
}

/// `/Annots` 的 `/Page` 序数落在 `/Pages` 声明范围之外：提示性诊断。
fn annotation_page_advisories(annots: &FdfAnnots, pages: &FdfPages) -> Vec<FdfPagesIssue> {
    let declared = pages.entries.len();
    let declared_as_ordinal = i64::try_from(declared).unwrap_or(i64::MAX);
    annots
        .entries
        .iter()
        .enumerate()
        .filter_map(|(index, annotation)| {
            let ordinal = annotation.page?;
            (ordinal >= declared_as_ordinal).then_some(
                FdfPagesIssue::AnnotationPageNotInPagesArray {
                    index,
                    page: ordinal,
                    declared_pages: declared,
                },
            )
        })
        .collect()
}

/// 一个模板的导入条目（含 Annex J J.2 前缀建议）。
fn template_import(page: usize, template: usize, entry: &FdfTemplate) -> FdfTemplateImport {
    let name = entry.name().map(str::to_owned);
    let rename = entry.rename_effective();
    let rename_prefix = match (rename, name.as_deref()) {
        (true, Some(name)) => Some(format!("P{page}.{name}_{template}.")),
        (true, None) => None,
        (false, _) => None,
    };
    FdfTemplateImport {
        page,
        template,
        name,
        rename,
        rename_prefix,
        fields: imported_names(entry),
    }
}

/// 模板名重复导入（NOTE L25093–L25095 的第一种形态）：按命名页名字汇总。
fn template_name_repeats(templates: &[FdfTemplateImport]) -> Vec<FdfPagesIssue> {
    let mut seen: Vec<(&str, Vec<(usize, usize)>)> = Vec::new();
    for entry in templates {
        let Some(name) = entry.name.as_deref() else {
            continue;
        };
        let position = (entry.page, entry.template);
        match seen.iter_mut().find(|(known, _)| *known == name) {
            Some((_, positions)) => positions.push(position),
            None => seen.push((name, vec![position])),
        }
    }
    seen.into_iter()
        .filter(|(_, positions)| positions.len() > 1)
        .map(
            |(name, positions)| FdfPagesIssue::TemplateFieldNameConflict {
                name: format!("template \"{name}\""),
                sources: positions,
            },
        )
        .collect()
}

/// 模板导入的全限定名：根字段（Table 249 `/Fields`）及其 `/Kids` 后代，
/// 声明序、模板内去重（Annex J L37509–L37511「all fields with that name
/// shall be updated」⇒ 同名在一次导入里只算一个来源）。
fn imported_names(entry: &FdfTemplate) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for field in &entry.fields {
        collect_names(field, &mut names);
    }
    names
}

/// 深度优先收集全限定名；`FdfField::kids` 在解析期已受
/// `MAX_FDF_FIELD_DEPTH`（64）约束，故这里的递归有界。
fn collect_names(field: &FdfField, names: &mut Vec<String>) {
    if !names.contains(&field.fully_qualified_name) {
        names.push(field.fully_qualified_name.clone());
    }
    for kid in &field.kids {
        collect_names(kid, names);
    }
}

/// 汇总跨模板的同名冲突（NOTE L25095「two different templates have
/// fields with the same names」；按名字首次出现的声明序）。
fn collect_conflicts(templates: &[FdfTemplateImport]) -> Vec<FdfFieldNameConflict> {
    let mut order: Vec<String> = Vec::new();
    let mut sources: Vec<Vec<(usize, usize)>> = Vec::new();
    let mut all_rename: Vec<bool> = Vec::new();
    for entry in templates {
        let position = (entry.page, entry.template);
        for name in &entry.fields {
            match order.iter().position(|existing| existing == name) {
                Some(index) => {
                    sources[index].push(position);
                    all_rename[index] &= entry.rename;
                }
                None => {
                    order.push(name.clone());
                    sources.push(vec![position]);
                    all_rename.push(entry.rename);
                }
            }
        }
    }
    order
        .into_iter()
        .zip(sources)
        .zip(all_rename)
        .filter(|((_, sources), _)| sources.len() > 1)
        .map(|((name, sources), rename)| FdfFieldNameConflict {
            name,
            sources,
            all_rename: rename,
        })
        .collect()
}
