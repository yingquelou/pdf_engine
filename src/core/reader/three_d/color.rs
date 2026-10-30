//! 3D 颜色条目（`/AC` `/PC` `/IC`，Table 307/311）。
//!
//! 规范只定义 `[/DeviceRGB r g b]`。首项解析为名字 `DeviceRGB` 且随后恰有
//! 三个有限数值时类型化为 [`ThreeDColor::DeviceRgb`]；其他数组保留在
//! [`ThreeDColor::Other`]（规范要求消费者忽略未知颜色空间并按缺省颜色
//! 呈现）。非数组条目视为畸形。

use crate::core::types::{Array, Document, Value};

use super::{ThreeDError, malformed, number_value, resolve_value};

/// 3D 颜色（`/AC` `/PC` `/IC`）。
#[derive(Debug, Clone, PartialEq)]
pub enum ThreeDColor {
    /// `[/DeviceRGB r g b]`。
    DeviceRgb([f64; 3]),
    /// 未识别的颜色空间或分量（原值保留）。
    Other(Value),
}

impl ThreeDColor {
    /// 构造 DeviceRGB 颜色。
    pub fn rgb(values: [f64; 3]) -> Self {
        Self::DeviceRgb(values)
    }

    /// 写成 `[/DeviceRGB r g b]` 数组或原值。
    pub fn to_value(&self) -> Value {
        match self {
            Self::DeviceRgb(values) => rgb_array(*values),
            Self::Other(value) => value.clone(),
        }
    }
}

/// 构造 `[/DeviceRGB r g b]`。
pub(super) fn rgb_array(values: [f64; 3]) -> Value {
    let mut items = Vec::with_capacity(4);
    items.push(Value::Name("DeviceRGB".into()));
    items.extend(values.iter().map(|value| Value::Float(*value)));
    Value::Array(Array(items))
}

/// 解析颜色条目：必须是数组；DeviceRGB 三元组类型化，其他数组原值保留。
pub(super) fn parse_color(
    document: &Document,
    value: &Value,
    context: &str,
) -> Result<ThreeDColor, ThreeDError> {
    let resolved = resolve_value(document, value)?;
    let Value::Array(Array(items)) = &resolved else {
        return Err(malformed(format!("{context} must be an array")));
    };
    if items.len() == 4
        && matches!(
            resolve_value(document, &items[0])?,
            Value::Name(name) if name == "DeviceRGB"
        )
        && let Some(values) = numeric_triplet(document, &items[1..], context)?
    {
        return Ok(ThreeDColor::DeviceRgb(values));
    }
    Ok(ThreeDColor::Other(resolved))
}

/// 恰三个有限数值时返回数组，任一非数值返回 `None`（未知分量按原值保留）。
pub(super) fn numeric_triplet(
    document: &Document,
    items: &[Value],
    context: &str,
) -> Result<Option<[f64; 3]>, ThreeDError> {
    if items.len() != 3 {
        return Ok(None);
    }
    let mut out = [0.0; 3];
    for (index, item) in items.iter().enumerate() {
        let resolved = resolve_value(document, item)?;
        match number_value(&resolved, context) {
            Ok(number) => out[index] = number,
            Err(_) => return Ok(None),
        }
    }
    Ok(Some(out))
}
