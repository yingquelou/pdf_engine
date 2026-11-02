//! Layout 读端模型：Table 343–346 的类型化枚举与 `LayoutAttributes` 结构体。

use crate::core::types::Dict;

/// 元素定位方式（Table 343 `/Placement`），缺省 `Inline`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Placement {
    /// `/Block`：在父级块内按块方向堆叠。
    Block,
    /// `/Inline`：在父级块内按行内方向排布。
    Inline,
    /// `/Before`：浮动到参考区之前边缘。
    Before,
    /// `/Start`：浮动到参考区起始边缘。
    Start,
    /// `/End`：浮动到参考区结束边缘。
    End,
    /// 未知名字，保留原名。
    Other(String),
}

impl Placement {
    pub(super) fn from_name(name: &str) -> Self {
        match name {
            "Block" => Self::Block,
            "Inline" => Self::Inline,
            "Before" => Self::Before,
            "Start" => Self::Start,
            "End" => Self::End,
            other => Self::Other(other.to_owned()),
        }
    }
}

/// 书写方向（Table 343 `/WritingMode`），缺省 `LrTb`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WritingMode {
    /// `/LrTb`：行内从左到右，块方向从上到下。
    LrTb,
    /// `/RlTb`：行内从右到左，块方向从上到下。
    RlTb,
    /// `/TbRl`：行内从上到下，块方向从右到左。
    TbRl,
    /// 未知名字，保留原名。
    Other(String),
}

impl WritingMode {
    pub(super) fn from_name(name: &str) -> Self {
        match name {
            "LrTb" => Self::LrTb,
            "RlTb" => Self::RlTb,
            "TbRl" => Self::TbRl,
            other => Self::Other(other.to_owned()),
        }
    }
}

/// 边框样式（Table 343 `/BorderStyle`、Table 345 `/TBorderStyle`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BorderStyle {
    /// `/None`：无边框。
    None,
    /// `/Hidden`：与 None 相同，但参与表格边框冲突消解。
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
    /// 未知名字，保留原名。
    Other(String),
}

impl BorderStyle {
    pub(super) fn from_name(name: &str) -> Self {
        match name {
            "None" => Self::None,
            "Hidden" => Self::Hidden,
            "Dotted" => Self::Dotted,
            "Dashed" => Self::Dashed,
            "Solid" => Self::Solid,
            "Double" => Self::Double,
            "Groove" => Self::Groove,
            "Ridge" => Self::Ridge,
            "Inset" => Self::Inset,
            "Outset" => Self::Outset,
            other => Self::Other(other.to_owned()),
        }
    }
}

/// 四边统一的边框样式，或按 before/after/start/end 的四项数组。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BorderStyles {
    /// 四边使用同一样式（单个名字）。
    Uniform(BorderStyle),
    /// 四边各自的样式；`None` 表示该边不绘制（`null`）。
    Edges([Option<BorderStyle>; 4]),
}

/// 边框颜色（Table 343 `/BorderColor`）。
#[derive(Debug, Clone, PartialEq)]
pub enum BorderColor {
    /// 四边统一的 RGB 颜色（三个 0–1 分量）。
    Uniform([f64; 3]),
    /// 四边各自的 RGB 颜色；`None` 表示该边不绘制（`null`）。
    Edges([Option<[f64; 3]>; 4]),
}

/// 四边统一的数值，或按 before/after/start/end 的四项数组。
#[derive(Debug, Clone, PartialEq)]
pub enum NumberOrFour {
    /// 四边统一的值。
    Uniform(f64),
    /// 四边各自的值（before、after、start、end）。
    Edges([f64; 4]),
}

/// 边框厚度（Table 343 `/BorderThickness`）：四边统一数值或带 `null` 的四项数组。
#[derive(Debug, Clone, PartialEq)]
pub enum BorderThickness {
    /// 四边统一的厚度。
    Uniform(f64),
    /// 四边各自的厚度；`None` 表示该边不绘制（`null`）。
    Edges([Option<f64>; 4]),
}

/// 数值或名字 `Auto`（Table 344 `/Width` `/Height`）。
#[derive(Debug, Clone, PartialEq)]
pub enum NumberOrAuto {
    /// 具体数值。
    Number(f64),
    /// `/Auto`：不施加尺寸约束。
    Auto,
    /// 其它名字，保留原名。
    Other(String),
}

/// 文本对齐方式（Table 344 `/TextAlign`），缺省 `Start`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextAlign {
    /// `/Start`：起始边缘对齐。
    Start,
    /// `/Center`：居中。
    Center,
    /// `/End`：结束边缘对齐。
    End,
    /// `/Justify`：两端对齐。
    Justify,
    /// 未知名字，保留原名。
    Other(String),
}

impl TextAlign {
    pub(super) fn from_name(name: &str) -> Self {
        match name {
            "Start" => Self::Start,
            "Center" => Self::Center,
            "End" => Self::End,
            "Justify" => Self::Justify,
            other => Self::Other(other.to_owned()),
        }
    }
}

