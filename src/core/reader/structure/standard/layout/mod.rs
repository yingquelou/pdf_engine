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

mod model;
mod parsing;
mod values;

pub use model::{
    BlockAlign, BorderColor, BorderStyle, BorderStyles, BorderThickness, GlyphOrientationVertical,
    InlineAlign, LayoutAttributes, LineHeight, NumberOrAuto, NumberOrFour, NumberOrNumbers,
    Placement, RubyAlign, RubyPosition, TextAlign, TextDecorationType, WritingMode,
};
pub(in crate::core::reader::structure::standard) use parsing::parse;
