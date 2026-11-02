//! `LayoutAttributes` 模型与构造器（ISO 32000-1 14.8.5.4 Table 342–346）：
//! Table 343–346 全部标准条目的 `Option` 字段，以及按调用方声明顺序逐条
//! 设置条目的 `with_*` 构造器；字典组装见 `dict` 模块。

use super::enums::{
    BlockAlign, InlineAlign, Placement, RubyAlign, RubyPosition, TextAlign, TextDecorationType,
    WritingMode,
};
use super::forms::{
    BorderColor, BorderStyles, BorderThickness, GlyphOrientationVertical, LineHeight, NumberOrAuto,
    NumberOrFour, NumberOrNumbers,
};

/// Layout 标准属性（Table 342–346，`/O /Layout`）。
///
/// 所有字段默认 `None`；未设置的条目不会写出，规范缺省值不会被物化。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LayoutAttributes {
    /// `/Placement`：放置方式（缺省 `Inline`）。
    pub placement: Option<Placement>,
    /// `/WritingMode`：书写方向（缺省 `LrTb`）。
    pub writing_mode: Option<WritingMode>,
    /// `/BackgroundColor`：背景 RGB（PDF 1.5）。
    pub background_color: Option<[f64; 3]>,
    /// `/BorderColor`：边框颜色（PDF 1.5）。
    pub border_color: Option<BorderColor>,
    /// `/BorderStyle`：边框样式（PDF 1.5）。
    pub border_style: Option<BorderStyles>,
    /// `/BorderThickness`：边框厚度（PDF 1.5）。
    pub border_thickness: Option<BorderThickness>,
    /// `/Color`：文字颜色（PDF 1.5）。
    pub color: Option<[f64; 3]>,
    /// `/Padding`：内边距（PDF 1.5）。
    pub padding: Option<NumberOrFour>,
    /// `/SpaceBefore`：段前间距。
    pub space_before: Option<f64>,
    /// `/SpaceAfter`：段后间距。
    pub space_after: Option<f64>,
    /// `/StartIndent`：start 边缩进。
    pub start_indent: Option<f64>,
    /// `/EndIndent`：end 边缩进。
    pub end_indent: Option<f64>,
    /// `/TextIndent`：首行缩进。
    pub text_indent: Option<f64>,
    /// `/TextAlign`：行内对齐。
    pub text_align: Option<TextAlign>,
    /// `/BBox`：元素包围盒 `[left, bottom, right, top]`。
    pub bbox: Option<[f64; 4]>,
    /// `/Width`：内容矩形宽度（缺省 `Auto`）。
    pub width: Option<NumberOrAuto>,
    /// `/Height`：内容矩形高度（缺省 `Auto`）。
    pub height: Option<NumberOrAuto>,
    /// `/BlockAlign`：表格单元块方向对齐。
    pub block_align: Option<BlockAlign>,
    /// `/InlineAlign`：表格单元行内方向对齐。
    pub inline_align: Option<InlineAlign>,
    /// `/TBorderStyle`：表格单元边框样式（PDF 1.5）。
    pub t_border_style: Option<BorderStyles>,
    /// `/TPadding`：表格单元内边距（PDF 1.5）。
    pub t_padding: Option<NumberOrFour>,
    /// `/BaselineShift`：基线偏移。
    pub baseline_shift: Option<f64>,
    /// `/LineHeight`：行高（缺省 `Normal`）。
    pub line_height: Option<LineHeight>,
    /// `/TextDecorationColor`：文本装饰颜色（PDF 1.5）。
    pub text_decoration_color: Option<[f64; 3]>,
    /// `/TextDecorationThickness`：文本装饰厚度（PDF 1.5）。
    pub text_decoration_thickness: Option<f64>,
    /// `/TextDecorationType`：文本装饰类型。
    pub text_decoration_type: Option<TextDecorationType>,
    /// `/RubyAlign`：Ruby 对齐（PDF 1.5）。
    pub ruby_align: Option<RubyAlign>,
    /// `/RubyPosition`：Ruby 位置（PDF 1.5）。
    pub ruby_position: Option<RubyPosition>,
    /// `/GlyphOrientationVertical`：竖排字形方向（PDF 1.5）。
    pub glyph_orientation_vertical: Option<GlyphOrientationVertical>,
    /// `/ColumnCount`：分栏数（PDF 1.6，缺省 1）。
    pub column_count: Option<u32>,
    /// `/ColumnGap`：栏间距（PDF 1.6）。
    pub column_gap: Option<NumberOrNumbers>,
    /// `/ColumnWidths`：栏宽（PDF 1.6）。
    pub column_widths: Option<NumberOrNumbers>,
}

impl LayoutAttributes {
    /// 空 Layout 属性：不写出任何条目。
    pub fn new() -> Self {
        Self::default()
    }

    /// 设置 `/Placement`。
    pub fn with_placement(mut self, placement: Placement) -> Self {
        self.placement = Some(placement);
        self
    }

    /// 设置 `/WritingMode`。
    pub fn with_writing_mode(mut self, writing_mode: WritingMode) -> Self {
        self.writing_mode = Some(writing_mode);
        self
    }

