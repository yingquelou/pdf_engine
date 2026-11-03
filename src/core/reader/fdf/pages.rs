//! FDF **页字典**（Table 248）、**模板字典**（Table 249）、**命名页引用
//! 字典**（Table 250）与 **注解字典**（Table 251）的类型化只读视图，
//! 以及 Annex J 的导入合并（`/Rename`）分析（ISO 32000-1 12.7.7.3.3、
//! 12.7.7.3.4 与信息性 Annex J）。
//!
//! 这四个结构都是 32000-1 **正文**内容（不是 Adobe 扩展），实测依据
//! （`pdftotext -raw docs/PDF32000_2008.pdf -`，行号为 raw 抽取行）：
//!
//! - L24755–L24761（12.7.7.1）「In addition, beginning in PDF 1.3, FDF can
//!   be used to define a container for annotations that are separate from
//!   the PDF document to which they apply」⇒ 注解容器档位 PDF 1.3。
//! - L24874–L24877（Table 243）「Pages array (Optional; PDF 1.3) An array of
//!   FDF page dictionaries … describing pages that shall be added to a PDF
//!   target document. The Fields and Status entries shall not be present
//!   together with this entry」。
//! - L24885–L24889（Table 243）「Annots array (Optional; PDF 1.3) An array of
//!   FDF annotation dictionaries … The array may include annotations of any
//!   of the standard types listed in Table 169 **except Link, Movie, Widget,
//!   PrinterMark, Screen, and TrapNet**」。
//! - L25054–L25057（12.7.7.3.3）「The optional Pages field in an FDF
//!   dictionary (see Table 243) shall contain an array of FDF page
//!   dictionaries (PDF 1.3) describing new pages that shall be added to the
//!   target document. Table 248 shows the contents of this type of
//!   dictionary」。
//! - L25085–L25090（Table 248）`Templates` = **array，Required**；
//!   `Info` = dictionary，Optional。
//! - L25058–L25059 + L25113–L25123（Table 249）`TRef` = **dictionary，
//!   Required**；`Fields` = array，Optional（元素是 Table 246 FDF 字段
//!   字典的引用）；`Rename` = boolean，Optional，**Default value: true**。
//! - L25097–L25098 + L25124–L25128（Table 250）`Name` = **string，Required**
//!   「The name of the referenced page」；`F` = file specification，Optional
//!   「If this entry is absent, it shall be assumed that the page resides in
//!   the associated PDF file」。
//! - L25099–L25101（12.7.7.3.4）「Each annotation dictionary in an FDF file
//!   shall have a Page entry (see Table 251) that shall indicate the page of
//!   the **source** document to which the annotation is attached」；
//!   L25129–L25132（Table 251）`Page` = **integer，Required for annotations
//!   in FDF files**「where page 0 is the first page」。
//! - L25093–L25096（12.7.7.3.3 的 NOTE）「The names of fields imported from
//!   a template can sometimes conflict with those of existing fields in the
//!   target document … if the same template page is imported more than once
//!   or if two different templates have fields with the same names. The
//!   Rename flag does not define a renaming algorithm (see Annex J)」；
//!   L37500–L37516（Annex J，**informative**）给出改名形态
//!   「prepending a page number, a template name, and an ordinal number to
//!   the field name」与示例 `P5.Template_0.`。⇒ 合并分析只做**信息性**
//!   报告，不做规范性重命名（规范自己就说 Rename 不定义算法）。
//!
//! 与 12.7.6（L24742–L24749）PDF 名字字典的 `Pages` 树**同名不同物**：
//! 那一处映射的是**已有**文档内的页，本处的 FDF `/Pages` 描述**要加入目标
//! 文档的新页**；两者的 `/Page`（Table 251，源码文档序数）也不可比。
//!
//! 类型化策略（沿用同目录 `embedded.rs` 的既有口径）：
//! - 以**方法**而非 `FdfDictionary` 字段暴露（[`Document::fdf_pages`]、
//!   [`Document::fdf_annots`]、[`Document::fdf_import_merge`]）：加字段要
//!   改 `FdfDictionary` 的全字段字面构造点
//!   （`tests/main/fdf/{embedded,javascript,write}.rs`），那些文件不在
//!   本车道写集内；且类型化需要 `&Document` 解引用上下文，惰性方法形态
//!   与之匹配。
//! - **可选条目不物化缺省**：Table 249 `/Rename` 缺席时
//!   [`FdfTemplate::rename`] 保持 `None`，缺省语义（true）由
//!   [`FdfTemplate::rename_effective`] 单独给出；Table 250 `/F` 缺席由
//!   [`FdfNamedPage::resides_in_associated_pdf`] 表达「在关联 PDF 文件里」
//!   的规范推定（L25127–L25128），不伪造文件规范。
//! - **未建模条目原样保留**：每个结构都带 `dict` 与 `raw`。特别是
//!   Table 248 `/Info`「An FDF page information dictionary」（L25089）——
//!   「FDF page information dictionary」这个术语在 32000-1 全文**只出现
//!   这一次**、没有任何条目表，属 Adobe 扩展（待核），故整体按原值保留。
//! - **shall 违例走类型化诊断表**（[`FdfPagesIssue`]），绝不让整份 FDF
//!   解析失败，手法与 `reader/linearization/issues.rs`、
//!   `reader/navigation/requirements/issues.rs` 一致。
//! - **不可信输入防护**：数组长度、模板数、注解数与整棵 `/Pages` 展开
//!   共享的总条目预算（[`PagesBudget`]）多重上限；模板 `/Fields` 复用
//!   读端字段树的 [`MAX_FDF_FIELD_DEPTH`] / [`MAX_FDF_FIELD_NODES`] 与
//!   环检测（[`super::parse::Walker`]），路径性嵌套不会耗尽栈。
//! - **版本门禁只报告不拒绝**：`/Pages` 与 `/Annots` 都是 PDF 1.3 条目，
//!   声明版本低于 1.3 时照常类型化、附一条诊断；声明版本优先序与
//!   `embedded.rs` 相同（目录 `/Version`（Table 242，L24841–L24849）→
//!   `%FDF-x.y` 文件头 → [`Document::source_version`]）。禁止新增
//!   [`PdfFeature`](crate::core::types::PdfFeature) 变体，故直接比较
//!   `PdfVersion` 常量。
//!
//! 剩余缺口（如实记录，本车道不实现）：
//! 1. FDF `/Target` 之外的 Adobe FDF 2.0 扩展、Table 248 `/Info` 的
//!    条目语义（32000-1 未定义）与 Annex J 的**实际**改名算法
//!    （informative 且规范明说 Rename 不定义算法）都不类型化。
//! 2. 模板 `/Fields` 引用的字段字典若写在**另一份** FDF 文件里
//!    （经 Table 250 `/F` 外部文件）不可解引用：本视图只做本地
//!    `Document` 内的解析，外部文件留原值 + 诊断。
//! 3. 类型化结果与 `FdfDictionary` 的 `pages`/`annots` 原值字段是两条
//!    互不替代的通道（同 `embedded.rs` 缺口 2）。

