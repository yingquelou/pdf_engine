//! 3D 截面字典（Table 311）。
//!
//! `/C` 截平面中心（缺省 `[0 0 0]`）、`/O` 平面方向（三元素数组，恰有一个
//! `null`，其余两个为角度；缺省 `[null 0 0]`）、`/PO` 平面不透明度
//! （`0..=1`，缺省 0.5）、`/PC` 平面颜色（缺省白色）、`/IV` 是否显示交线
//! （缺省 false）、`/IC` 交线颜色（缺省绿色，仅 `/IV` 为 true 时有意义）。
//! 规范将 `/O` 标为必填，但给出缺省值，读端接受缺失；缺省值均不物化。

use crate::core::types::{Dict, Document, Value};

use super::color::{ThreeDColor, parse_color};
use super::{
    ThreeDError, array_value, check_type, malformed, number_value, optional_bool, optional_number,
    resolve_value,
};

/// 3D 截面字典（Table 311）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ThreeDCrossSection {
    /// `/C` 截平面中心；`None` 表示缺省 `[0 0 0]`。
    pub center: Option<[f64; 3]>,
    /// `/O` 平面方向：恰有一个 `None`（`null`），其余为角度。
    pub orientation: Option<[Option<f64>; 3]>,
    /// `/PO` 平面不透明度（`0..=1`）；`None` 表示缺省 0.5。
    pub plane_opacity: Option<f64>,
    /// `/PC` 平面颜色；`None` 表示缺省白色。
    pub plane_color: Option<ThreeDColor>,
    /// `/IV` 是否显示交线（缺省 false）。
    pub intersection_visible: Option<bool>,
    /// `/IC` 交线颜色（仅 `/IV` 为 true 有意义）；`None` 表示缺省绿色。
    pub intersection_color: Option<ThreeDColor>,
}

/// 解析已解引用的截面字典。
pub(super) fn parse_cross_section(
    document: &Document,
    dict: &Dict,
    context: &str,
) -> Result<ThreeDCrossSection, ThreeDError> {
    check_type(dict, "3DCrossSection", context)?;
    let center = match dict.get("C") {
        None => None,
        Some(value) => Some(parse_triplet(document, value, &format!("{context} /C"))?),
    };
    let orientation = match dict.get("O") {
        None => None,
        Some(value) => Some(parse_orientation(
            document,
            value,
            &format!("{context} /O"),
        )?),
    };
    let plane_opacity = optional_number(document, dict, "PO", context)?;
    if let Some(opacity) = plane_opacity
        && !(0.0..=1.0).contains(&opacity)
    {
        return Err(malformed(format!("{context} /PO must be within 0..=1")));
    }
    let plane_color = match dict.get("PC") {
        None => None,
        Some(value) => Some(parse_color(document, value, &format!("{context} /PC"))?),
    };
    let intersection_visible = optional_bool(document, dict, "IV", context)?;
    let intersection_color = match dict.get("IC") {
        None => None,
        Some(value) => Some(parse_color(document, value, &format!("{context} /IC"))?),
    };
    Ok(ThreeDCrossSection {
        center,
        orientation,
        plane_opacity,
        plane_color,
        intersection_visible,
        intersection_color,
    })
}

/// 恰三个有限数值的数组。
fn parse_triplet(
    document: &Document,
    value: &Value,
    context: &str,
) -> Result<[f64; 3], ThreeDError> {
    let array = array_value(document, value, context)?;
    if array.len() != 3 {
        return Err(malformed(format!("{context} must hold exactly 3 numbers")));
    }
    let mut out = [0.0; 3];
    for (index, item) in array.iter().enumerate() {
        let resolved = resolve_value(document, item)?;
        out[index] = number_value(&resolved, &format!("{context} entry"))?;
    }
    Ok(out)
}

/// `/O`：恰有一个 `null`，其余两个为有限数值。
fn parse_orientation(
    document: &Document,
    value: &Value,
    context: &str,
) -> Result<[Option<f64>; 3], ThreeDError> {
    let array = array_value(document, value, context)?;
    if array.len() != 3 {
        return Err(malformed(format!("{context} must hold exactly 3 entries")));
    }
    let mut out: [Option<f64>; 3] = [None; 3];
    let mut nulls = 0;
    for (index, item) in array.iter().enumerate() {
        match resolve_value(document, item)? {
            Value::Null => nulls += 1,
            resolved => out[index] = Some(number_value(&resolved, &format!("{context} entry"))?),
        }
    }
    if nulls != 1 {
        return Err(malformed(format!(
            "{context} must hold exactly one null entry"
        )));
    }
    Ok(out)
}
