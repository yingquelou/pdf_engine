//! FreeText 子类型专有条目（Table 174）：意图、引线、对齐、边框效果与矩形差值，
//! 兼作 FreeText 与 Square/Circle/Polygon 共用的 `/BE`（Table 167）、`/RD` 解析。

use super::super::{
    Result,
    entries::{number, parse_number_array},
    malformed,
};
use super::parse_exact_numbers;
use crate::core::types::{Dict, Value};

/// FreeText 注解意图（`/IT`，Table 174，PDF 1.6）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FreeTextIntent {
    /// 普通文本框（`/FreeText`，缺省）。
    FreeText,
    /// 带引线标注的文本框（`/FreeTextCallout`，`/CL` 有意义）。
    FreeTextCallout,
    /// 点击即输/打字机式，不画引线（`/FreeTextTypeWriter`）。
    FreeTextTypeWriter,
    /// 非标准名（原样保留）。
    Other(String),
}

impl FreeTextIntent {
    /// 由 `/IT` 名字解析；未知名返回 [`FreeTextIntent::Other`]。
    pub fn from_name(name: &str) -> Self {
        match name {
            "FreeText" => Self::FreeText,
            "FreeTextCallout" => Self::FreeTextCallout,
            "FreeTextTypeWriter" => Self::FreeTextTypeWriter,
            other => Self::Other(other.to_string()),
        }
    }

    /// 标准意图名；[`FreeTextIntent::Other`] 返回其原文。
    pub fn name(&self) -> &str {
        match self {
            Self::FreeText => "FreeText",
            Self::FreeTextCallout => "FreeTextCallout",
            Self::FreeTextTypeWriter => "FreeTextTypeWriter",
            Self::Other(other) => other,
        }
    }
}

/// FreeText 引线（`/CL`，Table 174，仅意图为 `/FreeTextCallout` 时有意义）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CalloutLine {
    /// 四点 `[x1 y1 x2 y2]`：起点与终点。
    Straight([f64; 4]),
    /// 六点 `[x1 y1 x2 y2 x3 y3]`：起点、拐点与终点。
    Knee([f64; 6]),
}

/// 边框效果样式（`/BE /S`，Table 167）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BorderEffectStyle {
    /// 无效果，边框按 `/BS` 原样绘制（`/S`，缺省）。
    NoEffect,
    /// 云状边框（`/C`），`/I` 仅在此样式下有效。
    Cloudy,
    /// 非标准名（原样保留）。
    Other(String),
}

impl BorderEffectStyle {
    /// 由 `/BE /S` 名字解析；未知名返回 [`BorderEffectStyle::Other`]。
    pub fn from_name(name: &str) -> Self {
        match name {
            "S" => Self::NoEffect,
            "C" => Self::Cloudy,
            other => Self::Other(other.to_string()),
        }
    }

    /// 标准样式名；[`BorderEffectStyle::Other`] 返回其原文。
    pub fn name(&self) -> &str {
        match self {
            Self::NoEffect => "S",
            Self::Cloudy => "C",
            Self::Other(other) => other,
        }
    }
}

/// 边框效果字典（`/BE`，Table 167）。
#[derive(Debug, Clone, PartialEq)]
pub struct BorderEffect {
    /// `/S` 样式；缺省为 [`BorderEffectStyle::NoEffect`]。
    pub style: BorderEffectStyle,
    /// `/I` 强度（0–2），仅在 `/S` 为云状时有效；缺省 `0`。
    pub intensity: Option<f64>,
}

/// `/Q` 对齐方式（0 左、1 居中、2 右，Table 174）。
pub(super) fn parse_quadding(dict: &Dict) -> Result<Option<u8>> {
    let Some(value) = dict.get("Q") else {
        return Ok(None);
    };
    let number = number(value, "annotation /Q")?;
    if number.fract() != 0.0 || !(0.0..=2.0).contains(&number) {
        return Err(malformed("annotation /Q must be 0, 1 or 2"));
    }
    Ok(Some(number as u8))
}

/// FreeText 引线 `/CL`：四或六个有限数值（Table 174）。
pub(super) fn parse_callout_line(dict: &Dict) -> Result<Option<CalloutLine>> {
    let Some(values) = parse_number_array(dict, "CL", false)? else {
        return Ok(None);
    };
    match values.len() {
        4 => Ok(Some(CalloutLine::Straight([
            values[0], values[1], values[2], values[3],
        ]))),
        6 => Ok(Some(CalloutLine::Knee([
            values[0], values[1], values[2], values[3], values[4], values[5],
        ]))),
        _ => Err(malformed("annotation /CL must hold four or six numbers")),
    }
}

/// `/RD` 注解矩形与 `/Rect` 的四边差值，均须 ≥0（FreeText Table 174，
/// Square/Circle Table 177 共用同一键与约束）。
pub(super) fn parse_text_rectangle(dict: &Dict) -> Result<Option<[f64; 4]>> {
    let Some(values) = parse_exact_numbers(dict, "RD", 4)? else {
        return Ok(None);
    };
    if values.iter().any(|value| *value < 0.0) {
        return Err(malformed("annotation /RD differences must not be negative"));
    }
    Ok(Some([values[0], values[1], values[2], values[3]]))
}

/// `/BE` 边框效果字典（Table 167）：`/S` 样式名，`/I` 强度 0–2
/// （FreeText 与 Square/Circle/Polygon 共用同一结构与约束）。
pub(super) fn parse_border_effect(dict: &Dict) -> Result<Option<BorderEffect>> {
    let Some(value) = dict.get("BE") else {
        return Ok(None);
    };
    let effect = match value {
        Value::Dict(effect) => effect,
        _ => return Err(malformed("annotation /BE is not a dictionary")),
    };
    let style = match effect.get("S") {
        None => BorderEffectStyle::NoEffect,
        Some(name) => BorderEffectStyle::from_name(
            name.as_name()
                .ok_or_else(|| malformed("annotation /BE /S must be a name"))?,
        ),
    };
    let intensity = match effect.get("I") {
        None => None,
        Some(value) => {
            let number = number(value, "annotation /BE /I")?;
            if !(0.0..=2.0).contains(&number) {
                return Err(malformed("annotation /BE /I must be within 0.0–2.0"));
            }
            Some(number)
        }
    };
    Ok(Some(BorderEffect { style, intensity }))
}
