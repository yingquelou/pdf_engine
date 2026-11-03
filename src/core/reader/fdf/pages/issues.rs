//! FDF 页/注解类型化视图的 **shall 违例与降级记录**（Table 243/248/249/
//! 250/251 与信息性 Annex J）：每个变体的 `Display` 自带条款依据与 raw
//! 抽取行号，手法同 `reader/linearization/issues.rs` 与
//! `reader/navigation/requirements/issues.rs`。
//!
//! 这些诊断**都是非致命**的：读端宽容，任何一条都不阻断解析，原值一律
//! 保留。[`FdfPagesIssue::is_advisory`] 区分「规范 shall 违例」与
//! 「提示性交叉核对」——后者不是违例（如 Table 251 的 `/Page` 指向
//! **源文档**页，而 Table 248 的 `/Pages` 描述**目标文档**新页，两者
//! 序数本就不可比）。

use std::fmt;

use super::MAX_FDF_PAGES_TOTAL_ENTRIES;

/// FDF `/Pages`（Table 248/249/250）与 `/Annots`（Table 251）类型化视图的
/// 诊断项。
///
/// 变体保留**原始值**（找到的类型名、未定义的名字、错误信息）与位置
/// （`page` 是 `/Pages` 数组下标，`template` 是该页 `/Templates` 数组下标，
/// `index` 是 `/Annots` 数组下标），不静默丢弃。
#[derive(Debug, Clone, PartialEq)]
pub enum FdfPagesIssue {
    /// Table 243（L24874–L24877 / L24885–L24889）：`/Pages` 与 `/Annots`
    /// 都声明为 array；解析结果不是数组时整体降级，原值仍在
    /// [`super::FdfPages::raw`]。
    EntryNotArray {
        key: &'static str,
        found: &'static str,
    },
    /// 条目的间接引用无法解析（悬空引用；FDF 交叉引用表可选，
    /// 12.7.7.1 L24763–L24764）。
    EntryUnresolvable { key: &'static str, message: String },
    /// 单个数组超出 [`super::MAX_FDF_PAGES_ARRAY_ENTRIES`]（库策略，
    /// 数组长度是不可信输入），尾部未类型化。
    ArrayTruncated { key: &'static str, limit: usize },
    /// 整棵 `/Pages` + `/Annots` 展开耗尽
    /// [`super::MAX_FDF_PAGES_TOTAL_ENTRIES`] 全局条目预算（库策略）。
    BudgetExhausted { key: &'static str },
    /// Table 243 的 shall：「The Fields and Status entries shall not be
    /// present together with this entry」（`/Pages`，L24876–L24877），
    /// 以及 `/Fields` 一侧的同一条 shall（L24865–L24868）。
    PagesWithFieldsOrStatus { fields: bool, status: bool },
    /// Table 242/243 的 PDF 1.3 档位：`/Pages` 在声明版本低于 1.3 的文件
    /// 里出现（仍照常读取）。
    PagesVersionGate { declared: String },
    /// Table 243 的 PDF 1.3 档位：`/Annots` 在声明版本低于 1.3 的文件里
    /// 出现（仍照常读取；容器语义见 12.7.7.1 L24760）。
    AnnotsVersionGate { declared: String },
    /// Table 248：`/Pages` 数组的一项不是字典。
    PageNotDictionary { page: usize, found: &'static str },
    /// Table 248：页字典的间接引用无法解析。
    PageUnresolvable { page: usize, message: String },
    /// Table 248：`Templates` 是 **Required** 条目却缺失（L25087）。
    PageTemplatesMissing { page: usize },
    /// Table 248：`Templates` 存在但不是数组。
    PageTemplatesNotArray { page: usize, found: &'static str },
    /// Table 248/249：`Templates` 数组的一项不是字典。
    PageTemplateNotDictionary {
        page: usize,
        template: usize,
        found: &'static str,
    },
    /// Table 249：模板字典的间接引用无法解析。
    PageTemplateUnresolvable {
        page: usize,
        template: usize,
        message: String,
    },
    /// Table 249：`TRef` 是 **Required** 条目却缺失（L25115–L25116，
    /// 另见 L25097–L25098「The TRef entry … shall hold a named page
    /// reference dictionary」）。
    TemplateReferenceMissing { page: usize, template: usize },
    /// Table 249：`TRef` 存在但不是字典。
    TemplateReferenceNotDictionary {
        page: usize,
        template: usize,
        found: &'static str,
    },
    /// Table 249：`TRef` 的间接引用无法解析。
    TemplateReferenceUnresolvable {
        page: usize,
        template: usize,
        message: String,
    },
    /// Table 250：`Name` 是 **Required** 条目却缺失（L25126）。
    TemplateNameMissing { page: usize, template: usize },
    /// Table 250：`Name` 存在但不是可解码的文本串。
    TemplateNameNotString {
        page: usize,
        template: usize,
        found: &'static str,
    },
    /// Table 249：`Fields` 存在但不是数组（Optional 条目，形态不符）。
    TemplateFieldsNotArray {
        page: usize,
        template: usize,
        found: &'static str,
    },
    /// Table 249 `/Fields` 的某一项无法类型化为 Table 246 字段字典
    /// （非字典、缺必填 `/T`、超深度/超节点上限或命中引用环）；
    /// `message` 携带读端字段解析的原始错误，数组原值不丢弃。
    TemplateFieldSkipped {
        page: usize,
        template: usize,
        field: usize,
        message: String,
    },
    /// Table 249：`Rename` 存在但不是布尔（缺省语义 true 只在条目**缺席**
    /// 时生效，形态不符时不猜值）。
    TemplateRenameNotBoolean {
        page: usize,
        template: usize,
        found: &'static str,
    },
    /// Table 251：`/Annots` 数组的一项不是字典。
    AnnotationNotDictionary { index: usize, found: &'static str },
    /// Table 251：注解字典的间接引用无法解析。
    AnnotationUnresolvable { index: usize, message: String },
    /// Table 251 / 12.7.7.3.4（L25099–L25101）：`Page` 是 FDF 注解字典的
    /// **Required** 条目却缺失。
    AnnotationPageMissing {
        index: usize,
        subtype: Option<String>,
    },
    /// Table 251：`Page` 存在但不是整型（L25130–L25132 声明 integer）。
    AnnotationPageNotInteger { index: usize, found: &'static str },
    /// Table 251：`Page` 序数为负（「where page 0 is the first page」，
    /// L25131–L25132）。
    AnnotationPageNegative { index: usize, page: i64 },
    /// 12.5.2：注解字典的 `Subtype` 是公共必需条目；FDF 侧缺失时无法
    /// 核对 Table 243 的类型白名单。
    AnnotationSubtypeMissing { index: usize },
    /// Table 243（L24887–L24889）：`/Annots` 里出现被明文排除的注解类型
    /// （Link、Movie、Widget、PrinterMark、Screen、TrapNet）。
    AnnotationSubtypeProhibited { index: usize, subtype: String },
    /// Table 248 的 `/Pages` 数组未声明那么多页，但某条注解的 Table 251
    /// `/Page` 指向了那个序数。**提示性**诊断，非 shall 违例：
    /// `/Page` 指的是**源文档**页（L25100–L25101），`/Pages` 描述要加入
    /// **目标文档**的新页（L25056），两者本就不必一致；仅在调用方把
    /// 二者当同一页集合时才有意义。
    AnnotationPageNotInPagesArray {
        index: usize,
        page: i64,
        declared_pages: usize,
    },
    /// Annex J（informative）J.2：两个模板导入的字段**全限定名**冲突
    /// （L25093–L25095 的 NOTE 描述的正是要冲突）。`sources` 是
    /// `(页下标, 页内模板下标)` 列表，按声明序。
    ///
    /// 非 shall 违例：规范说「The Rename flag does not define a renaming
    /// algorithm」（L25096），且 J.2 补充「Each time the FDF file provides
    /// attributes for a given field name, all fields with that name shall
    /// be updated」（L37509–L37511）。
    TemplateFieldNameConflict {
        name: String,
        sources: Vec<(usize, usize)>,
    },
}

impl FdfPagesIssue {
    /// 是否只是**提示性**交叉核对（非规范 shall 违例、非降级）。
    pub fn is_advisory(&self) -> bool {
        matches!(
            self,
            Self::AnnotationPageNotInPagesArray { .. } | Self::TemplateFieldNameConflict { .. }
        )
    }

    /// 是否超出库策略上限（不可信输入防护），而非文件本身不合规范。
    pub fn is_limit(&self) -> bool {
        matches!(
            self,
            Self::ArrayTruncated { .. } | Self::BudgetExhausted { .. }
        )
    }

    /// 是否 **Required** 条目缺失（Table 248 `/Templates`、Table 249
    /// `/TRef`、Table 250 `/Name`、Table 251 `/Page`）：这四条是本次切片
    /// 覆盖的表里全部的 shall 级必需条目。
    pub fn is_required_entry_missing(&self) -> bool {
        matches!(
            self,
            Self::PageTemplatesMissing { .. }
                | Self::TemplateReferenceMissing { .. }
                | Self::TemplateNameMissing { .. }
                | Self::AnnotationPageMissing { .. }
        )
    }
}

impl fmt::Display for FdfPagesIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EntryNotArray { key, found } => write!(
                f,
                "FDF /{key} resolved to {found} but Table 243 (raw L24874-L24877, L24885-L24889) declares an array; raw value kept"
            ),
            Self::EntryUnresolvable { key, message } => write!(
                f,
                "FDF /{key} reference could not be resolved: {message}; raw value kept"
            ),
            Self::ArrayTruncated { key, limit } => write!(
                f,
                "FDF /{key} holds more than {limit} entries; the tail was skipped (library guard, not a spec rule)"
            ),
            Self::BudgetExhausted { key } => write!(
                f,
                "FDF /{key} expansion stopped: the {MAX_FDF_PAGES_TOTAL_ENTRIES}-entry global budget for this expansion is used up; later entries are not typed and their raw values are kept"
            ),
            Self::PagesWithFieldsOrStatus { fields, status } => write!(
                f,
                "FDF /Pages shall not be present together with {} (Table 243 raw L24865-L24877); read leniently",
                match (fields, status) {
                    (true, true) => "/Fields and /Status",
                    (true, false) => "/Fields",
                    (false, true) => "/Status",
                    (false, false) => "neither",
                }
            ),
            Self::PagesVersionGate { declared } => write!(
                f,
                "FDF /Pages is a PDF 1.3 entry (Table 243 raw L24874) but the file declares {declared}; it is still read leniently"
            ),
            Self::AnnotsVersionGate { declared } => write!(
                f,
                "FDF /Annots is a PDF 1.3 entry (Table 243 raw L24885) but the file declares {declared}; it is still read leniently"
            ),
            Self::PageNotDictionary { page, found } => write!(
                f,
                "FDF /Pages entry {page} is {found} but Table 248 (raw L25085) is a dictionary; raw value kept"
            ),
            Self::PageUnresolvable { page, message } => write!(
                f,
                "FDF /Pages entry {page} could not be resolved: {message}; raw value kept"
            ),
            Self::PageTemplatesMissing { page } => write!(
                f,
                "FDF page dictionary {page} is missing /Templates, which Table 248 (raw L25087) marks Required"
            ),
            Self::PageTemplatesNotArray { page, found } => write!(
                f,
                "FDF page dictionary {page} /Templates is {found} but Table 248 (raw L25087) declares an array; raw entry kept"
            ),
            Self::PageTemplateNotDictionary {
                page,
                template,
                found,
            } => write!(
                f,
                "FDF page {page} template {template} is {found} but Table 249 (raw L25113) is a dictionary; raw value kept"
            ),
            Self::PageTemplateUnresolvable {
                page,
                template,
                message,
            } => write!(
                f,
                "FDF page {page} template {template} could not be resolved: {message}; raw value kept"
            ),
            Self::TemplateReferenceMissing { page, template } => write!(
                f,
                "FDF template {page}:{template} is missing /TRef, which Table 249 (raw L25115-L25116, L25097-L25098) marks Required"
            ),
            Self::TemplateReferenceNotDictionary {
                page,
                template,
                found,
            } => write!(
                f,
                "FDF template {page}:{template} /TRef is {found} but Table 249 declares a named page reference dictionary; raw value kept"
            ),
            Self::TemplateReferenceUnresolvable {
                page,
                template,
                message,
            } => write!(
                f,
                "FDF template {page}:{template} /TRef could not be resolved: {message}; raw value kept"
            ),
            Self::TemplateNameMissing { page, template } => write!(
                f,
                "FDF template {page}:{template} /TRef is missing /Name, which Table 250 (raw L25126) marks Required"
            ),
            Self::TemplateNameNotString {
                page,
                template,
                found,
            } => write!(
                f,
                "FDF template {page}:{template} /TRef /Name is {found} but Table 250 (raw L25126) declares a string; raw value kept"
            ),
            Self::TemplateFieldsNotArray {
                page,
                template,
                found,
            } => write!(
                f,
                "FDF template {page}:{template} /Fields is {found} but Table 249 (raw L25117) declares an array; raw entry kept"
            ),
            Self::TemplateFieldSkipped {
                page,
                template,
                field,
                message,
            } => write!(
                f,
                "FDF template {page}:{template} /Fields entry {field} is not a typed Table 246 field: {message}; raw array kept"
            ),
            Self::TemplateRenameNotBoolean {
                page,
                template,
                found,
            } => write!(
                f,
                "FDF template {page}:{template} /Rename is {found} but Table 249 (raw L25120) declares a boolean; the default (true) is not applied to a malformed entry and the raw value is kept"
            ),
            Self::AnnotationNotDictionary { index, found } => write!(
                f,
                "FDF /Annots entry {index} is {found} but an FDF annotation is a dictionary (12.7.7.3.4 raw L25099); raw value kept"
            ),
            Self::AnnotationUnresolvable { index, message } => write!(
                f,
                "FDF /Annots entry {index} could not be resolved: {message}; raw value kept"
            ),
            Self::AnnotationPageMissing { index, subtype } => write!(
                f,
                "FDF annotation {index} (subtype {:?}) is missing /Page, required for annotations in FDF files by Table 251 (raw L25099-L25101, L25130)",
                subtype.as_deref()
            ),
            Self::AnnotationPageNotInteger { index, found } => write!(
                f,
                "FDF annotation {index} /Page is {found} but Table 251 (raw L25130) declares an integer; raw value kept"
            ),
            Self::AnnotationPageNegative { index, page } => write!(
                f,
                "FDF annotation {index} /Page is {page}, but Table 251 (raw L25131-L25132) counts ordinals from 0 for the first page"
            ),
            Self::AnnotationSubtypeMissing { index } => write!(
                f,
                "FDF annotation {index} has no /Subtype, which 12.5.2 lists among the entries common to all annotations; the Table 243 type whitelist could not be checked"
            ),
            Self::AnnotationSubtypeProhibited { index, subtype } => write!(
                f,
                "FDF annotation {index} has /Subtype /{subtype}, which Table 243 (raw L24887-L24889) excludes from FDF /Annots"
            ),
            Self::AnnotationPageNotInPagesArray {
                index,
                page,
                declared_pages,
            } => write!(
                f,
                "advisory: FDF annotation {index} claims /Page {page} but the /Pages array types {declared_pages} page(s); this is not a violation - Table 251 addresses source-document pages (raw L25100-L25101) while Table 248 addresses pages added to the target document (raw L25056)"
            ),
            Self::TemplateFieldNameConflict { name, sources } => {
                write!(
                    f,
                    "advisory: field name \"{name}\" is imported by {} template(s)",
                    sources.len()
                )?;
                for (position, (page, template)) in sources.iter().enumerate() {
                    write!(
                        f,
                        "{}{}:{template}",
                        if position == 0 { " " } else { ", " },
                        page
                    )?;
                }
                write!(
                    f,
                    " (Table 249 NOTE raw L25093-L25096; the spec defines no renaming algorithm - see Annex J)"
                )
            }
        }
    }
}
