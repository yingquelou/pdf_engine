//! 结构元素 OBJR 内容项（ISO 32000-1 14.7.4.3 Table 325、14.7.4.4 与
//! Table 326 `/StructParent`）的写端测试：`/K` 对象引用字典（`/Type`
//! `/OBJR`、间接 `/Pg`、间接 `/Obj`）、父树单元引用条目与页面数组条目
//! 的区分、父树键先页面后 OBJR（按目标对象号升序）的连续分配、目标字典
//! `/StructParent` 追加，以及句柄/页/资源序号越界、Form 未在该页绘制、
//! 两路都取不到 `/Pg`、按解析对象号判重的重复声称与所在页越界校验的
//! 零字节输出契约。
//!
//! `/Obj` 的目标（14.7.4.3）覆盖三档：注解字典、文档级 Form XObject 与页
//! 资源里的图像 XObject。Table 326 的 `/StructParent` ↔ `/StructParents`
//! 互斥与目标形态（注解不得是流、XObject 须带对应 `/Subtype`、目标对象须
//! 存在）在结构对象号分配之前拒绝；这些分支中「目标字典已带
//! `/StructParent`/`/StructParents`」与类别错配在公开 API 上不可达（写端
//! 不给目标字典预置这两个条目），故按本目录既有模式直接调用
//! `navigation::apply` 并传入自定义 `StructureTargets` 触发。
//!
//! 按主题继续拆分（与 `tests/main/`、`tests/rendition/` 的目录化形态一致）：
//! - `annotation_target`：注解目标的 `/K` OBJR 写出形态与父树联动；
//! - `annotation_rejects`：注解目标的句柄越界/重复声称拒绝与整档零字节契约；
//! - `form_target`：Form XObject 目标的写出、`/Pg` 三档解析与拒绝路径；
//! - `image_target`：图像 XObject 目标的写出、对象号判重与越界拒绝；
//! - `table326_contract`：Table 326 `/StructParent` ↔ `/StructParents` 互斥
//!   与目标形态/类别校验；
//! - `support`：本主题各文件共享的夹具（构建后重解析、零字节断言、最小
//!   结构树与三类目标构造、解析侧字典/父树/资源引用取用）。
//!
//! 这些夹具原先直接住在本主题文件里，且不被其它测试文件引用（全仓 grep 只
//! 命中 `tests/mod.rs` 的 `mod structure_objr;`），故子模块统一
//! `use super::support::*;` 取得，可见性保持 `pub(super)`，不外泄到 `pub`。

mod annotation_rejects;
mod annotation_target;
mod form_target;
mod image_target;
mod support;
mod table326_contract;