/// 表格单元块方向对齐（Table 344 `/BlockAlign`），缺省 `Before`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockAlign {
    /// `/Before`：与单元内容区之前边缘对齐。
    Before,
    /// `/Middle`：居中。
    Middle,
    /// `/After`：与之后边缘对齐。
    After,
    /// `/Justify`：两端对齐。
    Justify,
    /// 未知名字，保留原名。
    Other(String),
}

impl BlockAlign {
    pub(super) fn from_name(name: &str) -> Self {
        match name {
            "Before" => Self::Before,
            "Middle" => Self::Middle,
            "After" => Self::After,
            "Justify" => Self::Justify,
            other => Self::Other(other.to_owned()),
        }
    }
}

/// 表格单元行内方向对齐（Table 345 `/InlineAlign`），缺省 `Start`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InlineAlign {
    /// `/Start`：起始边缘对齐。
    Start,
    /// `/Center`：居中。
    Center,
    /// `/End`：结束边缘对齐。
    End,
    /// 未知名字，保留原名。
    Other(String),
}

impl InlineAlign {
    pub(super) fn from_name(name: &str) -> Self {
        match name {
            "Start" => Self::Start,
            "Center" => Self::Center,
            "End" => Self::End,
            other => Self::Other(other.to_owned()),
        }
    }
}

/// 行高（Table 345 `/LineHeight`），缺省 `Normal`。
#[derive(Debug, Clone, PartialEq)]
pub enum LineHeight {
    /// 具体行高（默认用户空间单位）。
    Number(f64),
    /// `/Normal`：按内容字号推导，并计入 `BaselineShift`。
    Normal,
    /// `/Auto`：按内容字号推导，不计入 `BaselineShift`。
    Auto,
    /// 其它名字，保留原名。
    Other(String),
}

/// 文本装饰类型（Table 345 `/TextDecorationType`），缺省 `None`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TextDecorationType {
    /// `/None`：无装饰。
    None,
    /// `/Underline`：下划线。
    Underline,
    /// `/Overline`：上划线。
    Overline,
    /// `/LineThrough`：删除线。
    LineThrough,
    /// 未知名字，保留原名。
    Other(String),
}

impl TextDecorationType {
    pub(super) fn from_name(name: &str) -> Self {
        match name {
            "None" => Self::None,
            "Underline" => Self::Underline,
            "Overline" => Self::Overline,
            "LineThrough" => Self::LineThrough,
            other => Self::Other(other.to_owned()),
        }
    }
}

/// Ruby 行对齐（Table 345 `/RubyAlign`），缺省 `Distribute`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RubyAlign {
    /// `/Start`：起始边缘对齐。
    Start,
    /// `/Center`：居中。
    Center,
    /// `/End`：结束边缘对齐。
    End,
    /// `/Justify`：两端对齐。
    Justify,
    /// `/Distribute`：均匀分布并在两端加空。
    Distribute,
    /// 未知名字，保留原名。
    Other(String),
}

impl RubyAlign {
    pub(super) fn from_name(name: &str) -> Self {
        match name {
            "Start" => Self::Start,
            "Center" => Self::Center,
            "End" => Self::End,
            "Justify" => Self::Justify,
            "Distribute" => Self::Distribute,
            other => Self::Other(other.to_owned()),
        }
    }
}

/// Ruby 位置（Table 345 `/RubyPosition`），缺省 `Before`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RubyPosition {
    /// `/Before`：置于 before 边缘。
    Before,
    /// `/After`：置于 after 边缘。
    After,
    /// `/Warichu`：按 warichu 方式排版。
    Warichu,
    /// `/Inline`：作为括号注释排在行内。
    Inline,
    /// 未知名字，保留原名。
    Other(String),
}

impl RubyPosition {
    pub(super) fn from_name(name: &str) -> Self {
        match name {
            "Before" => Self::Before,
            "After" => Self::After,
            "Warichu" => Self::Warichu,
            "Inline" => Self::Inline,
            other => Self::Other(other.to_owned()),
        }
    }
}

/// 竖排字形朝向（Table 345 `/GlyphOrientationVertical`），缺省 `Auto`。
#[derive(Debug, Clone, PartialEq)]
pub enum GlyphOrientationVertical {
    /// `/Auto`：按全角/非全角自动选择。
    Auto,
    /// 顺时针旋转角度（-180–360，90 的倍数）。
    Angle(f64),
    /// 其它名字，保留原名。
    Other(String),
}

impl GlyphOrientationVertical {
    pub(super) fn from_name(name: &str) -> Self {
        if name == "Auto" {
            return Self::Auto;
        }
        let Some(angle) = name.parse::<f64>().ok() else {
            return Self::Other(name.to_owned());
        };
        if angle.is_finite() && (-180.0..=360.0).contains(&angle) && angle.rem_euclid(90.0) == 0.0 {
            Self::Angle(angle)
        } else {
            Self::Other(name.to_owned())
        }
    }
}

