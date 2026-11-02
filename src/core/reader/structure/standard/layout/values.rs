//! Layout 取值解析辅助与常量表：名字、数值、颜色、边框与列数组条目的形态校验。

use crate::core::reader::structure::standard::{StructureError, malformed};
use crate::core::types::{Dict, Value};

use super::model::{
    BorderColor, BorderStyle, BorderStyles, BorderThickness, GlyphOrientationVertical, LineHeight,
    NumberOrAuto, NumberOrFour, NumberOrNumbers,
};

/// `/ColumnGap` `/ColumnWidths` 数组的条目上限。
const MAX_LAYOUT_COLUMN_VALUES: usize = 65536;

/// 存在时调用解析器，缺省返回 `None`。
pub(super) fn optional<T>(
    dict: &Dict,
    key: &str,
    index: usize,
    parse: impl FnOnce(&Value, usize, &str) -> Result<T, StructureError>,
) -> Result<Option<T>, StructureError> {
    match dict.get(key) {
        Some(value) => parse(value, index, key).map(Some),
        None => Ok(None),
    }
}

/// 名字型条目按给定映射转换；非名字返回 `Malformed`。
pub(super) fn name_enum<T>(
    value: &Value,
    index: usize,
    key: &str,
    parse: impl FnOnce(&str) -> T,
) -> Result<T, StructureError> {
    match value {
        Value::Name(name) => Ok(parse(name)),
        other => Err(malformed(format!(
            "structure element {index} /{key} must be a name ({})",
            other.type_name()
        ))),
    }
}

/// `/GlyphOrientationVertical`：名字（`Auto`）或 90 的倍数角度。
pub(super) fn glyph_orientation(
    value: &Value,
    index: usize,
    key: &str,
) -> Result<GlyphOrientationVertical, StructureError> {
    match value {
        Value::Name(name) => Ok(GlyphOrientationVertical::from_name(name)),
        Value::Integer(_) | Value::Float(_) => {
            let angle = number(value, index, key)?;
            if angle.rem_euclid(90.0) != 0.0 || !(-180.0..=360.0).contains(&angle) {
                return Err(malformed(format!(
                    "structure element {index} /{key} must be a multiple of 90 between -180 and 360"
                )));
            }
            Ok(GlyphOrientationVertical::Angle(angle))
        }
        other => Err(malformed(format!(
            "structure element {index} /{key} must be a name or a number ({})",
            other.type_name()
        ))),
    }
}

/// 有限数值（整数或有限浮点数）。
fn number_value(value: &Value) -> Option<f64> {
    match value {
        Value::Integer(value) => Some(*value as f64),
        Value::Float(value) if value.is_finite() => Some(*value),
        _ => None,
    }
}

pub(super) fn number(value: &Value, index: usize, key: &str) -> Result<f64, StructureError> {
    number_value(value).ok_or_else(|| {
        malformed(format!(
            "structure element {index} /{key} must be a finite number ({})",
            value.type_name()
        ))
    })
}

pub(super) fn non_negative_number(
    value: &Value,
    index: usize,
    key: &str,
) -> Result<f64, StructureError> {
    let value = number(value, index, key)?;
    if value < 0.0 {
        return Err(malformed(format!(
            "structure element {index} /{key} must be non-negative ({value})"
        )));
    }
    Ok(value)
}

/// RGB 颜色：三个 0–1 的有限数值。
pub(super) fn color(value: &Value, index: usize, key: &str) -> Result<[f64; 3], StructureError> {
    let Value::Array(items) = value else {
        return Err(malformed(format!(
            "structure element {index} /{key} must be an array of three numbers in the range 0.0 to 1.0 ({})",
            value.type_name()
        )));
    };
    if items.0.len() != 3 {
        return Err(malformed(format!(
            "structure element {index} /{key} must be an array of three numbers in the range 0.0 to 1.0 ({} entries)",
            items.0.len()
        )));
    }
    let mut color = [0.0; 3];
    for (position, item) in items.0.iter().enumerate() {
        let component = number_value(item)
            .filter(|component| (0.0..=1.0).contains(component))
            .ok_or_else(|| {
                malformed(format!(
                    "structure element {index} /{key} entry {position} must be a number in the range 0.0 to 1.0 ({})",
                    item.type_name()
                ))
            })?;
        color[position] = component;
    }
    Ok(color)
}

