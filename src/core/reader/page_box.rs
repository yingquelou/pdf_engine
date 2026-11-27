//! 页面边界与框颜色信息（ISO 32000-1 14.11.2、Table 30 页面条目与
//! Table 360/361 框颜色信息/框样式字典）。
//!
//! 页面字典声明四种页面边界：`/MediaBox`（必填、可继承）、`/CropBox`
//! （可选、可继承，缺省等于 MediaBox）、`/BleedBox` `/TrimBox` `/ArtBox`
//! （PDF 1.3，可选，**不可继承**，缺省等于 CropBox）。本模块只报告页面
//! 实际声明的值，不物化缺省值；缺省链与 14.11.2.1 的「有效框 = 声明框
//! 与 MediaBox 的交集」由 [`PageBounds::effective_crop`] 等有效值方法
//! 给出。页面旋转角 `/Rotate`（Table 30，可继承、90 的倍数、缺省 0）
//! 由 [`PageBounds::rotate`] 给出，沿 `/Parent` 链解析。
//!
//! `/BoxColorInfo`（PDF 1.4，Table 360）为每个框声明显示样式（Table 361
//! 框样式字典）。按 Table 360 注，页面对象未定义对应框时该条目被忽略；
//! 读端不做对应性强制，只按字典结构解析。`/S` 的扩展样式名原样保留。

mod bounds;
mod document;
mod error;
mod rectangle;
mod style;

use crate::core::reader::navigation::resolve;
use crate::core::types::{Dict, Document, Value};

pub use bounds::PageBounds;
pub use error::PageBoxError;
pub use rectangle::Rectangle;
pub use style::{BoxColorInfo, BoxStyle, BoxStyleName};

fn malformed(message: impl Into<String>) -> PageBoxError {
    PageBoxError::Malformed(message.into())
}

/// 解析矩形数组（解引用后须为恰好 4 个有限数字，任意对角顺序）。
fn parse_rectangle(
    document: &Document,
    value: &Value,
    context: &str,
) -> Result<Rectangle, PageBoxError> {
    let value =
        resolve(document, value).map_err(|error| malformed(format!("{context}: {error}")))?;
    let array = value
        .as_array()
        .ok_or_else(|| malformed(format!("{context} is not an array")))?;
    if array.len() != 4 {
        return Err(malformed(format!(
            "{context} must have exactly 4 numbers, got {}",
            array.len()
        )));
    }
    let mut coordinates = [0.0f64; 4];
    for (index, item) in array.iter().enumerate() {
        coordinates[index] = match item {
            Value::Integer(number) => *number as f64,
            Value::Float(number) if number.is_finite() => *number,
            Value::Float(_) => {
                return Err(malformed(format!(
                    "{context} coordinate {index} is not finite"
                )));
            }
            other => {
                return Err(malformed(format!(
                    "{context} coordinate {index} is not a number ({})",
                    other.type_name()
                )));
            }
        };
    }
    Ok(Rectangle::new(
        coordinates[0],
        coordinates[1],
        coordinates[2],
        coordinates[3],
    ))
}

/// 解析 `/BoxColorInfo`（Table 360），条目本身须为字典。
fn parse_box_color_info(document: &Document, value: &Value) -> Result<BoxColorInfo, PageBoxError> {
    let value =
        resolve(document, value).map_err(|error| malformed(format!("/BoxColorInfo: {error}")))?;
    let dict = value
        .as_dict()
        .ok_or_else(|| malformed("/BoxColorInfo is not a dictionary"))?
        .clone();
    let crop = optional_style(document, &dict, "CropBox")?;
    let bleed = optional_style(document, &dict, "BleedBox")?;
    let trim = optional_style(document, &dict, "TrimBox")?;
    let art = optional_style(document, &dict, "ArtBox")?;
    Ok(BoxColorInfo {
        crop,
        bleed,
        trim,
        art,
        dict,
    })
}

/// 解析 `/BoxColorInfo` 的一个框样式条目。
fn optional_style(
    document: &Document,
    dict: &Dict,
    key: &str,
) -> Result<Option<BoxStyle>, PageBoxError> {
    let Some(value) = dict.get(key) else {
        return Ok(None);
    };
    let context = format!("/BoxColorInfo /{key}");
    let value =
        resolve(document, value).map_err(|error| malformed(format!("{context}: {error}")))?;
    let style = value
        .as_dict()
        .ok_or_else(|| malformed(format!("{context} is not a dictionary")))?
        .clone();
    parse_box_style(document, style, &context).map(Some)
}

