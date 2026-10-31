//! Layout 标准结构属性读端（ISO 32000-1 14.8.5.4 Table 342–346）。
//!
//! 解析 `/O /Layout` 属性对象中的标准条目：
//!
//! - Table 343：`/Placement`、`/WritingMode`、`/BackgroundColor`、
//!   `/BorderColor`、`/BorderStyle`、`/BorderThickness`、`/Padding`、
//!   `/Color`；
//! - Table 344：`/SpaceBefore`、`/SpaceAfter`、`/StartIndent`、`/EndIndent`、
//!   `/TextIndent`、`/TextAlign`、`/BBox`、`/Width`、`/Height`、
//!   `/BlockAlign`；
//! - Table 345：`/InlineAlign`、`/TBorderStyle`、`/TPadding`、
//!   `/BaselineShift`、`/LineHeight`、`/TextDecorationColor`、
//!   `/TextDecorationThickness`、`/TextDecorationType`、`/RubyAlign`、
//!   `/RubyPosition`、`/GlyphOrientationVertical`；
//! - Table 346：`/ColumnCount`、`/ColumnGap`、`/ColumnWidths`。
//!
//! 每个条目保留 `None` 表示“未指定”，不把规范缺省值（如 `/Placement
//! Inline`、`/WritingMode LrTb`、`/Padding 0`）物化进类型化字段；调用方
//! 需要缺省语义时按规范自行套用。可继承条目的继承链解析（14.8.5.3）不在
//! 本模块内：这里只报告元素自身（含 `/ClassMap` 合并后）的属性。
//!
//! 所有值形态违规返回 [`StructureError::Malformed`]；列数组长度超过
//! [`MAX_LAYOUT_COLUMN_VALUES`] 返回 [`StructureError::LimitExceeded`]。

use crate::core::types::{Dict, Value};

use super::{StructureError, malformed};

/// `/ColumnGap` `/ColumnWidths` 数组的条目上限。
const MAX_LAYOUT_COLUMN_VALUES: usize = 65536;

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
    fn from_name(name: &str) -> Self {
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
    fn from_name(name: &str) -> Self {
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
    fn from_name(name: &str) -> Self {
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
    fn from_name(name: &str) -> Self {
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
    fn from_name(name: &str) -> Self {
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
    fn from_name(name: &str) -> Self {
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
    fn from_name(name: &str) -> Self {
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
    fn from_name(name: &str) -> Self {
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
    fn from_name(name: &str) -> Self {
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
    fn from_name(name: &str) -> Self {
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

/// 解析一个合并后的 `/O /Layout` 属性字典（Table 343–346）。
pub(super) fn parse(dict: &Dict, index: usize) -> Result<LayoutAttributes, StructureError> {
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

/// 存在时调用解析器，缺省返回 `None`。
fn optional<T>(
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
fn name_enum<T>(
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
fn glyph_orientation(
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

fn number(value: &Value, index: usize, key: &str) -> Result<f64, StructureError> {
    number_value(value).ok_or_else(|| {
        malformed(format!(
            "structure element {index} /{key} must be a finite number ({})",
            value.type_name()
        ))
    })
}

fn non_negative_number(value: &Value, index: usize, key: &str) -> Result<f64, StructureError> {
    let value = number(value, index, key)?;
    if value < 0.0 {
        return Err(malformed(format!(
            "structure element {index} /{key} must be non-negative ({value})"
        )));
    }
    Ok(value)
}

/// RGB 颜色：三个 0–1 的有限数值。
fn color(value: &Value, index: usize, key: &str) -> Result<[f64; 3], StructureError> {
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
fn bbox(value: &Value, index: usize, key: &str) -> Result<[f64; 4], StructureError> {
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
fn number_or_four(value: &Value, index: usize, key: &str) -> Result<NumberOrFour, StructureError> {
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
fn border_color(value: &Value, index: usize, key: &str) -> Result<BorderColor, StructureError> {
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
fn border_styles(value: &Value, index: usize, key: &str) -> Result<BorderStyles, StructureError> {
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
fn border_thickness(
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
fn number_or_auto(value: &Value, index: usize, key: &str) -> Result<NumberOrAuto, StructureError> {
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
fn line_height(value: &Value, index: usize, key: &str) -> Result<LineHeight, StructureError> {
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
fn column_count(value: &Value, index: usize, key: &str) -> Result<u32, StructureError> {
    match value {
        Value::Integer(count) if *count >= 1 && *count <= i64::from(u32::MAX) => Ok(*count as u32),
        other => Err(malformed(format!(
            "structure element {index} /{key} must be an integer of at least 1 ({})",
            other.type_name()
        ))),
    }
}

/// `/ColumnGap` `/ColumnWidths`：有限数值或非空有限数值数组。
fn number_or_numbers(
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
