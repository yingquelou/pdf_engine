//! 标准结构属性写出 → 解析 → 回读往返（ISO 32000-1 14.8.5.2、14.8.5.5–14.8.5.7）。
//!
//! 两页文档：`Document` 下含列表（L/Lbl/LI，`/ListNumbering`）、表单域
//! （Form，PrintField `/Role` `/checked` `/Desc`）、表格（Table/TR/TH/TD，
//! TH 带 `/ID` `/RowSpan` `/Scope`，TD 带 `/ColSpan` 与指向 TH 的
//! `/Headers`，Table 带 `/Summary`）。解析后逐元素校验
//! `standard_attributes` 与特性画像。

use super::*;

mod attributes;
mod format_specific;
mod inherit;
mod layout;
mod types;
