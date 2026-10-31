//! Layout 标准属性写端类型化构造器（ISO 32000-1 14.8.5.4 Table 342–346）。
//!
//! [`LayoutAttributes`] 只物化调用方显式设置的条目：所有字段均为
//! `Option`，未设置即不写出，规范缺省值（`Placement` 缺省 `Inline`、
//! `BorderStyle` 缺省 `None`、`Padding`/`TPadding` 缺省 0 等）不会自动
//! 写入属性字典。`/BorderColor`、`/BorderStyle`、`/BorderThickness` 支持
//! 单值与 four-entry（before/after/start/end）两种形态；`/Padding`
//! `/TPadding` 支持单值或四数字数组。
//!
//! 元素适用性、值域与版本门禁由构建器在分配对象号之前完成（见
//! `builder/navigation/structure_standard.rs` 与 `structure_layout.rs`）；
//! 本模块只负责生成符合 Table 342–346 的值形态。

use crate::core::types::{Array, Dict, Value};

use super::StructureAttribute;

/// 元素放置方式（Table 343 `/Placement`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Placement {
    /// `/Block`：沿块方向堆叠。
    Block,
    /// `/Inline`：沿行内方向排列（缺省）。
    Inline,
    /// `/Before`：浮到参考区 before 边。
    Before,
    /// `/Start`：浮到参考区 start 边。
    Start,
    /// `/End`：浮到参考区 end 边。
    End,
    /// 未知名字，按原名字写出。
    Other(String),
}

impl Placement {
    /// `/Placement` 的 PDF 名字（不含前导 `/`）。
    pub fn name(&self) -> &str {
        match self {
            Self::Block => "Block",
            Self::Inline => "Inline",
            Self::Before => "Before",
            Self::Start => "Start",
            Self::End => "End",
            Self::Other(name) => name,
        }
    }
}

/// 书写方向（Table 343 `/WritingMode`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WritingMode {
    /// `/LrTb`：行内从左到右、块从上到下（缺省）。
    LrTb,
    /// `/RlTb`：行内从右到左、块从上到下。
    RlTb,
    /// `/TbRl`：行内从上到下、块从右到左。
    TbRl,
    /// 未知名字，按原名字写出。
    Other(String),
}

impl WritingMode {
    /// `/WritingMode` 的 PDF 名字（不含前导 `/`）。
    pub fn name(&self) -> &str {
        match self {
            Self::LrTb => "LrTb",
            Self::RlTb => "RlTb",
            Self::TbRl => "TbRl",
            Self::Other(name) => name,
        }
    }
}

/// 边框样式（Table 343 `/BorderStyle`、Table 345 `/TBorderStyle`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BorderStyle {
    /// `/None`：无边框（缺省）。
    None,
    /// `/Hidden`：无边框，但参与表格边框冲突消解。
    Hidden,
    /// `/Dotted`：点线。
    Dotted,
    /// `/Dashed`：短划线。
    Dashed,
    /// `/Solid`：实线。
    Solid,
    /// `/Double`：双线。
    Double,
    /// `/Groove`：凹槽。
    Groove,
    /// `/Ridge`：凸脊。
    Ridge,
    /// `/Inset`：内嵌。
    Inset,
    /// `/Outset`：外凸。
    Outset,
    /// 未知名字，按原名字写出。
    Other(String),
}

impl BorderStyle {
    /// 边框样式的 PDF 名字（不含前导 `/`）。
    pub fn name(&self) -> &str {
        match self {
            Self::None => "None",
            Self::Hidden => "Hidden",
            Self::Dotted => "Dotted",
            Self::Dashed => "Dashed",
            Self::Solid => "Solid",
            Self::Double => "Double",
            Self::Groove => "Groove",
            Self::Ridge => "Ridge",
            Self::Inset => "Inset",
            Self::Outset => "Outset",
            Self::Other(name) => name,
        }
    }
}

/// `/BorderStyle` `/TBorderStyle` 的单值或四边形态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BorderStyles {
    /// 四个边统一使用同一样式。
    Uniform(BorderStyle),
    /// before/after/start/end 四边；`None` 表示该边不绘制。
    Edges([Option<BorderStyle>; 4]),
}

/// `/BorderColor` 的单值或四边形态。
#[derive(Debug, Clone, PartialEq)]
pub enum BorderColor {
    /// 四个边统一使用同一 RGB 颜色（分量 0–1）。
    Uniform([f64; 3]),
    /// before/after/start/end 四边；`None` 表示该边不绘制。
    Edges([Option<[f64; 3]>; 4]),
}

/// `/Padding` `/TPadding` 的单值或四数字数组形态。
#[derive(Debug, Clone, PartialEq)]
pub enum NumberOrFour {
    /// 四边统一使用同一数值。
    Uniform(f64),
    /// before/after/start/end 四边的数值。
    Edges([f64; 4]),
}