/// `/BBox`：四个有限数值。
pub(super) fn bbox(value: &Value, index: usize, key: &str) -> Result<[f64; 4], StructureError> {
    let Value::Array(items) = value else {
        return Err(malformed(format!(
            "structure element {index} /{key} must be an array of four numbers ({})",
            value.type_name()
        )));
    };
    if items.0.len() != 4 {
        return Err(malformed(format!(
            "structure element {index} /{key} must be an array of four numbers ({} entries)",
            items.0.len()
        )));
    }
    let mut bbox = [0.0; 4];
    for (position, item) in items.0.iter().enumerate() {
        bbox[position] = number(item, index, key).map_err(|_| {
            malformed(format!(
                "structure element {index} /{key} entry {position} must be a finite number ({})",
                item.type_name()
            ))
        })?;
    }
    Ok(bbox)
}

/// 四边数值数组（`/Padding` `/TPadding`）。
pub(super) fn number_or_four(
    value: &Value,
    index: usize,
    key: &str,
) -> Result<NumberOrFour, StructureError> {
    match value {
        Value::Integer(_) | Value::Float(_) => {
            Ok(NumberOrFour::Uniform(number(value, index, key)?))
        }
        Value::Array(items) => {
            if items.0.len() != 4 {
                return Err(malformed(format!(
                    "structure element {index} /{key} must be a number or an array of four numbers ({} entries)",
                    items.0.len()
                )));
            }
            let mut edges = [0.0; 4];
            for (position, item) in items.0.iter().enumerate() {
                edges[position] = number(item, index, key).map_err(|_| {
                    malformed(format!(
                        "structure element {index} /{key} entry {position} must be a finite number ({})",
                        item.type_name()
                    ))
                })?;
            }
            Ok(NumberOrFour::Edges(edges))
        }
        other => Err(malformed(format!(
            "structure element {index} /{key} must be a number or an array of four numbers ({})",
            other.type_name()
        ))),
    }
}

/// `/BorderColor`：三数值 RGB 数组或四边 RGB 数组（边可为 `null`）。
pub(super) fn border_color(
    value: &Value,
    index: usize,
    key: &str,
) -> Result<BorderColor, StructureError> {
    let Value::Array(items) = value else {
        return Err(malformed(format!(
            "structure element {index} /{key} must be an array of three numbers or an array of four colour arrays ({})",
            value.type_name()
        )));
    };
    match items.0.len() {
        3 => {
            let uniform = color(value, index, key)?;
            Ok(BorderColor::Uniform(uniform))
        }
        4 => {
            if !items
                .0
                .iter()
                .all(|item| matches!(item, Value::Null | Value::Array(_)))
            {
                return Err(malformed(format!(
                    "structure element {index} /{key} must be an array of three numbers or an array of four colour arrays (4 entries)"
                )));
            }
            let mut edges: [Option<[f64; 3]>; 4] = [None; 4];
            for (position, item) in items.0.iter().enumerate() {
                edges[position] = match item {
                    Value::Null => None,
                    Value::Array(_) => Some(color(item, index, key).map_err(|_| {
                        malformed(format!(
                            "structure element {index} /{key} edge {position} must be an array of three numbers in the range 0.0 to 1.0"
                        ))
                    })?),
                    other => {
                        return Err(malformed(format!(
                            "structure element {index} /{key} edge {position} must be an array of three numbers or null ({})",
                            other.type_name()
                        )));
                    }
                };
            }
            Ok(BorderColor::Edges(edges))
        }
        count => Err(malformed(format!(
            "structure element {index} /{key} must be an array of three numbers or an array of four colour arrays ({count} entries)"
        ))),
    }
}

/// `/BorderStyle` `/TBorderStyle`：单个名字或 four-entry 名字/`null` 数组。
pub(super) fn border_styles(
    value: &Value,
    index: usize,
    key: &str,
) -> Result<BorderStyles, StructureError> {
    match value {
        Value::Name(name) => Ok(BorderStyles::Uniform(BorderStyle::from_name(name))),
        Value::Array(items) => {
            if items.0.len() != 4 {
                return Err(malformed(format!(
                    "structure element {index} /{key} must be a name or an array of four names ({} entries)",
                    items.0.len()
                )));
            }
            let mut edges: [Option<BorderStyle>; 4] = [None, None, None, None];
            for (position, item) in items.0.iter().enumerate() {
                edges[position] = match item {
                    Value::Null => None,
                    Value::Name(name) => Some(BorderStyle::from_name(name)),
                    other => {
                        return Err(malformed(format!(
                            "structure element {index} /{key} entry {position} must be a name or null ({})",
                            other.type_name()
                        )));
                    }
                };
            }
            Ok(BorderStyles::Edges(edges))
        }
        other => Err(malformed(format!(
            "structure element {index} /{key} must be a name or an array of four names ({})",
            other.type_name()
        ))),
    }
}

