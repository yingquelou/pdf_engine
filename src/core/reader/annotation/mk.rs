//! 外观特性字典 `/MK`（12.5.2，Table 189）与图标适配字典 `/IF`（Table 247）。
//!
//! 规范只把 `/MK` 声明给 Widget（Table 188）与 Screen（Table 187）注解；读端在
//! 任何注解上出现 `/MK` 时都按表解析（与其余子类型条目一致的宽容策略）。
//! 注意 `/MK /CA` 是按钮题注（文本串），与注解字典的通用 `/CA` 不透明度
//! （数值，Table 164）同名不同义。
//!
//! `/I` `/RI` `/IX` 规范要求为间接引用的 Form XObject，读端原样保留引用，
//! 由调用方经 `Document::get_object` 解析。

use super::{
    Result,
    entries::{parse_number_array, parse_optional_bool, parse_text_string},
    malformed,
};
use crate::core::types::{Dict, Document, Value};

/// 图标缩放时机（`/IF /SW`，Table 247，缺省 `A`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IconScaleWhen {
    /// 始终缩放（`/A`，缺省）。
    Always,
    /// 仅当图标大于注解矩形时缩放（`/B`）。
    Bigger,
    /// 仅当图标小于注解矩形时缩放（`/S`）。
    Smaller,
    /// 从不缩放（`/N`）。
    Never,
    /// 非标准名（原样保留）。
    Other(String),
}

impl IconScaleWhen {
    /// 由 `/SW` 名字解析；未知名返回 [`IconScaleWhen::Other`]。
    pub fn from_name(name: &str) -> Self {
        match name {
            "A" => Self::Always,
            "B" => Self::Bigger,
            "S" => Self::Smaller,
            "N" => Self::Never,
            other => Self::Other(other.to_string()),
        }
    }

    /// 标准模式名；[`IconScaleWhen::Other`] 返回其原文。
    pub fn name(&self) -> &str {
        match self {
            Self::Always => "A",
            Self::Bigger => "B",
            Self::Smaller => "S",
            Self::Never => "N",
            Self::Other(other) => other,
        }
    }
}

/// 图标缩放类型（`/IF /S`，Table 247，缺省 `P`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IconScaling {
    /// 变形缩放，填满注解矩形、不顾宽高比（`/A`）。
    Anamorphic,
    /// 等比缩放，保持宽高比（`/P`，缺省）。
    Proportional,
    /// 非标准名（原样保留）。
    Other(String),
}

impl IconScaling {
    /// 由 `/S` 名字解析；未知名返回 [`IconScaling::Other`]。
    pub fn from_name(name: &str) -> Self {
        match name {
            "A" => Self::Anamorphic,
            "P" => Self::Proportional,
            other => Self::Other(other.to_string()),
        }
    }

    /// 标准模式名；[`IconScaling::Other`] 返回其原文。
    pub fn name(&self) -> &str {
        match self {
            Self::Anamorphic => "A",
            Self::Proportional => "P",
            Self::Other(other) => other,
        }
    }
}

/// 图标适配字典（`/MK /IF`，Table 247）的解析结果。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct IconFit {
    /// `/SW` 缩放时机（缺省 `A`）。
    pub scale_when: Option<IconScaleWhen>,
    /// `/S` 缩放类型（缺省 `P`）。
    pub scaling: Option<IconScaling>,
    /// `/A` 左右与上下剩余空间比例（仅等比缩放生效，缺省 `[0.5 0.5]`）。
    pub leftover: Option<[f64; 2]>,
    /// `/FB` 是否按不含边框线宽的界缩放（PDF 1.5，缺省 false）。
    pub fit_border: Option<bool>,
}

/// 外观特性字典（`/MK`，Table 189）的解析结果。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AppearanceCharacteristics {
    /// `/R` 相对页面逆时针旋转角度，须为 90 的倍数（缺省 0）。
    pub rotation: Option<i64>,
    /// `/BC` 边框颜色；元素个数决定色空间（0 透明、1 Gray、3 RGB、4 CMYK）。
    pub border_color: Option<Vec<f64>>,
    /// `/BG` 背景颜色，形态同 `/BC`。
    pub background_color: Option<Vec<f64>>,
    /// `/CA` 常态题注（按钮字段）。
    pub caption: Option<String>,
    /// `/RC` 悬停题注（仅 pushbutton 字段）。
    pub rollover_caption: Option<String>,
    /// `/AC` 按下（交替）题注（仅 pushbutton 字段）。
    pub alternate_caption: Option<String>,
    /// `/I` 常态图标 Form XObject 的间接引用原值。
    pub icon: Option<Value>,
    /// `/RI` 悬停图标 Form XObject 的间接引用原值。
    pub rollover_icon: Option<Value>,
    /// `/IX` 按下（交替）图标 Form XObject 的间接引用原值。
    pub alternate_icon: Option<Value>,
    /// `/IF` 图标适配字典。
    pub icon_fit: Option<IconFit>,
    /// `/TP` 题注相对图标的位置码 0–6（缺省 0）。
    pub text_position: Option<u8>,
}

