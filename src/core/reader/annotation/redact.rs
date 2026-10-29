//! Redact 注解专有条目解析（12.5.6.23，Table 192）。
//!
//! 本模块只覆盖 Table 192 中不与其他子类型共用的键：`/OverlayText`、
//! `/Repeat` 与 `/RO`。`/QuadPoints`、`/IC`、`/DA`、`/Q` 与通用条目同名，
//! 分别落在 [`Annotation::quad_points`]、[`Annotation::interior_color`]、
//! [`Annotation::default_appearance`] 与 [`Annotation::quadding`]。
//!
//! `/RO` 是叠加外观的 Form XObject（可为直接流或间接引用），读端原样保留，
//! 由调用方经 `Document::get_object` 解析；其余子类型无关的取值一律
//! 返回类型化错误。

use super::{
    Result,
    entries::{parse_optional_bool, parse_text_string},
    malformed,
};
use crate::core::types::{Dict, Value};

/// Redact 注解专有条目的解析结果。
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct RedactEntries {
    /// `/OverlayText` 涂消后叠加的文本（文本串）。
    pub(super) overlay_text: Option<String>,
    /// `/Repeat` 是否平铺 `/OverlayText`（缺省 false）。
    pub(super) repeat_overlay_text: Option<bool>,
    /// `/RO` 叠加外观 Form XObject 原值（流或引用）。
    pub(super) overlay_appearance: Option<Value>,
}

/// 解析 Table 192 的 Redact 专有条目；`/DA` `/Q` `/IC` 由通用与子类型
/// 公共解析承担。
pub(super) fn parse_redact_entries(dict: &Dict) -> Result<RedactEntries> {
    let overlay_text = match parse_text_string(dict, "OverlayText")? {
        Some(value) => Some(
            crate::core::reader::content::text_value(&value)
                .ok_or_else(|| malformed("annotation /OverlayText is not decodable text"))?,
        ),
        None => None,
    };
    let overlay_appearance = match dict.get("RO") {
        None => None,
        Some(value @ (Value::Stream(..) | Value::Ref(_, _))) => Some(value.clone()),
        Some(_) => return Err(malformed("annotation /RO must be a stream or a reference")),
    };
    Ok(RedactEntries {
        overlay_text,
        repeat_overlay_text: parse_optional_bool(dict, "Repeat")?,
        overlay_appearance,
    })
}