/// `/BorderThickness`：数值或四边数值/`null` 数组，非负。
pub(super) fn border_thickness(
    value: &Value,
    index: usize,
    key: &str,
) -> Result<BorderThickness, StructureError> {
    match value {
        Value::Integer(_) | Value::Float(_) => Ok(BorderThickness::Uniform(non_negative_number(
            value, index, key,
        )?)),
        Value::Array(items) => {
            if items.0.len() != 4 {
                return Err(malformed(format!(
                    "structure element {index} /{key} must be a number or an array of four numbers ({} entries)",
                    items.0.len()
                )));
            }
            let mut edges: [Option<f64>; 4] = [None; 4];
            for (position, item) in items.0.iter().enumerate() {
                edges[position] = match item {
                    Value::Null => None,
                    _ => Some(non_negative_number(item, index, key).map_err(|_| {
                        malformed(format!(
                            "structure element {index} /{key} entry {position} must be a non-negative number or null ({})",
                            item.type_name()
                        ))
                    })?),
                };
            }
            Ok(BorderThickness::Edges(edges))
        }
        other => Err(malformed(format!(
            "structure element {index} /{key} must be a number or an array of four numbers ({})",
            other.type_name()
        ))),
    }
}

/// `/Width` `/Height`：有限数值或名字（`Auto` 之外的名字保留为 `Other`）。
pub(super) fn number_or_auto(
    value: &Value,
    index: usize,
    key: &str,
) -> Result<NumberOrAuto, StructureError> {
    match value {
        Value::Integer(_) | Value::Float(_) => Ok(NumberOrAuto::Number(number(value, index, key)?)),
        Value::Name(name) if name == "Auto" => Ok(NumberOrAuto::Auto),
        Value::Name(name) => Ok(NumberOrAuto::Other(name.clone())),
        other => Err(malformed(format!(
            "structure element {index} /{key} must be a number or a name ({})",
            other.type_name()
        ))),
    }
}

/// `/LineHeight`：有限数值或名字（`Normal` `/Auto` 之外的名字保留为 `Other`）。
pub(super) fn line_height(
    value: &Value,
    index: usize,
    key: &str,
) -> Result<LineHeight, StructureError> {
    match value {
        Value::Integer(_) | Value::Float(_) => Ok(LineHeight::Number(number(value, index, key)?)),
        Value::Name(name) if name == "Normal" => Ok(LineHeight::Normal),
        Value::Name(name) if name == "Auto" => Ok(LineHeight::Auto),
        Value::Name(name) => Ok(LineHeight::Other(name.clone())),
        other => Err(malformed(format!(
            "structure element {index} /{key} must be a number or a name ({})",
            other.type_name()
        ))),
    }
}

/// `/ColumnCount`：≥1 的整数。
pub(super) fn column_count(value: &Value, index: usize, key: &str) -> Result<u32, StructureError> {
    match value {
        Value::Integer(count) if *count >= 1 && *count <= i64::from(u32::MAX) => Ok(*count as u32),
        other => Err(malformed(format!(
            "structure element {index} /{key} must be an integer of at least 1 ({})",
            other.type_name()
        ))),
    }
}

/// `/ColumnGap` `/ColumnWidths`：有限数值或非空有限数值数组。
pub(super) fn number_or_numbers(
    value: &Value,
    index: usize,
    key: &str,
) -> Result<NumberOrNumbers, StructureError> {
    match value {
        Value::Integer(_) | Value::Float(_) => {
            Ok(NumberOrNumbers::Number(number(value, index, key)?))
        }
        Value::Array(items) => {
            if items.0.is_empty() {
                return Err(malformed(format!(
                    "structure element {index} /{key} must not be empty"
                )));
            }
            if items.0.len() > MAX_LAYOUT_COLUMN_VALUES {
                return Err(StructureError::LimitExceeded("layout column values"));
            }
            let mut values = Vec::with_capacity(items.0.len());
            for (position, item) in items.0.iter().enumerate() {
                values.push(number(item, index, key).map_err(|_| {
                    malformed(format!(
                        "structure element {index} /{key} entry {position} must be a finite number ({})",
                        item.type_name()
                    ))
                })?);
            }
            Ok(NumberOrNumbers::Array(values))
        }
        other => Err(malformed(format!(
            "structure element {index} /{key} must be a number or an array of numbers ({})",
            other.type_name()
        ))),
    }
}
