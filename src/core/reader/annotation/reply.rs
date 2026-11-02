//! 标记类注解的回复条目 `/IRT` 与 `/RT`（ISO 32000-1 12.5.6.2，Table 170）。
//!
//! 规范原文（`pdftotext -raw docs/PDF32000_2008.pdf` 的行号；多列表格须用
//! `-raw`，`-layout` 会让列错位）：
//!
//! - Table 170 的 `/IRT` 行在 L21698–L21705：「(Required if an RT entry is
//!   present, otherwise optional; PDF 1.5) A reference to the annotation that
//!   this annotation is "in reply to." Both annotations shall be on the same
//!   page of the document. The relationship between the two annotations shall
//!   be specified by the RT entry.」同一段还给出 FDF 例外：「If this entry is
//!   present in an FDF file …, its type shall not be a dictionary but a text
//!   string containing the contents of the NM entry」（L21702–L21705）。本库
//!   只解析 PDF 内的注解字典，故按引用形态处理，文本串形态属已知范围限制。
//! - Table 170 的 `/RT` 行在 L21708–L21717：「(Optional; meaningful only if
//!   IRT is present; PDF 1.6) A name specifying the relationship (the "reply
//!   type") between this annotation and one specified by IRT」，合法值为
//!   `R`（回复，串成 threaded comments）与 `Group`（分组），缺省 `R`；
//!   读端不物化缺省值（缺 `/RT` 返回 `None`），未知名按
//!   [`super::LinkHighlightMode`] 的先例保留原文。
//! - 分组散文在 L21732–L21735：「The group consists of a primary annotation,
//!   which shall not have an IRT entry, and one or more subordinate
//!   annotations, which shall have an IRT entry that refers to the primary
//!   annotation and an RT entry whose value is Group」。因此 `/IRT` 的目标
//!   恒为**另一条**注解，同页约束（L21699）与「非自身」由写端在组装期核对。

use super::{Result, malformed};
use crate::core::types::Dict;

/// 回复类型 `/RT`（Table 170，PDF 1.6）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplyType {
    /// `R`：本注解是 `/IRT` 目标的回复（规范的缺省值）。
    Reply,
    /// `Group`：本注解与 `/IRT` 目标同组（12.5.6.2 L21732–L21735）。
    Group,
    /// 非标准名（原样保留）。
    Other(String),
}

impl ReplyType {
    /// 由 `/RT` 名字解析；未知名返回 [`ReplyType::Other`]。
    pub fn from_name(name: &str) -> Self {
        match name {
            "R" => Self::Reply,
            "Group" => Self::Group,
            other => Self::Other(other.to_string()),
        }
    }

    /// 标准名；[`ReplyType::Other`] 返回其原文。
    pub fn name(&self) -> &str {
        match self {
            Self::Reply => "R",
            Self::Group => "Group",
            Self::Other(other) => other,
        }
    }
}

/// `/IRT`（Table 170 L21698）：所回复注解的间接引用；缺失返回 `None`。
pub(super) fn parse_in_reply_to(dict: &Dict) -> Result<Option<(i64, i64)>> {
    match dict.get("IRT") {
        None => Ok(None),
        Some(value) => Ok(Some(value.as_ref().ok_or_else(|| {
            malformed("annotation /IRT must be an indirect reference to an annotation")
        })?)),
    }
}

/// `/RT`（Table 170 L21708）：回复类型名字；缺失返回 `None`（不物化缺省 `R`）。
pub(super) fn parse_reply_type(dict: &Dict) -> Result<Option<ReplyType>> {
    match dict.get("RT") {
        None => Ok(None),
        Some(value) => {
            Ok(Some(ReplyType::from_name(value.as_name().ok_or_else(
                || malformed("annotation /RT must be a name"),
            )?)))
        }
    }
}
