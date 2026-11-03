//! 标准结构属性写端测试（ISO 32000-1 14.8.5.2、14.8.5.5–14.8.5.7）。
//!
//! 覆盖 List（Table 347）、PrintField（Table 348）、Table（Table 349）
//! 类型化构造器的写出 → 解析往返，元素适用性与 `/Headers` 引用的校验，
//! 以及 PDF 1.4/1.5/1.7 版本门禁；失败均要求零字节输出。
//!
//! 按主题目录化拆分（与 `tests/structure_mcr_stream/`、`tests/graphics_state/`、
//! `tests/structure/` 的目录化形态一致）：
//! - `format_specific`：格式专用所有者（Table 341）的 `/O` 与条目编码、
//!   直接属性与 `/ClassMap` 两条通道的特性画像登记；
//! - `roundtrip`：List/PrintField/Table（Table 347–349）类型化属性的写出 →
//!   解析回读取值与读端特性画像；
//! - `validation`：元素适用性、`/Headers` 的 `/ID` 引用与属性值形态在构建期
//!   拒绝且零字节输出；
//! - `version_gates`：格式专用属性 PDF 1.4、PrintField 属性 PDF 1.7、
//!   表头属性 PDF 1.5 与 `/Summary` PDF 1.7 的版本门禁。
//!
//! 模块 doc、helper 与夹具保留在本文件，子文件经 `use super::*;` 复用。

mod format_specific;
mod roundtrip;
mod validation;
mod version_gates;

use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::core::types::{Document, PdfFeature, PdfVersion};

/// 写出后重新解析：结构树只在 `build` 的导航后处理中物化。
fn build_and_parse(b: &mut PdfBuilder) -> Document {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let mut out = Vec::new();
    b.build(&mut out).expect("build");
    let base = std::env::current_exe()
        .ok()
        .and_then(|p| p.ancestors().nth(3).map(|t| t.to_path_buf()))
        .filter(|p| p.file_name() == Some(std::ffi::OsStr::new("target")))
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join("test-tmp");
    std::fs::create_dir_all(&dir).expect("create test-tmp");
    let path = dir.join(format!(
        "pdf-engine-structure-standard-{}-{}.pdf",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&path, &out).expect("write temp pdf");
    let file = std::fs::File::open(&path).expect("open temp pdf");
    let mut parser = crate::core::reader::Parser::new(file).expect("parser");
    let doc = parser.parse_document().expect("parse");
    let _ = std::fs::remove_file(&path);
    doc
}

fn assert_invalid(b: &mut PdfBuilder, expected: &str) {
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("build should fail");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(error.to_string().contains(expected), "{error}");
    assert!(out.is_empty(), "失败时不应写出任何字节");
}

fn assert_version_error(b: &mut PdfBuilder, expected: &str) {
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("build should fail");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(_)),
        "{error}"
    );
    assert!(error.to_string().contains(expected), "{error}");
    assert!(out.is_empty(), "失败时不应写出任何字节");
}

fn one_page(version: PdfVersion) -> PdfBuilder {
    let mut b = PdfBuilder::with_version(version);
    b.page(PageSize::A4);
    b
}

fn reader_features(doc: &Document) -> Vec<PdfFeature> {
    doc.features.borrow().iter().copied().collect()
}
