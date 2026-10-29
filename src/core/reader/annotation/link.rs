//! Link 注解专有条目解析（12.5.6.5，Table 173）。
//!
//! `/H` 高亮模式（PDF 1.2，缺省 `Invert`）与 `/PA` 按下 URI 动作（PDF 1.3）。
//! `/PA` 与 `/A` 一样原样保留，动作语义由
//! `Document::annotation_pressed_action` 交给 12.6 的动作模型解析；
//! 非标准高亮模式名落成 [`LinkHighlightMode::Other`] 保留原文。

use super::{Result, malformed};
use crate::core::types::{Dict, Value};

/// 链接注解高亮模式（`/H`，Table 173，PDF 1.2）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkHighlightMode {
    /// 不高亮（`/N`）。
    None,
    /// 反色注解矩形内容（`/I`，缺省）。
    Invert,
    /// 反色注解边框（`/O`）。
    Outline,
    /// 按下立体效果（`/P`）。
    Push,
    /// 非标准名（原样保留）。
    Other(String),
}

impl LinkHighlightMode {
    /// 由 `/H` 名字解析；未知名返回 [`LinkHighlightMode::Other`]。
    pub fn from_name(name: &str) -> Self {
        match name {
            "N" => Self::None,
            "I" => Self::Invert,
            "O" => Self::Outline,
            "P" => Self::Push,
            other => Self::Other(other.to_string()),
        }
    }

    /// 标准模式名；[`LinkHighlightMode::Other`] 返回其原文。
    pub fn name(&self) -> &str {
        match self {
            Self::None => "N",
            Self::Invert => "I",
            Self::Outline => "O",
            Self::Push => "P",
            Self::Other(other) => other,
        }
    }
}

/// Link 注解专有条目的解析结果。
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct LinkEntries {
    /// `/H` 高亮模式（缺省 `Invert`，读端保留 `None`）。
    pub(super) highlight: Option<LinkHighlightMode>,
    /// `/PA` 原值（URI 动作字典或间接引用）。
    pub(super) pressed_action: Option<Value>,
}

/// 解析 `/H` 与 `/PA`；`/H` 必须是名字，`/PA` 留给动作解析宽容处理。
pub(super) fn parse_link_entries(dict: &Dict) -> Result<LinkEntries> {
    let highlight = match dict.get("H") {
        None => None,
        Some(value) => Some(LinkHighlightMode::from_name(
            value
                .as_name()
                .ok_or_else(|| malformed("annotation /H must be a name"))?,
        )),
    };
    Ok(LinkEntries {
        highlight,
        pressed_action: dict.get("PA").cloned(),
    })
}