use crate::core::types::Document;

use super::FdfError;

mod expand;
mod issues;
mod merge;
mod view;

type Result<T> = std::result::Result<T, FdfError>;

/// `/Pages`、`/Annots`、`/Templates`、模板 `/Fields` 等**单个数组**一次展开
/// 的条目上限（**库策略**：数组长度是不可信输入，规范未给上限）。
pub(crate) const MAX_FDF_PAGES_ARRAY_ENTRIES: usize = 512;
/// **一次顶层 `/Pages` + `/Annots` 展开**累计类型化条目的全局预算
/// （库策略，跨所有嵌套层共享；与 `embedded::ExpansionBudget` 同一动机：
/// 单数组上限只约束一个数组，页 × 模板 × 字段相乘仍是指数级）。
pub(crate) const MAX_FDF_PAGES_TOTAL_ENTRIES: usize = 1_024;

/// 一次 `/Pages` + `/Annots` 展开的全局条目预算（跨层共享，见
/// [`MAX_FDF_PAGES_TOTAL_ENTRIES`]）。
#[derive(Debug, Default)]
pub(crate) struct PagesBudget {
    used: usize,
}

impl PagesBudget {
    /// 为本条目申请一个全局名额；预算耗尽返回 `false`，调用方记诊断并
    /// 停止展开所在数组的剩余项（已类型化的层不受影响）。
    pub(crate) fn admit(&mut self) -> bool {
        if self.used >= MAX_FDF_PAGES_TOTAL_ENTRIES {
            return false;
        }
        self.used += 1;
        true
    }
}

pub use issues::FdfPagesIssue;
pub use merge::{FdfFieldNameConflict, FdfImportMerge, FdfTemplateImport};
pub use view::{
    FdfAnnotation, FdfAnnots, FdfNamedPage, FdfPage, FdfPages, FdfTemplate,
    PROHIBITED_FDF_ANNOTATION_TYPES,
};

impl Document {
    /// FDF `/Pages` 数组的类型化读视图（Table 243 + Table 248/249/250；
    /// Optional; PDF 1.3）。
    ///
    /// 非 FDF 文件或该条目缺失返回 `Ok(None)`；条目存在时**总是**返回
    /// `Some`（形态不符只记 [`FdfPagesIssue`] 诊断，不报错），口径与
    /// [`Document::fdf_differences`] 一致。
    pub fn fdf_pages(&self) -> Result<Option<FdfPages>> {
        let Some(fdf) = self.fdf()? else {
            return Ok(None);
        };
        let mut budget = PagesBudget::default();
        Ok(expand::pages_view(self, &fdf, &mut budget))
    }

    /// FDF `/Annots` 数组的类型化读视图（Table 243 + Table 251；
    /// Optional; PDF 1.3）。每项都按 12.7.7.3.4 检查必填 `/Page`。
    ///
    /// 降级策略同 [`Document::fdf_pages`]。
    pub fn fdf_annots(&self) -> Result<Option<FdfAnnots>> {
        let Some(fdf) = self.fdf()? else {
            return Ok(None);
        };
        let mut budget = PagesBudget::default();
        Ok(expand::annots_view(self, &fdf, &mut budget))
    }

    /// Annex J（informative）口径的 FDF **导入合并**报告：把 `/Pages` 的
    /// 模板字段全限定名按声明序（页序 + 页内模板序）汇总，列出**同名
    /// 冲突**、各模板的 `/Rename` 生效值，以及 Table 243 的 `/Pages` 互斥
    /// shall 与 Table 251 `/Page` 序数的类型化诊断。
    ///
    /// 规范明文「The Rename flag does not
    /// define a renaming algorithm」（L25096），故本报告**不**产出重命名
    /// 后的字段树，只给出 Annex J J.2 的前缀**建议**形态
    /// （[`FdfTemplateImport::rename_prefix`]）。
    /// `/Pages` 与 `/Annots` 都缺失时返回 `Ok(None)`。
    pub fn fdf_import_merge(&self) -> Result<Option<FdfImportMerge>> {
        let Some(fdf) = self.fdf()? else {
            return Ok(None);
        };
        Ok(merge::import_merge(self, &fdf))
    }
}
