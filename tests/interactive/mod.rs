//! 集成测试：阶段 E/F 交互特性主题。
//!
//! 覆盖导航（E1）、注解（E2）、动作（E3）、AcroForm（E4）、元数据与文件
//! 标识（E5）、嵌入文件（E6）、增量更新（E7）与数字签名（F1）的
//! 写入 → 解析 → 回读往返；共享辅助见 `tests/support/mod.rs`。
//!
//! 按特性分文件：`navigation`、`articles`、`collection`、`templates`、`action`、`action_goto3d`、
//! `action_gotoe`、`action_ocg`、`action_thread`、`annotation`、
//! `annotation_appearance`、`annotation_geometry`、`annotation_link`、
//! `annotation_measure`、`annotation_mk`、`annotation_popup`、
//! `annotation_printer_mark`、`annotation_redact`、`annotation_subtype_table`、
//! `annotation_trap_net`、
//! `annotation_watermark`、
//! `alternate_presentation`、`form`、`embedded`、`metadata`、`page_box`、`page_presentation`、
//! `page_thumbnail`、`opi`、`rendition`、`sound`、`structure`、`structure_standard`、
//! `three_d`、`incremental`、`signature`；根模块只保留主题文档、子模块声明与共享导入
//! （子模块经 `use super::*;` 取用）。

#[path = "../support/mod.rs"]
mod support;

mod action;
mod action_goto3d;
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
mod annotation_printer_mark;
mod annotation_redact;
mod annotation_subtype_table;
mod annotation_trap_net;
mod annotation_watermark;
mod articles;
mod collection;
mod embedded;
mod form;
mod incremental;
mod metadata;
mod movie;
mod navigation;
mod opi;
mod outline_action;
mod output_intent;
mod page_box;
mod page_presentation;
mod page_thumbnail;
mod rendition;
mod separation;
mod signature;
mod sound;
mod structure;
mod structure_standard;
mod templates;
mod three_d;

pub use pdf_engine::core::types::{Array, Dict, Object, StreamData, Value};
pub use support::parse_buf;