/// `/BorderThickness` 的单值或四边形态。
#[derive(Debug, Clone, PartialEq)]
pub enum BorderThickness {
    /// 四边统一使用同一厚度（非负）。
    Uniform(f64),
    /// before/after/start/end 四边；`None` 表示该边不绘制。
    Edges([Option<f64>; 4]),
}

/// `/Width` `/Height` 的数值或 `Auto` 形态。
#[derive(Debug, Clone, PartialEq)]
pub enum NumberOrAuto {
    /// 具体数值。
    Number(f64),
    /// `/Auto`：不施加尺寸约束（缺省）。
    Auto,
    /// 未知名字，按原名字写出。
    Other(String),
}

impl NumberOrAuto {
    fn name(&self) -> &str {
        match self {
            Self::Auto => "Auto",
            Self::Other(name) => name,
            Self::Number(_) => "Auto",
        }
    }
}

/// 行内方向对齐（Table 344 `/TextAlign`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextAlign {
    /// `/Start`：对齐 start 边（缺省）。
    Start,
    /// `/Center`：居中。
    Center,
    /// `/End`：对齐 end 边。
    End,
    /// `/Justify`：两端对齐。
    Justify,
    /// 未知名字，按原名字写出。
    Other(String),
}

impl TextAlign {
    /// `/TextAlign` 的 PDF 名字（不含前导 `/`）。
    pub fn name(&self) -> &str {
        match self {
            Self::Start => "Start",
            Self::Center => "Center",
            Self::End => "End",
            Self::Justify => "Justify",
            Self::Other(name) => name,
        }
    }
}

/// 表格单元块方向对齐（Table 344 `/BlockAlign`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockAlign {
    /// `/Before`：对齐 before 边（缺省）。
    Before,
    /// `/Middle`：居中。
    Middle,
    /// `/After`：对齐 after 边。
    After,
    /// `/Justify`：两端对齐。
    Justify,
    /// 未知名字，按原名字写出。
    Other(String),
}

impl BlockAlign {
    /// `/BlockAlign` 的 PDF 名字（不含前导 `/`）。
    pub fn name(&self) -> &str {
        match self {
            Self::Before => "Before",
            Self::Middle => "Middle",
            Self::After => "After",
            Self::Justify => "Justify",
            Self::Other(name) => name,
        }
    }
}

/// 表格单元行内方向对齐（Table 345 `/InlineAlign`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InlineAlign {
    /// `/Start`：对齐 start 边（缺省）。
    Start,
    /// `/Center`：居中。
    Center,
    /// `/End`：对齐 end 边。
    End,
    /// 未知名字，按原名字写出。
    Other(String),
}

impl InlineAlign {
    /// `/InlineAlign` 的 PDF 名字（不含前导 `/`）。
    pub fn name(&self) -> &str {
        match self {
            Self::Start => "Start",
            Self::Center => "Center",
            Self::End => "End",
            Self::Other(name) => name,
        }
    }
}

/// 行高（Table 345 `/LineHeight`）：数值、`Normal`、`Auto` 或扩展名字。
#[derive(Debug, Clone, PartialEq)]
pub enum LineHeight {
    /// 具体数值。
    Number(f64),
    /// `/Normal`：按字号调整行高，并计入 `BaselineShift`（缺省）。
    Normal,
    /// `/Auto`：按字号调整行高，不计入 `BaselineShift`。
    Auto,
    /// 未知名字，按原名字写出。
    Other(String),
}

impl LineHeight {
    fn name(&self) -> &str {
        match self {
            Self::Normal => "Normal",
            Self::Auto => "Auto",
            Self::Other(name) => name,
            Self::Number(_) => "Normal",
        }
    }
}

/// 文本装饰类型（Table 345 `/TextDecorationType`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextDecorationType {
    /// `/None`：无装饰（缺省）。
    None,
    /// `/Underline`：下划线。
    Underline,
    /// `/Overline`：上划线。
    Overline,
    /// `/LineThrough`：删除线。
    LineThrough,
    /// 未知名字，按原名字写出。
    Other(String),
}

impl TextDecorationType {
    /// `/TextDecorationType` 的 PDF 名字（不含前导 `/`）。
    pub fn name(&self) -> &str {
        match self {
            Self::None => "None",
            Self::Underline => "Underline",
            Self::Overline => "Overline",
            Self::LineThrough => "LineThrough",
            Self::Other(name) => name,
        }
    }
}

