//! 渐变（Shading Type 1–7）与着色图案（PatternType 2）写端测试。
//!
//! 按主题目录化拆分（与 `tests/cid/`、`tests/structure_objr/` 的目录化形态一致）：
//! - `support`：跨主题共享夹具（指数函数样本、轴向渐变、着色图案与渲染辅助）；
//! - `direct_write`：Type 1–3 直接渐变与 PatternType 2 图案的写出、资源复用与未使用不写出；
//! - `mesh_write`：Type 4–7 网格/网格片渐变的流字典写出与图案间接引用形态；
//! - `function_sample`：`/Function` 样本函数与函数数组的写出、位宽规范枚举取值及其拒绝路径；
//! - `rejects`：非法渐变/图案/网格/网格片/公共条目参数在构建期拒绝且零字节输出；
//! - `common_options`：`/Background` `/BBox` `/AntiAlias` 公共条目的写出与省略；
//! - `version_gate`：PDF 1.3 版本门禁。
//!
//! 原 `shading.rs` 中被多个主题引用的夹具集中迁入 `support`（可见性放宽到
//! `pub(super)`），仅服务单一主题的夹具随主题留在对应子文件，本文件仅保留
//! 子模块声明。

mod common_options;
mod direct_write;
mod function_sample;
mod mesh_write;
mod rejects;
mod support;
mod version_gate;
