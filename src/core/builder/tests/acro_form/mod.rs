//! AcroForm 写端测试（ISO 32000-1 12.7.2–12.7.4）：目录 `/AcroForm`、
//! 字段树 `/Kids`/`/Parent`、Widget 合并字典与 `/Annots`、`/Opt` 与
//! `/AP` 外观流引用。
//!
//! 按主题目录化拆分（与 `tests/shading/`、`tests/structure/`、`tests/cid/`、
//! `tests/structure_objr/` 的目录化形态一致）：
//! - `support`：跨主题共享夹具（目录/字段字典取用、文本串条目断言、字段级
//!   构建失败断言）；
//! - `field_write`：字段树 `/Kids`/`/Parent` 与 `/T` 全限定名、Widget 合并字典
//!   条目与页面 `/Annots` 注册、`/Opt` 与旗标、`/P` 覆盖页绑定；
//! - `appearance`：Widget `/AP` 外观流对表单 XObject 池的复用引用；
//! - `default_fonts`：`/AcroForm /DR /Font` 默认字体资源写出与页面字体共享对象；
//! - `field_actions`：字段 `/AA` 附加动作字典的写出、回读与事件校验；
//! - `calculation_order`：`/CO` 计算顺序的显式声明、缺省前序展开与条目校验；
//! - `alternate_name`：Table 220 `/TU` 替代字段名的写出回读、门禁谓词与
//!   `has_alternate_names` 覆盖形态；
//! - `rejects`：非法字段树、`/AP` 句柄越界与默认字体名重复/非法的零字节拒绝；
//! - `version_gate`：AcroForm 的 PDF 1.2 门禁与 `/AA` 的 PDF 1.3 门禁。
//!
//! 原 `acro_form.rs` 中被多个主题引用的夹具集中迁入 `support`（可见性放宽到
//! `pub(super)`），仅服务单一主题的夹具（`page_annots`、`form_build_error`、
//! `form_with_fields_error`、`calculate_field`）随主题留在对应子文件，本文件
//! 仅保留子模块声明。

mod alternate_name;
mod appearance;
mod calculation_order;
mod default_fonts;
mod field_actions;
mod field_write;
mod rejects;
mod support;
mod version_gate;
