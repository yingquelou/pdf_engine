//! 便携集合写端测试（ISO 32000-1 12.3.5，Table 155–158）：模式字段、
//! `/D` `/View` `/Sort` 写出、缺省省略、非法输入与 PDF 1.7 门禁。

use super::*;
use crate::core::types::Array;
use crate::core::types::PdfFeature;
use crate::core::types::PdfTextString;

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

fn hex_of(text: &str) -> String {
    PdfTextString::from(text)
        .encode()
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect()
}

/// 导航后处理只在 `build` 中执行；写出后重新解析以检查对象图。
fn build_and_parse(b: &mut PdfBuilder) -> Document {
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
        "pdf-engine-collection-{}-{}.pdf",
        std::process::id(),
        out.len()
    ));
    std::fs::write(&path, &out).expect("write temp pdf");
    let file = std::fs::File::open(&path).expect("open temp pdf");
    let mut parser = crate::core::reader::Parser::new(file).expect("parser");
    let doc = parser.parse_document().expect("parse");
    let _ = std::fs::remove_file(&path);
    doc
}

fn features(doc: &Document) -> Vec<PdfFeature> {
    doc.features.borrow().iter().copied().collect()
}

#[test]
fn collection_writes_schema_view_and_sort() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    b.page(PageSize::A4);
    b.collection(
        Collection::new()
            .with_schema(vec![
                CollectionField::new("file", CollectionFieldType::FileName, "File")
                    .with_order(1)
                    .with_visible(false)
                    .with_editable(true),
                CollectionField::new("size", CollectionFieldType::Size, "Size"),
            ])
            .with_initial_document("report.pdf")
            .with_view(CollectionView::Tiles)
            .with_sort(CollectionSort::new(vec!["size".into()]).with_ascending(vec![false])),
    );

    let doc = build_and_parse(&mut b);

    let collection = catalog(&doc)
        .get("Collection")
        .and_then(Value::as_dict)
        .expect("catalog /Collection")
        .clone();
    assert!(features(&doc).contains(&PdfFeature::Collection));
    assert_eq!(
        collection.get("Type"),
        Some(&Value::Name("Collection".into()))
    );
    assert_eq!(
        collection.get("D"),
        Some(&Value::HexString(hex_of("report.pdf")))
    );
    assert_eq!(collection.get("View"), Some(&Value::Name("T".into())));

    let schema = collection
        .get("Schema")
        .and_then(Value::as_dict)
        .expect("collection /Schema");
    assert_eq!(
        schema.get("Type"),
        Some(&Value::Name("CollectionSchema".into()))
    );
    let file = schema
        .get("file")
        .and_then(Value::as_dict)
        .expect("file field");
    assert_eq!(
        file.get("Type"),
        Some(&Value::Name("CollectionField".into()))
    );
    assert_eq!(file.get("Subtype"), Some(&Value::Name("F".into())));
    assert_eq!(file.get("N"), Some(&Value::HexString(hex_of("File"))));
    assert_eq!(file.get("O"), Some(&Value::Integer(1)));
    assert_eq!(file.get("V"), Some(&Value::Bool(false)));
    assert_eq!(file.get("E"), Some(&Value::Bool(true)));

    let size = schema
        .get("size")
        .and_then(Value::as_dict)
        .expect("size field");
    assert_eq!(size.get("Subtype"), Some(&Value::Name("Size".into())));
    assert_eq!(size.get("O"), None, "未声明的 /O 不写出");
    assert_eq!(size.get("V"), None, "未声明的 /V 不写出");
    assert_eq!(size.get("E"), None, "未声明的 /E 不写出");

    let sort = collection
        .get("Sort")
        .and_then(Value::as_dict)
        .expect("collection /Sort");
    assert_eq!(
        sort.get("Type"),
        Some(&Value::Name("CollectionSort".into()))
    );
    assert_eq!(sort.get("S"), Some(&Value::Name("size".into())));
    assert_eq!(sort.get("A"), Some(&Value::Bool(false)));
}

