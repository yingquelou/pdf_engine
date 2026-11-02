//! 逻辑结构写端测试的跨主题共享夹具：构建后重解析、非法/版本错误断言、
//! 单页构建器与属性对象构造辅助。仅服务单一主题的夹具（`object_dict`、
//! `simple_tree`、`user_property_attribute`）随主题留在对应子文件。

use std::sync::atomic::{AtomicUsize, Ordering};

use super::super::*;
use crate::core::types::PdfVersion;

/// 写出后重新解析：结构树与父树只在 `build` 的导航后处理中物化。
pub(super) fn build_and_parse(b: &mut PdfBuilder) -> Document {
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
        "pdf-engine-structure-{}-{}.pdf",
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

pub(super) fn assert_invalid(b: &mut PdfBuilder, expected: &str) {
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("build should fail");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(error.to_string().contains(expected), "{error}");
    assert!(out.is_empty(), "失败时不应写出任何字节");
}

pub(super) fn assert_version_error(b: &mut PdfBuilder, expected: &str) {
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("build should fail");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(_)),
        "{error}"
    );
    assert!(error.to_string().contains(expected), "{error}");
    assert!(out.is_empty(), "失败时不应写出任何字节");
}

pub(super) fn one_page(version: PdfVersion) -> PdfBuilder {
    let mut b = PdfBuilder::with_version(version);
    b.page(PageSize::A4);
    b
}

pub(super) fn name(value: &str) -> Value {
    Value::Name(value.to_owned())
}

/// 最小 Layout 属性对象。
pub(super) fn layout_dict(space_before: i64) -> Dict {
    Dict::new()
        .insert("O", name("Layout"))
        .insert("SpaceBefore", Value::Integer(space_before))
}

/// 单属性元素，`/A` 为给定属性对象。
pub(super) fn attribute_tree(value: Value) -> StructureTree {
    StructureTree::new()
        .with_element(StructureElement::new("P").with_attribute(StructureAttribute::new(value)))
}
