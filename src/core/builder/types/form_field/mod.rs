//! 交互表单写端（ISO 32000-1 12.7.2–12.7.4）。
//!
//! [`AcroForm`] 持有表单级选项（`/NeedAppearances`、`/DA`、`/DR` 默认
//! 字体资源）与按页绑定的
//! 字段树；[`FormField`] 以链式构造声明字段条目（`/T`、`/FT`、`/Ff`、
//! `/V`/`/DV`、`/DA`、`/Q`、`/MaxLen`、`/Opt`、`/TI`、`/I`、`/TU`）与子字段。
//!
//! 带 [`FormField::new`] 矩形（`/Rect`）的终端节点是 Widget 注解，写端
//! 输出字段与 Widget 合并字典并登记进页面 `/Annots`；无矩形的节点是
//! 纯字段容器（`/Kids`）。`/AP` 外观流句柄指向
//! [`PdfBuilder::form_xobject`](super::super::PdfBuilder::form_xobject) 池。
//! 经 [`PdfBuilder::set_acro_form`](super::super::PdfBuilder::set_acro_form)
//! 注册后，由 assemble 写成目录 `/AcroForm` 与独立字段对象。
//!
//! 子模块：
//! - `field`：字段类型、值、选项与 [`FormField`] 本体；
//! - `acro_form`：[`AcroForm`] 表单级配置。

mod acro_form;
mod field;

pub use acro_form::AcroForm;
pub use field::{FieldActionEvent, FieldValue, FormField, FormFieldType, FormOption};
