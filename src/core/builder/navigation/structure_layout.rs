//! Layout 标准属性写端校验（ISO 32000-1 14.8.5.4 Table 342–346）。
//!
//! 在 `builder/navigation/structure.rs` 的展开阶段按 `/O /Layout` 复核
//! 条目的值形态与版本归属，并检查 Table 344/346 中明确限定结构类型的
//! 不可继承条目：
//!
//! - `/SpaceBefore` `/SpaceAfter` 只用于 BLSE，或显式 `/Placement` 不是
//!   `Inline` 的 ILSE（此类 ILSE 按 BLSE 处理）；
//! - `/Width` `/Height` 只用于 Figure/Formula/Form/Table/TH/TD；
//! - `/ColumnCount` `/ColumnGap` `/ColumnWidths` 只用于 Art/Sect/Div。
//!
//! 可继承条目（如 `/TextAlign` `/BlockAlign` `/BorderColor`）允许出现在
//! 任意元素上向子孙传播；`/ClassMap` 只做值形态检查。14.8.5.3 的继承
//! 解析不在本模块范围内。全部失败返回 [`WriteError::InvalidObject`]，
//! 且不分配任何对象号。

use crate::core::types::{Dict, Value};
use crate::core::writer::WriteError;

use super::invalid;
use super::structure_standard::StandardUsage;

/// `/ColumnGap` `/ColumnWidths` 数组的条目上限（与读端一致）。
const MAX_LAYOUT_COLUMN_VALUES: usize = 65536;

/// PDF 1.5 引入的 Table 343/345 条目。
const EXTENDED_KEYS: &[&str] = &[
    "BackgroundColor",
    "BorderColor",
    "BorderStyle",
    "BorderThickness",
    "Padding",
    "Color",
    "TBorderStyle",
    "TPadding",
    "TextDecorationColor",
    "TextDecorationThickness",
    "RubyAlign",
    "RubyPosition",
    "GlyphOrientationVertical",
];

/// PDF 1.6 引入的 Table 346 分栏条目。
const COLUMN_KEYS: &[&str] = &["ColumnCount", "ColumnGap", "ColumnWidths"];

/// BLSE 结构类型（14.8.4.2 Table 337–339 及插图/表格元素）。
const BLOCK_TYPES: &[&str] = &[
    "Document",
    "Part",
    "Art",
    "Sect",
    "Div",
    "BlockQuote",
    "Caption",
    "TOC",
    "TOCI",
    "Index",
    "NonStruct",
    "Private",
    "P",
    "H",
    "H1",
    "H2",
    "H3",
    "H4",
    "H5",
    "H6",
    "L",
    "LI",
    "Lbl",
    "LBody",
    "Table",
    "TR",
    "TH",
    "TD",
    "THead",
    "TBody",
    "TFoot",
    "Figure",
    "Formula",
    "Form",
    "Title",
    "FENote",
];

/// Table 344 `/Width` `/Height` 允许的结构类型。
const SIZED_TYPES: &[&str] = &["Figure", "Formula", "Form", "Table", "TH", "TD"];

/// Table 346 分栏条目允许的结构类型。
const COLUMN_TYPES: &[&str] = &["Art", "Sect", "Div"];