/// 解析注解字典的 `/MK` 条目；缺失返回 `None`。
pub(super) fn parse_appearance_characteristics(
    document: &Document,
    dict: &Dict,
) -> Result<Option<AppearanceCharacteristics>> {
    let Some(value) = dict.get("MK") else {
        return Ok(None);
    };
    let resolved = crate::core::reader::navigation::resolve(document, value)
        .map_err(|err| super::AnnotationError::Malformed(err.to_string()))?;
    let mk = resolved
        .as_dict()
        .ok_or_else(|| malformed("annotation /MK must be a dictionary"))?;
    let icon_fit = match mk.get("IF") {
        None => None,
        Some(value) => {
            let resolved = crate::core::reader::navigation::resolve(document, value)
                .map_err(|err| super::AnnotationError::Malformed(err.to_string()))?;
            Some(parse_icon_fit(resolved.as_dict().ok_or_else(|| {
                malformed("annotation /MK /IF must be a dictionary")
            })?)?)
        }
    };
    Ok(Some(AppearanceCharacteristics {
        rotation: parse_rotation(mk)?,
        border_color: parse_color(mk, "BC")?,
        background_color: parse_color(mk, "BG")?,
        caption: parse_caption(mk, "CA")?,
        rollover_caption: parse_caption(mk, "RC")?,
        alternate_caption: parse_caption(mk, "AC")?,
        icon: parse_icon(mk, "I")?,
        rollover_icon: parse_icon(mk, "RI")?,
        alternate_icon: parse_icon(mk, "IX")?,
        icon_fit,
        text_position: parse_code(mk, "TP", 6)?,
    }))
}

fn parse_icon_fit(mk: &Dict) -> Result<IconFit> {
    Ok(IconFit {
        scale_when: parse_name(mk, "SW")?.map(|name| IconScaleWhen::from_name(&name)),
        scaling: parse_name(mk, "S")?.map(|name| IconScaling::from_name(&name)),
        leftover: parse_leftover(mk)?,
        fit_border: parse_optional_bool(mk, "FB")?,
    })
}

/// `/R` 旋转角：须为整数且 90 的倍数。
fn parse_rotation(mk: &Dict) -> Result<Option<i64>> {
    match mk.get("R") {
        None => Ok(None),
        Some(Value::Integer(value)) => {
            if value.rem_euclid(90) != 0 {
                return Err(malformed("annotation /MK /R must be a multiple of 90"));
            }
            Ok(Some(*value))
        }
        Some(_) => Err(malformed("annotation /MK /R must be an integer")),
    }
}

/// `/BC` `/BG` 颜色数组：0、1、3 或 4 个 0.0–1.0 分量。
fn parse_color(mk: &Dict, key: &str) -> Result<Option<Vec<f64>>> {
    let Some(components) = parse_number_array(mk, key, false)? else {
        return Ok(None);
    };
    if !matches!(components.len(), 0 | 1 | 3 | 4) {
        return Err(malformed(format!(
            "annotation /MK /{key} must have 0, 1, 3 or 4 components"
        )));
    }
    for component in &components {
        if !(0.0..=1.0).contains(component) {
            return Err(malformed(format!(
                "annotation /MK /{key} components must be within 0.0–1.0"
            )));
        }
    }
    Ok(Some(components))
}

/// `/CA` `/RC` `/AC` 题注文本串。
fn parse_caption(mk: &Dict, key: &str) -> Result<Option<String>> {
    match parse_text_string(mk, key)? {
        Some(value) => crate::core::reader::content::text_value(&value)
            .map(Some)
            .ok_or_else(|| malformed(format!("annotation /MK /{key} is not decodable text"))),
        None => Ok(None),
    }
}

/// `/I` `/RI` `/IX` 图标：Table 247 与 Table 189 均要求间接引用。
fn parse_icon(mk: &Dict, key: &str) -> Result<Option<Value>> {
    match mk.get(key) {
        None => Ok(None),
        Some(value @ Value::Ref(_, _)) => Ok(Some(value.clone())),
        Some(_) => Err(malformed(format!(
            "annotation /MK /{key} must be an indirect reference"
        ))),
    }
}

/// `/TP` 等位置码：整数且不超过 `maximum`。
fn parse_code(mk: &Dict, key: &str, maximum: u8) -> Result<Option<u8>> {
    match mk.get(key) {
        None => Ok(None),
        Some(Value::Integer(value)) => {
            if *value < 0 || *value > i64::from(maximum) {
                return Err(malformed(format!(
                    "annotation /MK /{key} must be within 0–{maximum}"
                )));
            }
            Ok(Some(*value as u8))
        }
        Some(_) => Err(malformed(format!(
            "annotation /MK /{key} must be an integer"
        ))),
    }
}

fn parse_name(mk: &Dict, key: &str) -> Result<Option<String>> {
    match mk.get(key) {
        None => Ok(None),
        Some(Value::Name(name)) => Ok(Some(name.clone())),
        Some(_) => Err(malformed(format!(
            "annotation /MK /IF /{key} must be a name"
        ))),
    }
}

/// `/IF /A` 剩余空间比例：恰两个 0.0–1.0 数值。
fn parse_leftover(mk: &Dict) -> Result<Option<[f64; 2]>> {
    let Some(values) = parse_number_array(mk, "A", false)? else {
        return Ok(None);
    };
    let [left, bottom] = <[f64; 2]>::try_from(values)
        .map_err(|_| malformed("annotation /MK /IF /A must hold exactly two numbers"))?;
    for component in [left, bottom] {
        if !(0.0..=1.0).contains(&component) {
            return Err(malformed(
                "annotation /MK /IF /A components must be within 0.0–1.0",
            ));
        }
    }
    Ok(Some([left, bottom]))
}
