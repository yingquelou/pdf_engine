//! 3D 动画样式字典（Table 301）与动画样式（Table 302）。
//!
//! `/Subtype` 的合法值为 `None` `Linear` `Oscillating`；其他名字保留为
//! [`ThreeDAnimationStyleType::Other`]，规范要求读取器按 `None` 处理。
//! `/PC` 负数表示无限重复；`/TM` 必须为正数（>1 加速）。三个条目的缺省值
//! （`None` / 0 / 1）均不物化。

use crate::core::types::{Dict, Document, Value};

use super::{ThreeDError, check_type, malformed, optional_name, optional_number, resolve_value};

/// 动画样式（`/Subtype`，Table 302；缺省 `None`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThreeDAnimationStyleType {
    /// `None`：不播放动画（缺省）。
    None,
    /// `Linear`：线性播放。
    Linear,
    /// `Oscillating`：往复播放。
    Oscillating,
    /// 未识别的样式名（原样保留；规范要求按 `None` 处理）。
    Other(String),
}

/// 3D 动画样式字典（Table 301）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ThreeDAnimationStyle {
    /// `/Subtype` 动画样式；`None` 表示缺省 `None`。
    pub subtype: Option<ThreeDAnimationStyleType>,
    /// `/PC` 播放次数：非负为播放次数，负数为无限重复；`None` 表示缺省 0。
    pub play_count: Option<i64>,
    /// `/TM` 时间倍率（正数，>1 加速）；`None` 表示缺省 1。
    pub time_multiplier: Option<f64>,
}

/// 解析已解引用的 3D 动画样式字典；`context` 用于错误消息。
pub(super) fn parse_animation_style(
    document: &Document,
    dict: &Dict,
    context: &str,
) -> Result<ThreeDAnimationStyle, ThreeDError> {
    check_type(dict, "3DAnimationStyle", context)?;
    let subtype =
        optional_name(document, dict, "Subtype", context)?.map(|name| match name.as_str() {
            "None" => ThreeDAnimationStyleType::None,
            "Linear" => ThreeDAnimationStyleType::Linear,
            "Oscillating" => ThreeDAnimationStyleType::Oscillating,
            _ => ThreeDAnimationStyleType::Other(name),
        });
    let play_count = match dict.get("PC") {
        None => None,
        Some(value) => match resolve_value(document, value)? {
            Value::Integer(count) => Some(count),
            _ => return Err(malformed(format!("{context} /PC must be an integer"))),
        },
    };
    let time_multiplier = match optional_number(document, dict, "TM", context)? {
        Some(multiplier) if multiplier > 0.0 => Some(multiplier),
        Some(_) => {
            return Err(malformed(format!(
                "{context} /TM must be a positive number"
            )));
        }
        None => None,
    };
    Ok(ThreeDAnimationStyle {
        subtype,
        play_count,
        time_multiplier,
    })
}