/// 校验一个 `/O /Layout` 属性对象的全部条目并累计版本使用情况。
pub(super) fn validate_entries(
    dict: &Dict,
    context: &str,
    resolved: Option<&str>,
    usage: &mut StandardUsage,
) -> Result<(), WriteError> {
    if EXTENDED_KEYS.iter().any(|key| dict.0.contains_key(*key)) {
        usage.layout_extended_attributes = true;
    }
    if COLUMN_KEYS.iter().any(|key| dict.0.contains_key(*key)) {
        usage.layout_column_attributes = true;
    }

    if let Some(value) = dict.get("Placement") {
        require_name(value, context, "Placement")?;
    }
    if let Some(value) = dict.get("WritingMode") {
        require_name(value, context, "WritingMode")?;
    }
    if let Some(value) = dict.get("BackgroundColor") {
        require_color(value, context, "BackgroundColor")?;
    }
    if let Some(value) = dict.get("BorderColor") {
        require_border_color(value, context, "BorderColor")?;
    }
    if let Some(value) = dict.get("BorderStyle") {
        require_border_style(value, context, "BorderStyle")?;
    }
    if let Some(value) = dict.get("BorderThickness") {
        require_border_thickness(value, context, "BorderThickness")?;
    }
    if let Some(value) = dict.get("Color") {
        require_color(value, context, "Color")?;
    }
    if let Some(value) = dict.get("Padding") {
        require_number_or_four(value, context, "Padding")?;
    }

    for key in ["SpaceBefore", "SpaceAfter"] {
        if let Some(value) = dict.get(key) {
            require_number(value, context, key)?;
            check_block_level(dict, resolved, context, key)?;
        }
    }
    for key in ["StartIndent", "EndIndent", "TextIndent"] {
        if let Some(value) = dict.get(key) {
            require_number(value, context, key)?;
        }
    }
    if let Some(value) = dict.get("TextAlign") {
        require_name(value, context, "TextAlign")?;
    }
    if let Some(value) = dict.get("BBox") {
        require_bbox(value, context, "BBox")?;
    }
    for key in ["Width", "Height"] {
        if let Some(value) = dict.get(key) {
            require_number_or_auto(value, context, key)?;
            check_applicable(resolved, context, key, SIZED_TYPES)?;
        }
    }
    for key in ["BlockAlign", "InlineAlign"] {
        if let Some(value) = dict.get(key) {
            require_name(value, context, key)?;
        }
    }
    if let Some(value) = dict.get("TBorderStyle") {
        require_border_style(value, context, "TBorderStyle")?;
    }
    if let Some(value) = dict.get("TPadding") {
        require_number_or_four(value, context, "TPadding")?;
    }
    if let Some(value) = dict.get("BaselineShift") {
        require_number(value, context, "BaselineShift")?;
    }
    if let Some(value) = dict.get("LineHeight") {
        require_line_height(value, context, "LineHeight")?;
    }
    if let Some(value) = dict.get("TextDecorationColor") {
        require_color(value, context, "TextDecorationColor")?;
    }
    if let Some(value) = dict.get("TextDecorationThickness") {
        require_non_negative_number(value, context, "TextDecorationThickness")?;
    }
    if let Some(value) = dict.get("TextDecorationType") {
        require_name(value, context, "TextDecorationType")?;
    }
    for key in ["RubyAlign", "RubyPosition"] {
        if let Some(value) = dict.get(key) {
            require_name(value, context, key)?;
        }
    }
    if let Some(value) = dict.get("GlyphOrientationVertical") {
        require_glyph_orientation(value, context, "GlyphOrientationVertical")?;
    }

    if let Some(value) = dict.get("ColumnCount") {
        require_column_count(value, context, "ColumnCount")?;
        check_applicable(resolved, context, "ColumnCount", COLUMN_TYPES)?;
    }
    for key in ["ColumnGap", "ColumnWidths"] {
        if let Some(value) = dict.get(key) {
            require_number_or_numbers(value, context, key)?;
            check_applicable(resolved, context, key, COLUMN_TYPES)?;
        }
    }
    Ok(())
}

/// `/SpaceBefore` `/SpaceAfter`：BLSE 或显式非 `Inline` 放置的 ILSE。
fn check_block_level(
    dict: &Dict,
    resolved: Option<&str>,
    context: &str,
    key: &str,
) -> Result<(), WriteError> {
    let Some(resolved) = resolved else {
        return Ok(());
    };
    if BLOCK_TYPES.contains(&resolved) {
        return Ok(());
    }
    // 未知结构类型按 BLSE 保守接受；已知 ILSE 仅在显式 /Placement 不是
    // Inline 时按 BLSE 处理（14.8.5.4.2）。
    let inline_known = [
        "Span",
        "Quote",
        "Note",
        "Reference",
        "BibEntry",
        "Code",
        "Link",
        "Annot",
        "Ruby",
        "RB",
        "RT",
        "RP",
        "Warichu",
        "WT",
        "WP",
    ]
    .contains(&resolved);
    let placement = dict.get("Placement").and_then(Value::as_name);
    if inline_known && placement.is_none_or(|name| name == "Inline") {
        return Err(invalid(format!(
            "{context} /{key} applies only to block-level structure elements or inline-level structure elements with a non-Inline /Placement (resolved type {resolved})"
        )));
    }
    Ok(())
}

/// 元素 `/A` 的适用性：`resolved` 为 `None`（ClassMap）时跳过。
fn check_applicable(
    resolved: Option<&str>,
    context: &str,
    key: &str,
    allowed: &[&str],
) -> Result<(), WriteError> {
    let Some(resolved) = resolved else {
        return Ok(());
    };
    if allowed.contains(&resolved) {
        return Ok(());
    }
    Err(invalid(format!(
        "{context} /{key} applies only to {} structure elements (resolved type {resolved})",
        allowed.join(" or ")
    )))
}

