//! 逻辑结构写端测试（ISO 32000-1 14.7）：目录 `/MarkInfo`（Table 321）、
//! 结构树根（Table 322）、结构元素（Table 323）、MCR（Table 324）、父树
//! 与 `/IDTree`（14.7.4.4）的写出与读端往返，以及版本门禁与输入校验的
//! 零字节输出契约。
//!
//! 按主题目录化拆分（与 `tests/shading/`、`tests/cid/`、`tests/structure_objr/` 的
//! 目录化形态一致）：
//! - `support`：跨主题共享夹具（构建重解析、非法/版本错误断言、单页构建器与
//!   属性对象构造辅助）；
//! - `tree_write`：结构树与元素的写出、前序展开、父树/`/IDTree` 往返与空树；
//! - `attributes`：属性对象与 `/ClassMap`（Table 327）的写出、流形态间接对象、
//!   数组类映射与属性修订号合并语义；
//! - `user_properties`：`/MarkInfo`（Table 321）与用户属性（Table 328）的写出、
//!   版本门禁、交叉校验与条目校验；
//! - `rejects`：非法元素名/`/ID`/类名/`/RoleMap`/`/ClassMap`/MCR 目标/上限镜像
//!   与属性 `/O` 在构建期拒绝且零字节输出；
//! - `version_gate`：结构树与元素文本条目的 PDF 1.3/1.4/1.5 版本门禁。
//!
//! 原 `structure.rs` 中被多个主题引用的夹具集中迁入 `support`（可见性放宽到
//! `pub(super)`），仅服务单一主题的夹具（`object_dict`、`simple_tree`、
//! `user_property_attribute`）随主题留在对应子文件，本文件仅保留子模块声明。

mod attributes;
mod rejects;
mod support;
mod tree_write;
mod user_properties;
mod version_gate;