/// Ruby 对齐方式（Table 345 `/RubyAlign`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RubyAlign {
    /// `/Start`：对齐 start 边。
    Start,
    /// `/Center`：居中。
    Center,
    /// `/End`：对齐 end 边。
    End,
    /// `/Justify`：填满可用宽度。
    Justify,
    /// `/Distribute`：填满宽度并在两端加空隙（缺省）。
    Distribute,
    /// 未知名字，按原名字写出。
    Other(String),
}

impl RubyAlign {
    /// `/RubyAlign` 的 PDF 名字（不含前导 `/`）。
    pub fn name(&self) -> &str {
        match self {
            Self::Start => "Start",
            Self::Center => "Center",
            Self::End => "End",
            Self::Justify => "Justify",
            Self::Distribute => "Distribute",
            Self::Other(name) => name,
        }
    }
}

/// Ruby 注音位置（Table 345 `/RubyPosition`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RubyPosition {
    /// `/Before`：注音位于 before 边（缺省）。
    Before,
    /// `/After`：注音位于 after 边。
    After,
    /// `/Warichu`：按 warichu 排版。
    Warichu,
    /// `/Inline`：按括号注释排版。
    Inline,
    /// 未知名字，按原名字写出。
    Other(String),
}

impl RubyPosition {
    /// `/RubyPosition` 的 PDF 名字（不含前导 `/`）。
    pub fn name(&self) -> &str {
        match self {
            Self::Before => "Before",
            Self::After => "After",
            Self::Warichu => "Warichu",
            Self::Inline => "Inline",
            Self::Other(name) => name,
        }
    }
}

/// 竖排字形方向（Table 345 `/GlyphOrientationVertical`）。
#[derive(Debug, Clone, PartialEq)]
pub enum GlyphOrientationVertical {
    /// `/Auto`：按字形宽度自动选择（缺省）。
    Auto,
    /// 顺时针旋转角度，须为 90 的倍数且在 −180 到 360 之间。
    Angle(f64),
    /// 未知名字，按原名字写出。
    Other(String),
}

impl GlyphOrientationVertical {
    fn name(&self) -> &str {
        match self {
            Self::Auto => "Auto",
            Self::Other(name) => name,
            Self::Angle(_) => "Auto",
        }
    }
}

/// `/ColumnGap` `/ColumnWidths` 的单值或数字数组形态。
#[derive(Debug, Clone, PartialEq)]
pub enum NumberOrNumbers {
    /// 所有列使用同一数值。
    Number(f64),
    /// 按列给出的数值数组（须非空）。
    Array(Vec<f64>),
}

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

fn name_value(name: &str) -> Value {
    Value::Name(name.to_owned())
}

fn color_value(color: [f64; 3]) -> Value {
    let mut array = Array::new();
    for component in color {
        array = array.push(Value::Float(component));
    }
    Value::Array(array)
}

fn four_numbers(numbers: [f64; 4]) -> Value {
    let mut array = Array::new();
    for number in numbers {
        array = array.push(Value::Float(number));
    }
    Value::Array(array)
}

fn border_color_value(color: &BorderColor) -> Value {
    match color {
        BorderColor::Uniform(color) => color_value(*color),
        BorderColor::Edges(edges) => {
            let mut array = Array::new();
            for edge in edges {
                array = array.push(edge.map_or(Value::Null, color_value));
            }
            Value::Array(array)
        }
    }
}

fn border_styles_value(style: &BorderStyles) -> Value {
    match style {
        BorderStyles::Uniform(style) => name_value(style.name()),
        BorderStyles::Edges(edges) => {
            let mut array = Array::new();
            for edge in edges {
                array = array.push(
                    edge.as_ref()
                        .map_or(Value::Null, |style| name_value(style.name())),
                );
            }
            Value::Array(array)
        }
    }
}

fn border_thickness_value(thickness: &BorderThickness) -> Value {
    match thickness {
        BorderThickness::Uniform(thickness) => Value::Float(*thickness),
        BorderThickness::Edges(edges) => {
            let mut array = Array::new();
            for edge in edges {
                array = array.push(edge.map_or(Value::Null, Value::Float));
            }
            Value::Array(array)
        }
    }
}

fn number_or_four_value(padding: &NumberOrFour) -> Value {
    match padding {
        NumberOrFour::Uniform(number) => Value::Float(*number),
        NumberOrFour::Edges(edges) => four_numbers(*edges),
    }
}

fn number_or_auto_value(size: &NumberOrAuto) -> Value {
    match size {
        NumberOrAuto::Number(number) => Value::Float(*number),
        other => name_value(other.name()),
    }
}

fn number_or_numbers_value(value: &NumberOrNumbers) -> Value {
    match value {
        NumberOrNumbers::Number(number) => Value::Float(*number),
        NumberOrNumbers::Array(numbers) => {
            let mut array = Array::new();
            for number in numbers {
                array = array.push(Value::Float(*number));
            }
            Value::Array(array)
        }
    }
}
