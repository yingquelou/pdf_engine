//! 注解通用条目解析（12.5.2–12.5.5）、子类型公共条目辅助函数。
//!
//! `/Rect`、文本串条目、数值数组（`/C`、`/IC`、`/QuadPoints`）、`/CA` 不透明度、
//! 边框（`/BS` 优先旧式 `/Border`，12.5.4）与外观流 `/AP`（12.5.5）。

use super::{AnnotationBorder, AnnotationError, AppearanceStreams, Result, malformed};
use crate::core::types::{Dict, Document, Value};

pub(super) fn parse_rect(value: Option<&Value>) -> Result<[f64; 4]> {
    let value = value.ok_or_else(|| malformed("annotation has no /Rect"))?;
    let array = value
        .as_array()
        .ok_or_else(|| malformed("annotation /Rect is not an array"))?;
    if array.len() != 4 {
        return Err(malformed("annotation /Rect must have four numbers"));
    }
    let mut out = [0.0f64; 4];
    for (i, item) in array.iter().enumerate() {
        out[i] = number(item, "annotation /Rect")?;
    }
    Ok(out)
}

pub(super) fn parse_text_string(dict: &Dict, key: &str) -> Result<Option<Value>> {
    match dict.get(key) {
        None => Ok(None),
        Some(value @ (Value::String(_) | Value::HexString(_))) => Ok(Some(value.clone())),
        Some(_) => Err(malformed(format!(
            "annotation /{key} must be a text string"
        ))),
    }
}

pub(super) fn parse_number_array(
    dict: &Dict,
    key: &str,
    multiple_of_eight: bool,
) -> Result<Option<Vec<f64>>> {
    let Some(value) = dict.get(key) else {
        return Ok(None);
    };
    let array = value
        .as_array()
        .ok_or_else(|| malformed(format!("annotation /{key} is not an array")))?;
    if multiple_of_eight && !array.len().is_multiple_of(8) {
        return Err(malformed(format!(
            "annotation /{key} length must be a multiple of 8"
        )));
    }
    let mut out = Vec::with_capacity(array.len());
    for item in array.iter() {
        out.push(number(item, &format!("annotation /{key}"))?);
    }
    Ok(Some(out))
}

pub(super) fn parse_opacity(dict: &Dict) -> Result<Option<f64>> {
    let Some(value) = dict.get("CA") else {
        return Ok(None);
    };
    let number = number(value, "annotation /CA")?;
    if !(0.0..=1.0).contains(&number) {
        return Err(malformed("annotation /CA must be within 0.0–1.0"));
    }
    Ok(Some(number))
}

/// 可选布尔条目（Line 注解 `/Cap`）；非布尔一律 malformed。
pub(super) fn parse_optional_bool(dict: &Dict, key: &str) -> Result<Option<bool>> {
    match dict.get(key) {
        None => Ok(None),
        Some(Value::Bool(flag)) => Ok(Some(*flag)),
        Some(_) => Err(malformed(format!("annotation /{key} must be a boolean"))),
    }
}

/// 可选单个数值条目（Line 注解 `/LL`、`/LLE`、`/LLO`）。
pub(super) fn parse_optional_number(dict: &Dict, key: &str) -> Result<Option<f64>> {
    match dict.get(key) {
        None => Ok(None),
        Some(value) => Ok(Some(number(value, &format!("annotation /{key}"))?)),
    }
}

pub(super) fn parse_border(document: &Document, dict: &Dict) -> Result<Option<AnnotationBorder>> {
    let blend_mode = match dict.get("BM") {
        None => None,
        Some(value) => Some(
            value
                .as_name()
                .ok_or_else(|| malformed("annotation /BM must be a name"))?
                .to_string(),
        ),
    };
    let border = if let Some(bs) = dict.get("BS") {
        let resolved = crate::core::reader::navigation::resolve(document, bs)
            .map_err(|err| AnnotationError::Malformed(err.to_string()))?;
        let bs = resolved
            .as_dict()
            .ok_or_else(|| malformed("annotation /BS is not a dictionary"))?;
        let width = match bs.get("W") {
            None => None,
            Some(value) => {
                let width = number(value, "annotation /BS /W")?;
                if width < 0.0 {
                    return Err(malformed("annotation /BS /W must be nonnegative"));
                }
                Some(width)
            }
        };
        let style = match bs.get("S") {
            None => None,
            Some(value) => Some(
                value
                    .as_name()
                    .ok_or_else(|| malformed("annotation /BS /S must be a name"))?
                    .to_string(),
            ),
        };
        let dash = match bs.get("D") {
            None => Vec::new(),
            Some(value) => {
                let array = value
                    .as_array()
                    .ok_or_else(|| malformed("annotation /BS /D is not an array"))?;
                array
                    .iter()
                    .map(|item| number(item, "annotation /BS /D"))
                    .collect::<Result<Vec<f64>>>()?
            }
        };
        Some(AnnotationBorder {
            width,
            style,
            dash,
            corner_radius: None,
            blend_mode: blend_mode.clone(),
        })
    } else if let Some(legacy) = dict.get("Border") {
        let array = legacy
            .as_array()
            .ok_or_else(|| malformed("annotation /Border is not an array"))?;
        if array.len() > 3 {
            return Err(malformed("annotation /Border array exceeds 3 items"));
        }
        let corner = array.0.first().map(|v| number(v, "annotation /Border"));
        let width = array.0.get(1).map(|v| number(v, "annotation /Border"));
        let dash = match array.0.get(2) {
            // 规范允许 [h w] 或 [h w [dash…]]；真实文档常见第三项写成数字
            // （如 `/Border [0 0 0]`），此处宽容忽略而不报错。
            Some(dash) => match dash.as_array() {
                Some(dash) => dash
                    .iter()
                    .map(|item| number(item, "annotation /Border dash"))
                    .collect::<Result<Vec<f64>>>()?,
                None => Vec::new(),
            },
            None => Vec::new(),
        };
        Some(AnnotationBorder {
            width: width.transpose()?,
            style: None,
            dash,
            corner_radius: corner.transpose()?,
            blend_mode: blend_mode.clone(),
        })
    } else {
        return Ok(blend_mode.map(|mode| AnnotationBorder {
            blend_mode: Some(mode),
            ..Default::default()
        }));
    };
    Ok(border)
}

pub(super) fn number(value: &Value, context: &str) -> Result<f64> {
    let number = match value {
        Value::Float(number) => *number,
        Value::Integer(number) => *number as f64,
        _ => return Err(malformed(format!("{context} must be a number"))),
    };
    if !number.is_finite() {
        return Err(malformed(format!("{context} has a nonfinite number")));
    }
    Ok(number)
}

pub(super) fn parse_appearance(
    document: &Document,
    dict: &Dict,
) -> Result<Option<AppearanceStreams>> {
    let Some(ap) = dict.get("AP") else {
        return Ok(None);
    };
    let resolved = crate::core::reader::navigation::resolve(document, ap)
        .map_err(|err| AnnotationError::Malformed(err.to_string()))?;
    let ap = resolved
        .as_dict()
        .ok_or_else(|| malformed("annotation /AP is not a dictionary"))?;
    let streams = AppearanceStreams {
        normal: ap.get("N").cloned(),
        down: ap.get("D").cloned(),
        rollover: ap.get("R").cloned(),
    };
    if streams.is_empty() {
        return Ok(None);
    }
    Ok(Some(streams))
}
