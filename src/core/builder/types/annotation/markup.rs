//! 标记类注解的通用附加条目（ISO 32000-1 12.5.6.2，Table 170）：主题
//! `/Subj`、所回复注解 `/IRT` 与回复类型 `/RT` 的构建态、归属与依赖校验。
//!
//! 规范依据（`pdftotext -raw docs/PDF32000_2008.pdf` 的行号；多列表格须用
//! `-raw`，`-layout` 会让列错位）：
//!
//! - `/Subj` 行在 L21706–L21707：「(Optional; PDF 1.5) Text representing a
//!   short description of the subject being addressed by the annotation」。
//! - `/IRT` 行在 L21698–L21701：「(Required if an RT entry is present,
//!   otherwise optional; PDF 1.5) A reference to the annotation that this
//!   annotation is "in reply to." Both annotations shall be on the same page
//!   of the document. The relationship between the two annotations shall be
//!   specified by the RT entry」。因此 `/IRT` 的目标恒为**另一条**注解且必须
//!   同页；句柄范围、自引用与同页都在分配对象号与写出任何字节之前由
//!   `assemble/annotation_markup` 统一核对（零字节契约）。同段的 FDF 例外
//!   （L21702–L21705：在 FDF 里该条目是文本串而非字典）属 12.7.7 的范围，
//!   本库写端只写引用形态。
//! - `/RT` 行在 L21708–L21717：「(Optional; meaningful only if IRT is
//!   present; PDF 1.6) A name specifying the relationship (the "reply type")
//!   between this annotation and one specified by IRT. Valid values are:
//!   R … Group … Default value: R」。合法值封闭为 `R`/`Group`，故构建态用
//!   不可表达非法值的 [`ReplyType`] 枚举，缺省 `R` 不物化（未声明即不写）；
//!   `/RT` 挂在没有 `/IRT` 的注解上违反 `/IRT` 行的 shall，构建期拒绝。
//! - Table 170 的标题是「Additional entries specific to markup
//!   annotations」，三条目都只许挂在标记类注解上（与 `/ExData` 同一守卫
//!   口径）；12.5.6.2 的分组散文（L21732–L21734）另要求「主注解 shall not
//!   have an IRT entry」，故分组的从属注解才带 `/IRT` + `/RT /Group`。

use super::PdfAnnotation;

/// 回复类型 `/RT`（Table 170，PDF 1.6）的标准名集合。
///
/// 与读端的 `reader::annotation::ReplyType` 不同：读端为保留未知名另有
/// `Other` 形态，写端只接受规范枚举的两个值，因此非法名不可能被写出。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplyType {
    /// `R`：本注解是 `/IRT` 目标的回复（规范的缺省值，不物化）。
    Reply,
    /// `Group`：本注解与 `/IRT` 目标同组（12.5.6.2 L21732–L21735）。
    Group,
}

impl ReplyType {
    /// `/RT` 的名字值。
    pub fn name(self) -> &'static str {
        match self {
            Self::Reply => "R",
            Self::Group => "Group",
        }
    }
}

impl PdfAnnotation {
    /// 主题简述 `/Subj`（Table 170，PDF 1.5；仅标记类注解）。
    pub fn with_subject(mut self, subject: impl Into<String>) -> Self {
        self.subject = Some(subject.into());
        self
    }

    /// 所回复的注解 `/IRT`（Table 170，PDF 1.5）；参数为另一条已注册注解的
    /// 句柄，同页与自引用由写出阶段在任何字节之前校验。
    pub fn with_in_reply_to(mut self, handle: usize) -> Self {
        self.in_reply_to = Some(handle);
        self
    }

    /// 回复类型 `/RT`（Table 170，PDF 1.6）；须同时声明
    /// [`with_in_reply_to`](Self::with_in_reply_to)。
    pub fn with_reply_type(mut self, reply_type: ReplyType) -> Self {
        self.reply_type = Some(reply_type);
        self
    }

    /// 主题 `/Subj`。
    pub(crate) fn subject(&self) -> Option<&str> {
        self.subject.as_deref()
    }

    /// 所回复注解的句柄（`/IRT`）。
    pub(crate) fn in_reply_to(&self) -> Option<usize> {
        self.in_reply_to
    }

    /// 回复类型 `/RT`。
    pub(crate) fn reply_type(&self) -> Option<ReplyType> {
        self.reply_type
    }

    /// 是否声明了任一条 Table 170 附加条目（供版本门禁与特征登记复用）。
    pub(crate) fn has_markup_universal_entries(&self) -> bool {
        self.subject.is_some() || self.in_reply_to.is_some() || self.reply_type.is_some()
    }

    /// Table 170 附加条目的归属与依赖校验；由 [`PdfAnnotation::validate`] 调用。
    pub(super) fn validate_markup_universal_entries(&self) -> Result<(), String> {
        if !self.has_markup_universal_entries() {
            return Ok(());
        }
        // Table 170 是「Additional entries specific to markup annotations」。
        if !self.kind.is_markup() {
            return Err(
                "annotation /Subj, /IRT and /RT are only permitted on markup annotations (Table 170)"
                    .into(),
            );
        }
        // `/IRT` 行（L21698）是「Required if an RT entry is present」。
        if self.reply_type.is_some() && self.in_reply_to.is_none() {
            return Err("annotation /RT requires an /IRT entry (Table 170)".into());
        }
        Ok(())
    }
}