fn as_number(value: &Value) -> Option<f64> {
    match value {
        Value::Integer(integer) => Some(*integer as f64),
        Value::Float(float) if float.is_finite() => Some(*float),
        _ => None,
    }
}

fn require_number(value: &Value, context: &str, key: &str) -> Result<(), WriteError> {
    if as_number(value).is_some() {
        Ok(())
    } else {
        Err(invalid(format!(
            "{context} /{key} must be a finite number ({})",
            value.type_name()
        )))
    }
}

fn require_non_negative_number(value: &Value, context: &str, key: &str) -> Result<(), WriteError> {
    match as_number(value) {
        Some(number) if number >= 0.0 => Ok(()),
        Some(_) => Err(invalid(format!(
            "{context} /{key} must be a non-negative number"
        ))),
        None => Err(invalid(format!(
            "{context} /{key} must be a finite non-negative number ({})",
            value.type_name()
        ))),
    }
}

fn require_color(value: &Value, context: &str, key: &str) -> Result<(), WriteError> {
    let Value::Array(items) = value else {
        return Err(invalid(format!(
            "{context} /{key} must be an array of three numbers in the range 0.0 to 1.0 ({})",
            value.type_name()
        )));
    };
    if items.0.len() != 3 {
        return Err(invalid(format!(
            "{context} /{key} must be an array of three numbers in the range 0.0 to 1.0 ({} entries)",
            items.0.len()
        )));
    }
    for (position, item) in items.0.iter().enumerate() {
        match as_number(item) {
            Some(component) if (0.0..=1.0).contains(&component) => {}
            _ => {
                return Err(invalid(format!(
                    "{context} /{key} entry {position} must be a number in the range 0.0 to 1.0"
                )));
            }
        }
    }
    Ok(())
}

fn require_bbox(value: &Value, context: &str, key: &str) -> Result<(), WriteError> {
    let Value::Array(items) = value else {
        return Err(invalid(format!(
            "{context} /{key} must be an array of four numbers ({})",
            value.type_name()
        )));
    };
    if items.0.len() != 4 {
        return Err(invalid(format!(
            "{context} /{key} must be an array of four numbers ({} entries)",
            items.0.len()
        )));
    }
    for (position, item) in items.0.iter().enumerate() {
        if as_number(item).is_none() {
            return Err(invalid(format!(
                "{context} /{key} entry {position} must be a finite number"
            )));
        }
    }
    Ok(())
}

fn require_border_color(value: &Value, context: &str, key: &str) -> Result<(), WriteError> {
    let Value::Array(items) = value else {
        return Err(invalid(format!(
            "{context} /{key} must be an array of three numbers or an array of four colour arrays ({})",
            value.type_name()
        )));
    };
    if items.0.len() == 3 {
        return require_color(value, context, key);
    }
    if items.0.len() != 4 {
        return Err(invalid(format!(
            "{context} /{key} must be an array of three numbers or an array of four colour arrays ({} entries)",
            items.0.len()
        )));
    }
    for (position, item) in items.0.iter().enumerate() {
        if !matches!(item, Value::Null) {
            require_color(item, context, key).map_err(|_| {
                invalid(format!(
                    "{context} /{key} edge {position} must be an array of three numbers in the range 0.0 to 1.0 or null"
                ))
            })?;
        }
    }
    Ok(())
}

fn require_border_style(value: &Value, context: &str, key: &str) -> Result<(), WriteError> {
    match value {
        Value::Name(_) => Ok(()),
        Value::Array(items) => {
            if items.0.len() != 4 {
                return Err(invalid(format!(
                    "{context} /{key} must be a name or an array of four names ({} entries)",
                    items.0.len()
                )));
            }
            for (position, item) in items.0.iter().enumerate() {
                if !matches!(item, Value::Null | Value::Name(_)) {
                    return Err(invalid(format!(
                        "{context} /{key} entry {position} must be a name or null ({})",
                        item.type_name()
                    )));
                }
            }
            Ok(())
        }
        other => Err(invalid(format!(
            "{context} /{key} must be a name or an array of four names ({})",
            other.type_name()
        ))),
    }
}

