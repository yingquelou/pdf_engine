//! AcroForm 字段树对象分配与字典写出（ISO 32000-1 12.7.2–12.7.4）。
//!
//! 字段树在 assemble 主流程中先校验、再按前序分配对象号（根到叶），
//! 随后把每个节点写成独立间接对象：`/Fields`/`/Kids` 引用子节点，
//! 子节点 `/Parent` 回指父节点；带 `/Rect` 的终端节点是字段与 Widget
//! 合并字典，同时登记进页面 `/Annots`。`/AP` 句柄解析为 Form XObject
//! 流引用，与页面绘制及注解外观共享同一对象池。
//!
//! 子模块：
//! - `validate`：写前校验（字段类型专属条目、外观状态、`/CO` 名录）；
//! - `allocate`：前序对象号分配与计算顺序引用解析；
//! - `write`：单个字段/Widget 节点字典的写出。

mod allocate;
mod validate;
mod write;

use crate::core::builder::types::FormField;

pub(super) use allocate::{allocate_fields, calculation_order_nums};
pub(super) use validate::{
    appearance_handles, has_additional_actions, has_calculate_actions, validate,
};
pub(super) use write::field_dict;

/// 字段树节点及其已分配的对象号；`children` 为 `slots` 下标。
pub(super) struct FieldSlot<'a> {
    pub field: &'a FormField,
    pub num: i64,
    pub parent_num: Option<i64>,
    /// 生效页号（自身 `/P` 覆盖或从父节点继承）。
    pub page: Option<usize>,
    /// 全限定名（`/T` 链以 `.` 连接；无名 Widget 继承父名）。
    pub fqn: String,
    pub children: Vec<usize>,
}
