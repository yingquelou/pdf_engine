//! 文档默认语言与结构元素 `/Lang` 写端测试（ISO 32000-1 14.9.2）。
//!
//! 覆盖目录 `/Lang` 的写出/回读与 PDF 1.4 门禁、结构元素语言的 RFC 3066
//! 校验与空串写法；所有失败均要求零字节输出。

use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::core::types::{Document, PdfFeature, PdfVersion};

fn language(value: &str) -> LanguageIdentifier {
    LanguageIdentifier::parse(value).unwrap_or_else(|error| panic!("{value:?}: {error}"))
}

/// 写出并重新解析；返回原始字节供文本串形态断言。
fn build_and_parse(b: &mut PdfBuilder) -> (Vec<u8>, Document) {
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
        "pdf-engine-accessibility-{}-{}.pdf",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&path, &out).expect("write temp pdf");
    let file = std::fs::File::open(&path).expect("open temp pdf");
    let mut parser = crate::core::reader::Parser::new(file).expect("parser");
    let doc = parser.parse_document().expect("parse");
    let _ = std::fs::remove_file(&path);
    (out, doc)
}

fn tree_with_language(value: &str) -> StructureTree {
    StructureTree::new().with_element(StructureElement::new("Document").with_lang(value))
}

#[test]
fn document_language_writes_catalog_entry() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    b.page(PageSize::A4);
    b.set_document_language(language("en-US"));

    let (out, doc) = build_and_parse(&mut b);
    assert!(
        String::from_utf8_lossy(&out).contains("/Lang <656E2D5553>"),
        "/Lang 应以文本串写出"
    );
    assert_eq!(
        doc.document_language().expect("catalog /Lang"),
        Some(language("en-US"))
    );
    assert!(
        doc.features.borrow().contains(PdfFeature::DocumentLanguage),
        "特性画像应登记文档语言"
    );

    // 空串目录 `/Lang` 同样可写，回读为未知语言。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_4);
    b.page(PageSize::A4);
    b.set_document_language(LanguageIdentifier::unknown());
    let (out, doc) = build_and_parse(&mut b);
    assert!(
        String::from_utf8_lossy(&out).contains("/Lang <>"),
        "空目录语言应写空文本串"
    );
    let declared = doc
        .document_language()
        .expect("empty catalog /Lang")
        .expect("declared");
    assert!(declared.is_unknown());
}

#[test]
fn document_language_requires_pdf_1_4() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_3);
    b.page(PageSize::A4);
    b.set_document_language(language("en"));
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("PDF 1.3 must reject /Lang");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(_)),
        "错误应为 UnsupportedVersion: {error}"
    );
    assert!(
        error.to_string().contains("document language"),
        "错误信息应说明目录 /Lang: {error}"
    );
    assert!(error.to_string().contains("1.4"), "{error}");
    assert!(out.is_empty(), "构建失败时不应写出任何字节");

    let mut b = PdfBuilder::with_version(PdfVersion::V1_4);
    b.page(PageSize::A4);
    b.set_document_language(language("en"));
    let mut out = Vec::new();
    b.build(&mut out).expect("PDF 1.4 supports /Lang");
    assert!(String::from_utf8_lossy(&out).contains("/Lang <656E>"));
}

#[test]
fn structure_language_identifier_is_validated() {
    for invalid in ["en_US", "en-abcdefghi"] {
        let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
        b.page(PageSize::A4);
        b.set_structure_tree(tree_with_language(invalid));
        let mut out = Vec::new();
        let error = b
            .build(&mut out)
            .expect_err("invalid /Lang must be rejected");
        assert!(
            matches!(&error, WriteError::InvalidObject(_)),
            "错误应为 InvalidObject: {error}"
        );
        assert!(
            error.to_string().contains("structure element 0"),
            "错误信息应包含元素下标: {error}"
        );
        assert!(
            error.to_string().contains(invalid),
            "错误信息应包含原串 {invalid:?}: {error}"
        );
        assert!(out.is_empty(), "构建失败时不应写出任何字节");
    }

    // 空串是合法的「未知语言」，写空文本串并可回读。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    b.page(PageSize::A4);
    b.set_structure_tree(tree_with_language(""));
    let (out, doc) = build_and_parse(&mut b);
    assert!(
        String::from_utf8_lossy(&out).contains("/Lang <>"),
        "空语言应写空文本串"
    );
    let tree = doc
        .structure_tree()
        .expect("structure tree")
        .expect("tree present");
    assert!(
        tree.effective_language(0)
            .expect("decode /Lang")
            .expect("/Lang present")
            .is_unknown()
    );
}