/// 解析框样式字典（Table 361）。
///
/// `/C` 须为 3 个 0.0–1.0 的有限数字；`/W` 须有限；`/S` 须为名称（未知
/// 名称保留）；`/D` 须为非空、非负有限数字且不全为零的数组。Table 361
/// 限定 `/D` 只与 `/S /D` 同用，本读端不强制（宽松读取，写端强制）。
fn parse_box_style(
    document: &Document,
    dict: Dict,
    context: &str,
) -> Result<BoxStyle, PageBoxError> {
    let color = match dict.get("C") {
        Some(value) => Some(parse_color(document, value, context)?),
        None => None,
    };
    let width = match dict.get("W") {
        Some(value) => {
            let number = number_value(document, value, &format!("{context} /W"))?;
            if !number.is_finite() {
                return Err(malformed(format!("{context} /W is not finite")));
            }
            Some(number)
        }
        None => None,
    };
    let style = match dict.get("S") {
        Some(value) => {
            let value = resolve(document, value)
                .map_err(|error| malformed(format!("{context} /S: {error}")))?;
            let name = value
                .as_name()
                .ok_or_else(|| malformed(format!("{context} /S is not a name")))?;
            Some(BoxStyleName::from_name(name))
        }
        None => None,
    };
    let dash = match dict.get("D") {
        Some(value) => Some(parse_dash(document, value, context)?),
        None => None,
    };
    Ok(BoxStyle {
        color,
        width,
        style,
        dash,
        dict,
    })
}

/// 解析 `/C`：恰好 3 个 0.0–1.0 的有限数字。
fn parse_color(
    document: &Document,
    value: &Value,
    context: &str,
) -> Result<[f64; 3], PageBoxError> {
    let value =
        resolve(document, value).map_err(|error| malformed(format!("{context} /C: {error}")))?;
    let array = value
        .as_array()
        .ok_or_else(|| malformed(format!("{context} /C is not an array")))?;
    if array.len() != 3 {
        return Err(malformed(format!(
            "{context} /C must have exactly 3 numbers, got {}",
            array.len()
        )));
    }
    let mut color = [0.0f64; 3];
    for (index, item) in array.iter().enumerate() {
        let component = number_value(document, item, &format!("{context} /C"))?;
        if !component.is_finite() {
            return Err(malformed(format!(
                "{context} /C component {index} is not finite"
            )));
        }
        if !(0.0..=1.0).contains(&component) {
            return Err(malformed(format!(
                "{context} /C component {index} must be between 0 and 1, got {component}"
            )));
        }
        color[index] = component;
    }
    Ok(color)
}

/// 解析 `/D`：非空、非负有限数字且不全为零（8.4.3.6）。
fn parse_dash(document: &Document, value: &Value, context: &str) -> Result<Vec<f64>, PageBoxError> {
    let value =
        resolve(document, value).map_err(|error| malformed(format!("{context} /D: {error}")))?;
    let array = value
        .as_array()
        .ok_or_else(|| malformed(format!("{context} /D is not an array")))?;
    if array.is_empty() {
        return Err(malformed(format!("{context} /D must not be empty")));
    }
    let mut dash = Vec::with_capacity(array.len());
    for (index, item) in array.iter().enumerate() {
        let number = number_value(document, item, &format!("{context} /D"))?;
        if !number.is_finite() || number < 0.0 {
            return Err(malformed(format!(
                "{context} /D element {index} must be a non-negative finite number"
            )));
        }
        dash.push(number);
    }
    if dash.iter().all(|number| *number == 0.0) {
        return Err(malformed(format!("{context} /D must not be all zero")));
    }
    Ok(dash)
}

/// 取数字值（整数或浮点）。
fn number_value(document: &Document, value: &Value, context: &str) -> Result<f64, PageBoxError> {
    let value =
        resolve(document, value).map_err(|error| malformed(format!("{context}: {error}")))?;
    match value {
        Value::Integer(number) => Ok(number as f64),
        Value::Float(number) => Ok(number),
        other => Err(malformed(format!(
            "{context} is not a number ({})",
            other.type_name()
        ))),
    }
}

#[cfg(test)]
mod tests;
