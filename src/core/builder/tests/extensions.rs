//! 扩展字典写端测试（ISO 32000-1 7.12、Table 50）：目录 `/Extensions`
//! 直接对象嵌套写出、写端类型化读 API 往返、前缀与级别校验与 PDF 1.7
//! 门禁，全部失败路径零字节输出。

use std::sync::atomic::{AtomicU64, Ordering};

use super::*;
use crate::core::reader::navigation::ExtensionsDictionary;
use crate::core::types::{PdfFeature, PdfVersion, Value};

fn object_dict(doc: &Document, num: i64) -> Dict {
    doc.get_object(num)
        .expect("object")
        .expect("object exists")
        .value
        .as_dict()
        .expect("object dict")
        .clone()
}

fn catalog(doc: &Document) -> Dict {
    let (root, _generation) = doc
        .trailer
        .dict
        .get("Root")
        .and_then(Value::as_ref)
        .expect("/Root");
    object_dict(doc, root)
}

/// 写出后重新解析，走 reader 的懒加载对象路径。
fn build_and_parse(b: &mut PdfBuilder) -> Document {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
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
        "pdf-engine-extensions-{}-{}.pdf",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::write(&path, &out).expect("write temp pdf");
    let file = std::fs::File::open(&path).expect("open temp pdf");
    let mut parser = crate::core::reader::Parser::new(file).expect("parser");
    let doc = parser.parse_document().expect("parse");
    let _ = std::fs::remove_file(&path);
    doc
}

fn build_error(b: &mut PdfBuilder) -> (WriteError, Vec<u8>) {
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("build must fail");
    (error, out)
}

fn features(doc: &Document) -> Vec<PdfFeature> {
    doc.features.borrow().iter().copied().collect()
}

#[test]
fn extensions_write_direct_dicts_into_catalog() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    b.page(PageSize::A4);
    b.add_extension("ADBE", PdfVersion::V1_7, 3);
    b.add_extension("GLGR", PdfVersion::V1_7, 1002);
    let doc = build_and_parse(&mut b);

    // 7.12.1：`/Extensions` 与开发者字典必须直接嵌套在目录里。
    let catalog = catalog(&doc);
    let extensions = match catalog.get("Extensions") {
        Some(Value::Dict(dict)) => dict.clone(),
        other => panic!("catalog /Extensions must be a direct dictionary, got {other:?}"),
    };
    assert_eq!(extensions.0.len(), 2, "两个前缀字典");
    for prefix in ["ADBE", "GLGR"] {
        let value = extensions
            .get(prefix)
            .unwrap_or_else(|| panic!("missing /{prefix}"));
        let Value::Dict(inner) = value else {
            panic!("extensions /{prefix} must be a direct dictionary");
        };
        assert_eq!(
            inner.get("Type").and_then(Value::as_name),
            Some("DeveloperExtensions")
        );
        assert_eq!(
            inner.get("BaseVersion").and_then(Value::as_name),
            Some("1.7")
        );
        assert_eq!(
            inner.get("ExtensionLevel"),
            Some(&Value::Integer(if prefix == "ADBE" { 3 } else { 1002 }))
        );
    }

    let registered: Vec<PdfFeature> = features(&doc);
    assert!(
        registered.contains(&PdfFeature::ExtensionsDictionary),
        "特性画像应登记 ExtensionsDictionary"
    );
}

#[test]
fn extensions_roundtrip_through_read_api() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    b.page(PageSize::A4);
    b.add_extension("adbe", PdfVersion::V1_7, 7);
    let doc = build_and_parse(&mut b);

    let extensions: ExtensionsDictionary =
        doc.extensions().expect("read extensions").expect("present");
    assert_eq!(extensions.type_name, None, "写端省略可选 /Type Extensions");
    assert_eq!(extensions.prefixes.len(), 1);
    let entry = &extensions.prefixes["adbe"];
    assert_eq!(entry.base_version, "1.7");
    assert_eq!(entry.base_version_parsed(), Some(PdfVersion::V1_7));
    assert_eq!(entry.extension_level, 7);
    assert_eq!(entry.type_name.as_deref(), Some("DeveloperExtensions"));
}

#[test]
fn extensions_absent_yields_none_on_read() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    b.page(PageSize::A4);
    let doc = build_and_parse(&mut b);
    assert!(doc.extensions().expect("read").is_none());
    assert!(
        !features(&doc).contains(&PdfFeature::ExtensionsDictionary),
        "未写扩展字典不应登记特性"
    );
}

#[test]
fn extensions_gate_rejects_versions_below_1_7() {
    for version in [PdfVersion::V1_0, PdfVersion::V1_6] {
        let mut b = PdfBuilder::with_version(version);
        b.page(PageSize::A4);
        b.add_extension("ADBE", PdfVersion::V1_7, 3);
        let (error, out) = build_error(&mut b);
        assert!(
            matches!(error, WriteError::UnsupportedVersion(_)),
            "version {version:?}: {error}"
        );
        assert!(out.is_empty(), "版本门禁失败必须零字节");
    }
}

#[test]
fn extensions_reject_invalid_prefix_forms() {
    let cases: [(&str, &str); 4] = [
        ("", "empty"),
        ("ADB", "three bytes"),
        ("ABCDE", "five bytes"),
        ("AB-C", "delimiter byte"),
    ];
    for (prefix, label) in cases {
        let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
        b.page(PageSize::A4);
        b.add_extension(prefix, PdfVersion::V1_7, 1);
        let (error, out) = build_error(&mut b);
        assert!(
            matches!(error, WriteError::InvalidObject(_)),
            "{label} prefix {prefix:?}: {error}"
        );
        assert!(out.is_empty(), "非法前缀 {label} 必须零字节");
    }
}

#[test]
fn extensions_reject_duplicate_prefix() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    b.page(PageSize::A4);
    b.add_extension("ADBE", PdfVersion::V1_7, 3);
    b.add_extension("ADBE", PdfVersion::V1_7, 4);
    let (error, out) = build_error(&mut b);
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert!(out.is_empty(), "重复前缀必须零字节");
}

#[test]
fn extensions_reject_non_positive_extension_level() {
    for level in [0, -1] {
        let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
        b.page(PageSize::A4);
        b.add_extension("ADBE", PdfVersion::V1_7, level);
        let (error, out) = build_error(&mut b);
        assert!(
            matches!(error, WriteError::InvalidObject(_)),
            "level {level}: {error}"
        );
        assert!(out.is_empty(), "非正 ExtensionLevel 必须零字节");
    }
}

#[test]
fn extensions_reject_base_version_above_document_version() {
    // 7.12.3：BaseVersion 视作文版本号时须 ≤ 文档版本（头部与目录
    // /Version 的较大者；builder 目标版本即头部版本）。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    b.page(PageSize::A4);
    b.add_extension("ADBE", PdfVersion::V2_0, 1);
    let (error, out) = build_error(&mut b);
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert!(out.is_empty(), "BaseVersion 超版必须零字节");
}
