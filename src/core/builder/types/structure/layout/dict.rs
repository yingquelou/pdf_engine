//! `/O /Layout` 属性字典组装（ISO 32000-1 14.8.5.4 Table 342–346）：
//! `LayoutAttributes::into_dict` 按字段写出已设置的条目，
//! [`StructureAttribute::layout`] 是把属性对象挂进元素 `/A` 的入口；
//! 单个条目的值形态由 `values` 模块生成。
//!
//! 元素适用性与版本门禁不在本模块（见 `builder/navigation/structure_standard.rs`
//! 与 `structure_layout.rs`）。

use crate::core::types::{Dict, Value};

use super::super::StructureAttribute;
use super::forms::{GlyphOrientationVertical, LineHeight};
use super::model::LayoutAttributes;
use super::values::{
    border_color_value, border_styles_value, border_thickness_value, color_value, four_numbers,
    name_value, number_or_auto_value, number_or_four_value, number_or_numbers_value,
};

impl LayoutAttributes {
    /// 生成 `/O /Layout` 属性字典；仅包含已设置的条目。
    fn into_dict(self) -> Dict {
        let mut dict = Dict::new().insert("O", Value::Name("Layout".into()));
        if let Some(placement) = &self.placement {
            dict = dict.insert("Placement", name_value(placement.name()));
        }
        if let Some(writing_mode) = &self.writing_mode {
            dict = dict.insert("WritingMode", name_value(writing_mode.name()));
        }
        if let Some(color) = self.background_color {
            dict = dict.insert("BackgroundColor", color_value(color));
        }
        if let Some(color) = &self.border_color {
            dict = dict.insert("BorderColor", border_color_value(color));
        }
        if let Some(style) = &self.border_style {
            dict = dict.insert("BorderStyle", border_styles_value(style));
        }
        if let Some(thickness) = &self.border_thickness {
            dict = dict.insert("BorderThickness", border_thickness_value(thickness));
        }
        if let Some(color) = self.color {
            dict = dict.insert("Color", color_value(color));
        }
        if let Some(padding) = &self.padding {
            dict = dict.insert("Padding", number_or_four_value(padding));
        }
        if let Some(space) = self.space_before {
            dict = dict.insert("SpaceBefore", Value::Float(space));
        }
        if let Some(space) = self.space_after {
            dict = dict.insert("SpaceAfter", Value::Float(space));
        }
        if let Some(indent) = self.start_indent {
            dict = dict.insert("StartIndent", Value::Float(indent));
        }
        if let Some(indent) = self.end_indent {
            dict = dict.insert("EndIndent", Value::Float(indent));
        }
        if let Some(indent) = self.text_indent {
            dict = dict.insert("TextIndent", Value::Float(indent));
        }
        if let Some(align) = &self.text_align {
            dict = dict.insert("TextAlign", name_value(align.name()));
        }
        if let Some(bbox) = self.bbox {
            dict = dict.insert("BBox", four_numbers(bbox));
        }
        if let Some(width) = &self.width {
            dict = dict.insert("Width", number_or_auto_value(width));
        }
        if let Some(height) = &self.height {
            dict = dict.insert("Height", number_or_auto_value(height));
        }
        if let Some(align) = &self.block_align {
            dict = dict.insert("BlockAlign", name_value(align.name()));
        }
        if let Some(align) = &self.inline_align {
            dict = dict.insert("InlineAlign", name_value(align.name()));
        }
        if let Some(style) = &self.t_border_style {
            dict = dict.insert("TBorderStyle", border_styles_value(style));
        }
        if let Some(padding) = &self.t_padding {
            dict = dict.insert("TPadding", number_or_four_value(padding));
        }
        if let Some(shift) = self.baseline_shift {
            dict = dict.insert("BaselineShift", Value::Float(shift));
        }
        if let Some(height) = &self.line_height {
            let value = match height {
                LineHeight::Number(number) => Value::Float(*number),
                other => name_value(other.name()),
            };
            dict = dict.insert("LineHeight", value);
        }
        if let Some(color) = self.text_decoration_color {
            dict = dict.insert("TextDecorationColor", color_value(color));
        }
        if let Some(thickness) = self.text_decoration_thickness {
            dict = dict.insert("TextDecorationThickness", Value::Float(thickness));
        }
        if let Some(decoration) = &self.text_decoration_type {
            dict = dict.insert("TextDecorationType", name_value(decoration.name()));
        }
        if let Some(align) = &self.ruby_align {
            dict = dict.insert("RubyAlign", name_value(align.name()));
        }
        if let Some(position) = &self.ruby_position {
            dict = dict.insert("RubyPosition", name_value(position.name()));
        }
        if let Some(orientation) = &self.glyph_orientation_vertical {
            let value = match orientation {
                GlyphOrientationVertical::Angle(angle) => Value::Float(*angle),
                other => name_value(other.name()),
            };
            dict = dict.insert("GlyphOrientationVertical", value);
        }
        if let Some(count) = self.column_count {
            dict = dict.insert("ColumnCount", Value::Integer(i64::from(count)));
        }
        if let Some(gap) = &self.column_gap {
            dict = dict.insert("ColumnGap", number_or_numbers_value(gap));
        }
        if let Some(widths) = &self.column_widths {
            dict = dict.insert("ColumnWidths", number_or_numbers_value(widths));
        }
        dict
    }
}

impl StructureAttribute {
    /// Layout 属性对象（Table 342–346，`/O /Layout`）。
    ///
    /// 仅写出 `attributes` 中显式设置的条目，不物化规范缺省值。
    /// 版本门禁：基础条目要求 PDF 1.4；Table 343/345 的 1.5 条目与
    /// Table 346 的 1.6 分栏条目分别要求对应版本，由构建器校验。
    pub fn layout(attributes: LayoutAttributes) -> Self {
        Self::new(Value::Dict(attributes.into_dict()))
    }
}
