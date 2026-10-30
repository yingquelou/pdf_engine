//! 集合项 `/CI` 写端测试（7.11.6 Table 48/49，PDF 1.7）。
//!
//! Table 44 要求 `/CI` 为间接引用：文件规格、嵌入流之外再分配一个
//! 集合项对象；文本按 UTF-16BE 十六进制写出，子项字典直接内嵌。

use super::*;
use crate::core::types::PdfFeature;

fn object_dict(doc: &Document, num: i64) -> Dict {
    doc.get_object(num)
        .expect("object lookup")
        .expect("object exists")
        .value
        .as_dict()
        .expect("dict object")
        .clone()
}

/// 取 `/Names /EmbeddedFiles` 唯一条目的（文件规格对象号, 字典）。
fn single_filespec(doc: &Document) -> (i64, Dict) {
    let catalog = object_dict(doc, 1);
    let node = catalog
        .get("Names")
        .and_then(Value::as_dict)
        .and_then(|names| names.get("EmbeddedFiles"))
        .and_then(Value::as_dict)
        .expect("/EmbeddedFiles node");
    let array = node
        .get("Names")
        .and_then(Value::as_array)
        .expect("leaf /Names");
    let (num, _) = array.0[1]
        .as_ref()
        .expect("filespec should be an indirect reference");
    (num, object_dict(doc, num))
}

/// 文本串按 `text_string_value` 的编码（UTF-16BE 大写十六进制）构造期望值。
fn text_hex(text: &str) -> Value {
    let bytes = crate::core::types::PdfTextString::from(text).encode();
    Value::HexString(
        bytes
            .iter()
            .map(|byte| format!("{byte:02X}"))
            .collect::<String>(),
    )
}

fn sample_item() -> CollectionItem {
    CollectionItem::new()
        .with_text("Title", "Annual Report")
        .with_number("Copies", 2.0)
        .with_number("Score", 4.5)
        .with_subitem(
            "Issued",
            CollectionSubitem::new()
                .with_text("D:20261030120000Z")
                .with_prefix("Issued: "),
        )
        .with_subitem("Pages", CollectionSubitem::new().with_number(42.0))
}

/// 与 [`sample_item`] 键和类型一一对应的集合模式。
fn sample_schema() -> Vec<CollectionField> {
    vec![
        CollectionField::new("Title", CollectionFieldType::Text, "Title"),
        CollectionField::new("Copies", CollectionFieldType::Number, "Copies"),
        CollectionField::new("Score", CollectionFieldType::Number, "Score"),
        CollectionField::new("Issued", CollectionFieldType::Date, "Issued"),
        CollectionField::new("Pages", CollectionFieldType::Number, "Pages"),
    ]
}

#[test]
fn collection_item_is_written_as_indirect_object() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    b.page(PageSize::A4);
    b.collection(Collection::new().with_schema(sample_schema()));
    b.embed_file(
        EmbeddedFile::new("report.pdf", b"%PDF fake".to_vec()).with_collection_item(sample_item()),
    );
    let doc = b.assemble_document().expect("assemble");

    let (filespec_num, filespec) = single_filespec(&doc);
    let (ci_num, _) = filespec
        .get("CI")
        .and_then(Value::as_ref)
        .expect("/CI should be an indirect reference");
    // 文件规格、嵌入流、集合项依次分配。
    assert_eq!(ci_num, filespec_num + 2);
    let ci = object_dict(&doc, ci_num);
    assert_eq!(ci.get("Type"), Some(&Value::Name("CollectionItem".into())));
    assert_eq!(ci.get("Title"), Some(&text_hex("Annual Report")));
    assert_eq!(ci.get("Copies"), Some(&Value::Integer(2)));
    assert_eq!(ci.get("Score"), Some(&Value::Float(4.5)));

    let issued = ci
        .get("Issued")
        .and_then(Value::as_dict)
        .expect("Issued subitem");
    assert_eq!(
        issued.get("Type"),
        Some(&Value::Name("CollectionSubitem".into()))
    );
    assert_eq!(issued.get("D"), Some(&text_hex("D:20261030120000Z")));
    assert_eq!(issued.get("P"), Some(&text_hex("Issued: ")));
    let pages = ci
        .get("Pages")
        .and_then(Value::as_dict)
        .expect("Pages subitem");
    assert_eq!(pages.get("D"), Some(&Value::Integer(42)));
    assert!(pages.get("P").is_none(), "未声明的 /P 不物化");

    let features: Vec<PdfFeature> = doc.features.borrow().iter().copied().collect();
    assert!(features.contains(&PdfFeature::CollectionItem));
}

