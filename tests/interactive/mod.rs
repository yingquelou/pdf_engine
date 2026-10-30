//! 集成测试：阶段 E/F 交互特性主题。
//!
//! 覆盖导航（E1）、注解（E2）、动作（E3）、AcroForm（E4）、元数据与文件
//! 标识（E5）、嵌入文件（E6）、增量更新（E7）与数字签名（F1）的
//! 写入 → 解析 → 回读往返；共享辅助见 `tests/support/mod.rs`。
//!
//! 按特性分文件：`navigation`、`articles`、`collection`、`templates`、`action`、`action_gotoe`、
//! `action_ocg`、`action_thread`、`annotation`、
//! `annotation_appearance`、`annotation_geometry`、`annotation_link`、
//! `annotation_measure`、`annotation_mk`、`annotation_popup`、
//! `annotation_redact`、`annotation_subtype_table`、`annotation_watermark`、
//! `alternate_presentation`、`form`、`embedded`、`metadata`、`page_presentation`、
//! `page_thumbnail`、`sound`、`incremental`、`signature`；根模块只保留主题文档、子模块声明与共享导入
//! （子模块经 `use super::*;` 取用）。

#[path = "../support/mod.rs"]
mod support;

mod action;
mod action_gotoe;
mod action_ocg;
mod action_thread;
mod alternate_presentation;
mod annotation;
mod annotation_appearance;
mod annotation_geometry;
mod annotation_link;
mod annotation_measure;
mod annotation_mk;
mod annotation_popup;
mod annotation_redact;
mod annotation_subtype_table;
mod annotation_watermark;
mod articles;
mod collection;
mod embedded;
mod form;
mod incremental;
mod metadata;
mod movie;
mod navigation;
mod outline_action;
mod page_presentation;
mod page_thumbnail;
mod signature;
mod sound;
mod templates;

pub use pdf_engine::core::types::{Array, Dict, Object, StreamData, Value};
pub use support::parse_buf;
