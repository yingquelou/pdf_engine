//! `/O /Layout` 属性字典的解析入口（Table 343–346）。

use crate::core::reader::structure::standard::StructureError;
use crate::core::types::Dict;

use super::model::{
    BlockAlign, InlineAlign, LayoutAttributes, Placement, RubyAlign, RubyPosition, TextAlign,
    TextDecorationType, WritingMode,
};
use super::values::{
    bbox, border_color, border_styles, border_thickness, color, column_count, glyph_orientation,
    line_height, name_enum, non_negative_number, number, number_or_auto, number_or_four,
    number_or_numbers, optional,
};

/// 解析一个合并后的 `/O /Layout` 属性字典（Table 343–346）。
pub(in crate::core::reader::structure::standard) fn parse(
    dict: &Dict,
    index: usize,
) -> Result<LayoutAttributes, StructureError> {
    Ok(LayoutAttributes {
        placement: optional(dict, "Placement", index, |value, index, key| {
            name_enum(value, index, key, Placement::from_name)
        })?,
        writing_mode: optional(dict, "WritingMode", index, |value, index, key| {
            name_enum(value, index, key, WritingMode::from_name)
        })?,
        background_color: optional(dict, "BackgroundColor", index, color)?,
        border_color: optional(dict, "BorderColor", index, border_color)?,
        border_style: optional(dict, "BorderStyle", index, border_styles)?,
        border_thickness: optional(dict, "BorderThickness", index, |value, index, key| {
            border_thickness(value, index, key)
        })?,
        color: optional(dict, "Color", index, color)?,
        padding: optional(dict, "Padding", index, number_or_four)?,
        space_before: optional(dict, "SpaceBefore", index, number)?,
        space_after: optional(dict, "SpaceAfter", index, number)?,
        start_indent: optional(dict, "StartIndent", index, number)?,
        end_indent: optional(dict, "EndIndent", index, number)?,
        text_indent: optional(dict, "TextIndent", index, number)?,
        text_align: optional(dict, "TextAlign", index, |value, index, key| {
            name_enum(value, index, key, TextAlign::from_name)
        })?,
        bbox: optional(dict, "BBox", index, bbox)?,
        width: optional(dict, "Width", index, number_or_auto)?,
        height: optional(dict, "Height", index, number_or_auto)?,
        block_align: optional(dict, "BlockAlign", index, |value, index, key| {
            name_enum(value, index, key, BlockAlign::from_name)
        })?,
        inline_align: optional(dict, "InlineAlign", index, |value, index, key| {
            name_enum(value, index, key, InlineAlign::from_name)
        })?,
        t_border_style: optional(dict, "TBorderStyle", index, border_styles)?,
        t_padding: optional(dict, "TPadding", index, number_or_four)?,
        baseline_shift: optional(dict, "BaselineShift", index, number)?,
        line_height: optional(dict, "LineHeight", index, line_height)?,
        text_decoration_color: optional(dict, "TextDecorationColor", index, color)?,
        text_decoration_thickness: optional(
            dict,
            "TextDecorationThickness",
            index,
            non_negative_number,
        )?,
        text_decoration_type: optional(dict, "TextDecorationType", index, |value, index, key| {
            name_enum(value, index, key, TextDecorationType::from_name)
        })?,
        ruby_align: optional(dict, "RubyAlign", index, |value, index, key| {
            name_enum(value, index, key, RubyAlign::from_name)
        })?,
        ruby_position: optional(dict, "RubyPosition", index, |value, index, key| {
            name_enum(value, index, key, RubyPosition::from_name)
        })?,
        glyph_orientation_vertical: optional(
            dict,
            "GlyphOrientationVertical",
            index,
            glyph_orientation,
        )?,
        column_count: optional(dict, "ColumnCount", index, column_count)?,
        column_gap: optional(dict, "ColumnGap", index, number_or_numbers)?,
        column_widths: optional(dict, "ColumnWidths", index, number_or_numbers)?,
        dict: dict.clone(),
    })
}