    /// 设置 `/BackgroundColor`。
    pub fn with_background_color(mut self, color: [f64; 3]) -> Self {
        self.background_color = Some(color);
        self
    }

    /// 设置 `/BorderColor`。
    pub fn with_border_color(mut self, color: BorderColor) -> Self {
        self.border_color = Some(color);
        self
    }

    /// 设置 `/BorderStyle`。
    pub fn with_border_style(mut self, style: BorderStyles) -> Self {
        self.border_style = Some(style);
        self
    }

    /// 设置 `/BorderThickness`。
    pub fn with_border_thickness(mut self, thickness: BorderThickness) -> Self {
        self.border_thickness = Some(thickness);
        self
    }

    /// 设置 `/Color`。
    pub fn with_color(mut self, color: [f64; 3]) -> Self {
        self.color = Some(color);
        self
    }

    /// 设置 `/Padding`。
    pub fn with_padding(mut self, padding: NumberOrFour) -> Self {
        self.padding = Some(padding);
        self
    }

    /// 设置 `/SpaceBefore`。
    pub fn with_space_before(mut self, space: f64) -> Self {
        self.space_before = Some(space);
        self
    }

    /// 设置 `/SpaceAfter`。
    pub fn with_space_after(mut self, space: f64) -> Self {
        self.space_after = Some(space);
        self
    }

    /// 设置 `/StartIndent`。
    pub fn with_start_indent(mut self, indent: f64) -> Self {
        self.start_indent = Some(indent);
        self
    }

    /// 设置 `/EndIndent`。
    pub fn with_end_indent(mut self, indent: f64) -> Self {
        self.end_indent = Some(indent);
        self
    }

    /// 设置 `/TextIndent`。
    pub fn with_text_indent(mut self, indent: f64) -> Self {
        self.text_indent = Some(indent);
        self
    }

    /// 设置 `/TextAlign`。
    pub fn with_text_align(mut self, align: TextAlign) -> Self {
        self.text_align = Some(align);
        self
    }

    /// 设置 `/BBox`。
    pub fn with_bbox(mut self, bbox: [f64; 4]) -> Self {
        self.bbox = Some(bbox);
        self
    }

    /// 设置 `/Width`。
    pub fn with_width(mut self, width: NumberOrAuto) -> Self {
        self.width = Some(width);
        self
    }

    /// 设置 `/Height`。
    pub fn with_height(mut self, height: NumberOrAuto) -> Self {
        self.height = Some(height);
        self
    }

    /// 设置 `/BlockAlign`。
    pub fn with_block_align(mut self, align: BlockAlign) -> Self {
        self.block_align = Some(align);
        self
    }

    /// 设置 `/InlineAlign`。
    pub fn with_inline_align(mut self, align: InlineAlign) -> Self {
        self.inline_align = Some(align);
        self
    }

    /// 设置 `/TBorderStyle`。
    pub fn with_t_border_style(mut self, style: BorderStyles) -> Self {
        self.t_border_style = Some(style);
        self
    }

    /// 设置 `/TPadding`。
    pub fn with_t_padding(mut self, padding: NumberOrFour) -> Self {
        self.t_padding = Some(padding);
        self
    }

    /// 设置 `/BaselineShift`。
    pub fn with_baseline_shift(mut self, shift: f64) -> Self {
        self.baseline_shift = Some(shift);
        self
    }

    /// 设置 `/LineHeight`。
    pub fn with_line_height(mut self, height: LineHeight) -> Self {
        self.line_height = Some(height);
        self
    }

    /// 设置 `/TextDecorationColor`。
    pub fn with_text_decoration_color(mut self, color: [f64; 3]) -> Self {
        self.text_decoration_color = Some(color);
        self
    }

    /// 设置 `/TextDecorationThickness`。
    pub fn with_text_decoration_thickness(mut self, thickness: f64) -> Self {
        self.text_decoration_thickness = Some(thickness);
        self
    }

    /// 设置 `/TextDecorationType`。
    pub fn with_text_decoration_type(mut self, decoration: TextDecorationType) -> Self {
        self.text_decoration_type = Some(decoration);
        self
    }

    /// 设置 `/RubyAlign`。
    pub fn with_ruby_align(mut self, align: RubyAlign) -> Self {
        self.ruby_align = Some(align);
        self
    }

    /// 设置 `/RubyPosition`。
    pub fn with_ruby_position(mut self, position: RubyPosition) -> Self {
        self.ruby_position = Some(position);
        self
    }

    /// 设置 `/GlyphOrientationVertical`。
    pub fn with_glyph_orientation_vertical(
        mut self,
        orientation: GlyphOrientationVertical,
    ) -> Self {
        self.glyph_orientation_vertical = Some(orientation);
        self
    }

    /// 设置 `/ColumnCount`。
    pub fn with_column_count(mut self, count: u32) -> Self {
        self.column_count = Some(count);
        self
    }

    /// 设置 `/ColumnGap`。
    pub fn with_column_gap(mut self, gap: NumberOrNumbers) -> Self {
        self.column_gap = Some(gap);
        self
    }

    /// 设置 `/ColumnWidths`。
    pub fn with_column_widths(mut self, widths: NumberOrNumbers) -> Self {
        self.column_widths = Some(widths);
        self
    }
}
