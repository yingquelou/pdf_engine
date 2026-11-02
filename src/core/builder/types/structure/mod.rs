//! 逻辑结构写端模型（ISO 32000-1 14.7.1–14.7.5、14.8.5.2、14.8.5.4–14.8.5.7）。
//!
//! 建模写端可写出的条目：目录 `/MarkInfo`（Table 321）、结构树根
//! `/StructTreeRoot`（Table 322）、结构元素（Table 323）、标记内容引用
//! MCR（Table 324）与结构属性对象 `/A`（14.7.5 Table 327–329，含用户
//! 属性）。标准属性所有者（14.8.5.2 Table 341）与 Layout/List/PrintField/
//! Table 条目（14.8.5.4–14.8.5.7 Table 342–349）由子模块 [`layout`]、
//! [`standard`] 与 [`format_specific`] 提供类型化构造器。
//! 对象引用 OBJR（Table 325）以 [`StructureKid::ObjectReference`] 建模，
//! `/Obj` 的目标类别是 [`StructureObjectReference`]：注解字典、文档级 Form
//! XObject 与页面资源里的图像 XObject（14.7.4.3 的「an entire PDF object,
//! such as an XObject or an annotation」）；标记内容序列以 MCR 字典
//! （[`StructureKid::MarkedContent`]）或 14.7.4.2 的整数简写
//! （[`StructureKid::InlineMarkedContent`]，须配合元素 `/Pg`）两式建模；
//! 序列位于页面内容流之外的流形态 MCR（Table 324 的 `/Stm` `/StmOwn`，
//! 14.7.4.2 EXAMPLE 5）以 [`StructureKid::StreamMarkedContent`] 建模。
//!
//! 结构元素以拥有所有权的树形 `Vec` 表达（`StructureKid::Element` 装箱
//! 子元素），与读端的 arena 前序模型不同：写端在 `PdfBuilder::build`
//! 时才展开为前序并分配对象号，调用方按文档顺序构造即可。

mod attributes;
mod element;
mod format_specific;
mod kid;
mod layout;
mod model;
mod standard;

pub use attributes::{StructureAttribute, UserProperty};
pub use element::StructureElement;
pub use format_specific::{FormatSpecificAttributes, FormatSpecificOwner};
pub use kid::{StructureKid, StructureObjectReference};
pub use layout::{
    BlockAlign, BorderColor, BorderStyle, BorderStyles, BorderThickness, GlyphOrientationVertical,
    InlineAlign, LayoutAttributes, LineHeight, NumberOrAuto, NumberOrFour, NumberOrNumbers,
    Placement, RubyAlign, RubyPosition, TextAlign, TextDecorationType, WritingMode,
};
pub use model::{StructureMarkInfo, StructureTree};
pub use standard::{
    ListAttributes, ListNumbering, PrintFieldAttributes, PrintFieldChecked, PrintFieldRole,
    TableAttributes, TableScope,
};
