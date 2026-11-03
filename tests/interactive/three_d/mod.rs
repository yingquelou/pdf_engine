//! 3D artwork 流（13.6 Table 300/303）与 3D 注解（Table 298）写出 → 解析 → 回读。
//!
//! 按主题拆分：
//! - `reference`：引用字典（Table 303）与 `/OnInstantiate` JavaScript 流；
//! - `annotation`：`/3DD` 视图注册、`/3DV` 选择与 `/3DA` 激活；
//! - `view`：视图 `/O` 叠加外观与 PDF 1.7 条目 `/RM` `/LS` `/SA` `/NA` `/NR`；
//! - `animation`：`/AN` 动画样式字典（Table 301/302）；
//! - `resources`：3D 流 `/Resources` 名称树（Table 300）；
//! - `markup_exdata`：`/ExData` 的 Markup3D 载荷（Table 313）。

use super::*;

mod animation;
mod annotation;
mod markup_exdata;
mod reference;
mod resources;
mod view;