fn require_border_thickness(value: &Value, context: &str, key: &str) -> Result<(), WriteError> {
    match value {
        Value::Array(items) => {
            if items.0.len() != 4 {
                return Err(invalid(format!(
                    "{context} /{key} must be a number or an array of four numbers ({} entries)",
                    items.0.len()
                )));
            }
            for (position, item) in items.0.iter().enumerate() {
                if matches!(item, Value::Null) {
                    continue;
                }
                match as_number(item) {
                    Some(number) if number >= 0.0 => {}
                    Some(_) => {
                        return Err(invalid(format!(
                            "{context} /{key} entry {position} must be a non-negative number or null"
                        )));
                    }
                    None => {
                        return Err(invalid(format!(
                            "{context} /{key} entry {position} must be a non-negative number or null ({})",
                            item.type_name()
                        )));
                    }
                }
            }
            Ok(())
        }
        other => require_non_negative_number(other, context, key),
    }
}

fn require_number_or_four(value: &Value, context: &str, key: &str) -> Result<(), WriteError> {
    match value {
        Value::Array(items) => {
            if items.0.len() != 4 {
                return Err(invalid(format!(
                    "{context} /{key} must be a number or an array of four numbers ({} entries)",
                    items.0.len()
                )));
            }
            for (position, item) in items.0.iter().enumerate() {
                if as_number(item).is_none() {
                    return Err(invalid(format!(
                        "{context} /{key} entry {position} must be a finite number"
                    )));
                }
            }
            Ok(())
        }
        other => require_number(other, context, key).map_err(|_| {
            invalid(format!(
                "{context} /{key} must be a number or an array of four numbers ({})",
                other.type_name()
            ))
        }),
    }
}

fn require_number_or_auto(value: &Value, context: &str, key: &str) -> Result<(), WriteError> {
    if matches!(value, Value::Name(_)) {
        return Ok(());
    }
    require_number(value, context, key).map_err(|_| {
        invalid(format!(
            "{context} /{key} must be a number or a name ({})",
            value.type_name()
        ))
    })
}

fn require_line_height(value: &Value, context: &str, key: &str) -> Result<(), WriteError> {
    if matches!(value, Value::Name(_)) {
        return Ok(());
    }
    require_number(value, context, key).map_err(|_| {
        invalid(format!(
            "{context} /{key} must be a number or a name ({})",
            value.type_name()
        ))
    })
}

fn require_glyph_orientation(value: &Value, context: &str, key: &str) -> Result<(), WriteError> {
    if matches!(value, Value::Name(_)) {
        return Ok(());
    }
    let Some(angle) = as_number(value) else {
        return Err(invalid(format!(
            "{context} /{key} must be a name or a number ({})",
            value.type_name()
        )));
    };
    if angle % 90.0 != 0.0 || !(-180.0..=360.0).contains(&angle) {
        return Err(invalid(format!(
            "{context} /{key} must be a multiple of 90 between -180 and 360"
        )));
    }
    Ok(())
}

fn require_column_count(value: &Value, context: &str, key: &str) -> Result<(), WriteError> {
    match value {
        Value::Integer(count) if *count >= 1 && *count <= i64::from(u32::MAX) => Ok(()),
        other => Err(invalid(format!(
            "{context} /{key} must be an integer of at least 1 ({})",
            other.type_name()
        ))),
    }
}

fn require_number_or_numbers(value: &Value, context: &str, key: &str) -> Result<(), WriteError> {
    match value {
        Value::Array(items) => {
            if items.0.is_empty() {
                return Err(invalid(format!("{context} /{key} must not be empty")));
            }
            if items.0.len() > MAX_LAYOUT_COLUMN_VALUES {
                return Err(invalid(format!(
                    "{context} /{key} has {} entries (limit {MAX_LAYOUT_COLUMN_VALUES})",
                    items.0.len()
                )));
            }
            for (position, item) in items.0.iter().enumerate() {
                if as_number(item).is_none() {
                    return Err(invalid(format!(
                        "{context} /{key} entry {position} must be a finite number"
                    )));
                }
            }
            Ok(())
        }
        other => require_number(other, context, key).map_err(|_| {
            invalid(format!(
                "{context} /{key} must be a number or an array of numbers ({})",
                other.type_name()
            ))
        }),
    }
}

fn require_name(value: &Value, context: &str, key: &str) -> Result<(), WriteError> {
    if matches!(value, Value::Name(_)) {
        Ok(())
    } else {
        Err(invalid(format!(
            "{context} /{key} must be a name ({})",
            value.type_name()
        )))
    }
}