#[test]
fn invalid_collection_items_are_rejected() {
    let cases: Vec<(EmbeddedFile, &str)> = vec![
        (
            EmbeddedFile::new("a.txt", b"x".to_vec())
                .with_collection_item(CollectionItem::new().with_text("bad key", "value")),
            "contains name delimiters",
        ),
        (
            EmbeddedFile::new("a.txt", b"x".to_vec())
                .with_collection_item(CollectionItem::new().with_text("", "value")),
            "must not be empty",
        ),
        (
            EmbeddedFile::new("a.txt", b"x".to_vec())
                .with_collection_item(CollectionItem::new().with_number("size", f64::NAN)),
            "number must be finite",
        ),
        (
            EmbeddedFile::new("a.txt", b"x".to_vec()).with_collection_item(
                CollectionItem::new()
                    .with_subitem("date", CollectionSubitem::new().with_number(f64::INFINITY)),
            ),
            "/D number must be finite",
        ),
    ];
    for (file, expected) in cases {
        let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
        b.page(PageSize::A4);
        b.embed_file(file);
        let mut out = Vec::new();
        let error = b.build(&mut out).expect_err("invalid /CI must be rejected");
        assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
        assert!(error.to_string().contains(expected), "{error}");
        assert!(out.is_empty(), "构建失败时不应写出任何字节");
    }
}

#[test]
fn collection_item_requires_pdf_1_7() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_6);
    b.page(PageSize::A4);
    b.embed_file(
        EmbeddedFile::new("a.txt", b"x".to_vec())
            .with_collection_item(CollectionItem::new().with_text("Title", "x")),
    );
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("PDF 1.6 must reject /CI");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(_)),
        "错误应为 UnsupportedVersion: {error}"
    );
    assert!(
        error.to_string().contains("collection items (/CI)"),
        "{error}"
    );
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

/// `/CI` ↔ `/Schema` 交叉校验（12.3.5 Table 155/157、7.11.6 Table 48/49）：
/// 键必须在模式中声明，值形态须与 `/Subtype` 一致；无集合时 `/CI` 无模式可依。
#[test]
fn collection_item_values_must_match_the_schema() {
    let cases: Vec<(Option<Collection>, EmbeddedFile, &str)> = vec![
        (
            None,
            EmbeddedFile::new("a.txt", b"x".to_vec())
                .with_collection_item(CollectionItem::new().with_text("Title", "x")),
            "has a collection item but no collection is set",
        ),
        (
            Some(Collection::new()),
            EmbeddedFile::new("a.txt", b"x".to_vec())
                .with_collection_item(CollectionItem::new().with_text("Title", "x")),
            "collection items require a collection schema",
        ),
        (
            Some(Collection::new().with_schema(sample_schema())),
            EmbeddedFile::new("a.txt", b"x".to_vec())
                .with_collection_item(CollectionItem::new().with_text("Missing", "x")),
            "is not declared in the collection schema",
        ),
        (
            Some(Collection::new().with_schema(sample_schema())),
            EmbeddedFile::new("a.txt", b"x".to_vec())
                .with_collection_item(CollectionItem::new().with_number("Title", 1.0)),
            "expects a text string, got a number",
        ),
        (
            Some(Collection::new().with_schema(sample_schema())),
            EmbeddedFile::new("a.txt", b"x".to_vec())
                .with_collection_item(CollectionItem::new().with_text("Copies", "two")),
            "expects a number, got a text string",
        ),
        (
            Some(Collection::new().with_schema(sample_schema())),
            EmbeddedFile::new("a.txt", b"x".to_vec()).with_collection_item(
                CollectionItem::new()
                    .with_subitem("Copies", CollectionSubitem::new().with_text("two")),
            ),
            "expects a number, got a text string",
        ),
    ];
    for (collection, file, expected) in cases {
        let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
        b.page(PageSize::A4);
        if let Some(collection) = collection {
            b.collection(collection);
        }
        b.embed_file(file);
        let mut out = Vec::new();
        let error = b.build(&mut out).expect_err("集合交叉校验失败应拒绝构建");
        assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
        assert!(error.to_string().contains(expected), "{error}");
        assert!(out.is_empty(), "构建失败时不应写出任何字节");
    }
}

/// Table 155 `/D`：初始文档须标识 EmbeddedFiles 名称树中的一个条目。
#[test]
fn collection_initial_document_must_name_an_embedded_file() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    b.page(PageSize::A4);
    b.collection(Collection::new().with_initial_document("report.pdf"));
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("/D 必须指向嵌入文件");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(
        error.to_string().contains("is not an embedded file name"),
        "{error}"
    );
    assert!(out.is_empty(), "构建失败时不应写出任何字节");

    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    b.page(PageSize::A4);
    b.collection(Collection::new().with_initial_document("report.pdf"));
    b.embed_file(EmbeddedFile::new("report.pdf", b"x".to_vec()));
    let mut out = Vec::new();
    b.build(&mut out).expect("/D 指向已嵌入的文件应通过");
    assert!(!out.is_empty());
}
