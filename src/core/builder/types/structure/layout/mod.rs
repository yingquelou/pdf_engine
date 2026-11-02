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
//!
//! 按主题目录化拆分（逐行搬运自原单文件）：
//! - `enums`：Table 343–345 的纯名字枚举（`Placement`、`WritingMode`、
//!   `BorderStyle`、`TextAlign`、`BlockAlign`、`InlineAlign`、
//!   `TextDecorationType`、`RubyAlign`、`RubyPosition`）；
//! - `forms`：单值与 four-entry 两种取值形态的联合枚举（`BorderStyles`、
//!   `BorderColor`、`BorderThickness`、`NumberOrFour`、`NumberOrAuto`、
//!   `LineHeight`、`GlyphOrientationVertical`、`NumberOrNumbers`）；
//! - `model`：`LayoutAttributes` 结构体与 `with_*` 构造器（Table 342–346）；
//! - `dict`：`/O /Layout` 属性字典组装与 `StructureAttribute::layout` 入口；
//! - `values`：名字、颜色、边框、数值与数组条目到 `Value` 的序列化辅助。

mod dict;
mod enums;
mod forms;
mod model;
mod values;

pub use enums::{
    BlockAlign, BorderStyle, InlineAlign, Placement, RubyAlign, RubyPosition, TextAlign,
    TextDecorationType, WritingMode,
};
pub use forms::{
    BorderColor, BorderStyles, BorderThickness, GlyphOrientationVertical, LineHeight, NumberOrAuto,
    NumberOrFour, NumberOrNumbers,
};
pub use model::LayoutAttributes;