/// 数值或数值数组（Table 346 `/ColumnGap` `/ColumnWidths`）。
#[derive(Debug, Clone, PartialEq)]
pub enum NumberOrNumbers {
    /// 单个数值。
    Number(f64),
    /// 数值数组。
    Array(Vec<f64>),
}

/// Layout 标准属性（14.8.5.4 Table 342–346，`/O /Layout`）。
///
/// 每个字段为 `None` 表示该条目未指定；规范缺省值不在此物化。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LayoutAttributes {
    /// `/Placement`：定位方式（缺省 `Inline`）。
    pub placement: Option<Placement>,
    /// `/WritingMode`：书写方向（缺省 `LrTb`）。
    pub writing_mode: Option<WritingMode>,
    /// `/BackgroundColor`：背景色（PDF 1.5）。
    pub background_color: Option<[f64; 3]>,
    /// `/BorderColor`：边框颜色（PDF 1.5）。
    pub border_color: Option<BorderColor>,
    /// `/BorderStyle`：边框样式（PDF 1.5，缺省 `None`）。
    pub border_style: Option<BorderStyles>,
    /// `/BorderThickness`：边框厚度（PDF 1.5）。
    pub border_thickness: Option<BorderThickness>,
    /// `/Color`：文本与默认边框颜色（PDF 1.5）。
    pub color: Option<[f64; 3]>,
    /// `/Padding`：内容矩形与边框的间距（PDF 1.5，缺省 0）。
    pub padding: Option<NumberOrFour>,
    /// `/SpaceBefore`：块前额外间距（缺省 0）。
    pub space_before: Option<f64>,
    /// `/SpaceAfter`：块后额外间距（缺省 0）。
    pub space_after: Option<f64>,
    /// `/StartIndent`：起始缩进（缺省 0）。
    pub start_indent: Option<f64>,
    /// `/EndIndent`：结束缩进（缺省 0）。
    pub end_indent: Option<f64>,
    /// `/TextIndent`：首行缩进（缺省 0）。
    pub text_indent: Option<f64>,
    /// `/TextAlign`：行内对齐（缺省 `Start`）。
    pub text_align: Option<TextAlign>,
    /// `/BBox`：单页包围盒（left、bottom、right、top）。
    pub bbox: Option<[f64; 4]>,
    /// `/Width`：内容矩形宽度（缺省 `Auto`）。
    pub width: Option<NumberOrAuto>,
    /// `/Height`：内容矩形高度（缺省 `Auto`）。
    pub height: Option<NumberOrAuto>,
    /// `/BlockAlign`：表格单元块方向对齐（缺省 `Before`）。
    pub block_align: Option<BlockAlign>,
    /// `/InlineAlign`：表格单元行内方向对齐（缺省 `Start`）。
    pub inline_align: Option<InlineAlign>,
    /// `/TBorderStyle`：表格单元边框样式（PDF 1.5，缺省 `None`）。
    pub t_border_style: Option<BorderStyles>,
    /// `/TPadding`：表格单元内边距（PDF 1.5，缺省 0）。
    pub t_padding: Option<NumberOrFour>,
    /// `/BaselineShift`：基线偏移（缺省 0）。
    pub baseline_shift: Option<f64>,
    /// `/LineHeight`：行高（缺省 `Normal`）。
    pub line_height: Option<LineHeight>,
    /// `/TextDecorationColor`：文本装饰颜色（PDF 1.5）。
    pub text_decoration_color: Option<[f64; 3]>,
    /// `/TextDecorationThickness`：文本装饰线宽（PDF 1.5）。
    pub text_decoration_thickness: Option<f64>,
    /// `/TextDecorationType`：文本装饰类型（缺省 `None`）。
    pub text_decoration_type: Option<TextDecorationType>,
    /// `/RubyAlign`：Ruby 行对齐（PDF 1.5，缺省 `Distribute`）。
    pub ruby_align: Option<RubyAlign>,
    /// `/RubyPosition`：Ruby 位置（PDF 1.5，缺省 `Before`）。
    pub ruby_position: Option<RubyPosition>,
    /// `/GlyphOrientationVertical`：竖排字形朝向（PDF 1.5，缺省 `Auto`）。
    pub glyph_orientation_vertical: Option<GlyphOrientationVertical>,
    /// `/ColumnCount`：列数（PDF 1.6，缺省 1）。
    pub column_count: Option<u32>,
    /// `/ColumnGap`：列间距（PDF 1.6）。
    pub column_gap: Option<NumberOrNumbers>,
    /// `/ColumnWidths`：列宽（PDF 1.6）。
    pub column_widths: Option<NumberOrNumbers>,
    /// 合并后的属性字典（保留未建模条目）。
    pub dict: Dict,
}