#[test]
fn collection_is_optional_and_defaults_are_omitted() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    b.page(PageSize::A4);
    let doc = build_and_parse(&mut b);
    assert!(catalog(&doc).get("Collection").is_none());
    assert!(!features(&doc).contains(&PdfFeature::Collection));

    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    b.page(PageSize::A4);
    b.collection(Collection::new());
    let doc = build_and_parse(&mut b);
    let collection = catalog(&doc)
        .get("Collection")
        .and_then(Value::as_dict)
        .expect("catalog /Collection")
        .clone();
    assert_eq!(
        collection.get("Type"),
        Some(&Value::Name("Collection".into()))
    );
    assert_eq!(collection.get("Schema"), None);
    assert_eq!(collection.get("D"), None);
    assert_eq!(collection.get("View"), None, "缺省详情视图不写出");
    assert_eq!(collection.get("Sort"), None);

    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    b.page(PageSize::A4);
    b.collection(
        Collection::new()
            .with_schema(vec![
                CollectionField::new("a", CollectionFieldType::Text, "A"),
                CollectionField::new("b", CollectionFieldType::Number, "B"),
            ])
            .with_sort(CollectionSort::new(vec!["a".into(), "b".into()])),
    );
    let doc = build_and_parse(&mut b);
    let sort = catalog(&doc)
        .get("Collection")
        .and_then(Value::as_dict)
        .and_then(|collection| collection.get("Sort"))
        .and_then(Value::as_dict)
        .expect("collection /Sort")
        .clone();
    assert_eq!(
        sort.get("S"),
        Some(&Value::Array(Array(vec![
            Value::Name("a".into()),
            Value::Name("b".into()),
        ])))
    );
    assert_eq!(sort.get("A"), None, "全升序与缺省等价，不写出");
}

#[test]
fn invalid_collections_are_rejected() {
    fn reject(collection: Collection, label: &str) {
        let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
        b.page(PageSize::A4);
        b.collection(collection);
        let mut out = Vec::new();
        let error = b.build(&mut out).expect_err(label);
        assert!(
            matches!(error, WriteError::InvalidObject(_)),
            "{label}: unexpected error {error:?}"
        );
        assert!(out.is_empty(), "{label}: 非法输入必须零字节");
    }

    reject(
        Collection::new().with_schema(vec![CollectionField::new(
            "bad key",
            CollectionFieldType::Text,
            "Bad",
        )]),
        "非名字键",
    );
    reject(
        Collection::new().with_schema(vec![
            CollectionField::new("dup", CollectionFieldType::Text, "A"),
            CollectionField::new("dup", CollectionFieldType::Text, "B"),
        ]),
        "重复键",
    );
    reject(
        Collection::new().with_schema(vec![CollectionField::new(
            "empty",
            CollectionFieldType::Text,
            "",
        )]),
        "空显示名",
    );
    reject(
        Collection::new().with_sort(CollectionSort::new(vec!["missing".into()])),
        "排序无模式",
    );
    reject(
        Collection::new()
            .with_schema(vec![CollectionField::new(
                "a",
                CollectionFieldType::Text,
                "A",
            )])
            .with_sort(CollectionSort::new(Vec::new())),
        "排序字段为空",
    );
    reject(
        Collection::new()
            .with_schema(vec![CollectionField::new(
                "a",
                CollectionFieldType::Text,
                "A",
            )])
            .with_sort(CollectionSort::new(vec!["missing".into()])),
        "排序字段未声明",
    );
    reject(
        Collection::new()
            .with_schema(vec![CollectionField::new(
                "a",
                CollectionFieldType::Text,
                "A",
            )])
            .with_sort(CollectionSort::new(vec!["a".into()]).with_ascending(vec![true, false])),
        "升序长度不匹配",
    );
    reject(Collection::new().with_initial_document(""), "空初始文档名");
}

#[test]
fn collection_requires_pdf_1_7() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_6);
    b.page(PageSize::A4);
    b.collection(Collection::new());
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("PDF 1.6 must reject collections");
    assert!(
        matches!(error, WriteError::UnsupportedVersion(_)),
        "unexpected error {error:?}"
    );
    assert!(out.is_empty(), "版本门禁失败必须零字节");

    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    b.page(PageSize::A4);
    b.collection(Collection::new());
    let mut out = Vec::new();
    b.build(&mut out).expect("PDF 1.7 supports collections");
    assert!(!out.is_empty());
}
