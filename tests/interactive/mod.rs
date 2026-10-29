//! 集成测试：阶段 E/F 交互特性主题。
//!
//! 覆盖导航（E1）、注解（E2）、动作（E3）、AcroForm（E4）、元数据与文件
//! 标识（E5）、嵌入文件（E6）、增量更新（E7）与数字签名（F1）的
//! 写入 → 解析 → 回读往返；共享辅助见 `tests/support/mod.rs`。
//!
//! 按特性分文件：`navigation`、`annotation`、`annotation_geometry`、
//! `annotation_link`、`form`、`embedded`、`metadata`、`incremental`、
//! `signature`；根模块只保留主题文档、子模块声明与共享导入
//! （子模块经 `use super::*;` 取用）。

#[path = "../support/mod.rs"]
mod support;

mod annotation;
mod annotation_geometry;
mod annotation_link;
mod embedded;
mod form;
mod incremental;
mod metadata;
mod navigation;
mod signature;

pub use pdf_engine::core::types::{Array, Dict, Object, StreamData, Value};
pub use support::parse_buf;
