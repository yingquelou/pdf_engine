//! 命名模板页写端测试（ISO 32000-1 12.7.6、Table 31）：`/Names /Templates`
//! 名称树、模板页对象（`/Type /Template`、无 `/Parent` `/B`）、页面树不变、
//! 与其他名称树类别共存、非法输入与 PDF 1.3 门禁。

use std::sync::atomic::{AtomicU64, Ordering};

use super::*;
use crate::core::reader::navigation::Destination;
use crate::core::types::{PdfFeature, PdfTextString, PdfVersion, Value};

fn hex_of(text: &str) -> String {
    PdfTextString::from(text)
        .encode()
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect()
}

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

/// 导航后处理只在 `build` 中执行；写出后重新解析以检查对象图。
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
        "pdf-engine-templates-{}-{}.pdf",
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

fn features(doc: &Document) -> Vec<PdfFeature> {
    doc.features.borrow().iter().copied().collect()
}

fn names_dict(doc: &Document) -> Dict {
    catalog(doc)
        .get("Names")
        .and_then(Value::as_dict)
        .cloned()
        .expect("catalog /Names")
}

#[test]
fn templates_write_invisible_page_objects_outside_page_tree() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_3);
    b.page(PageSize::A4);
    let handle = b.add_template("Zeta", PageSize::Custom(144.0, 72.0));
    assert_eq!(handle, 0, "add_template 返回注册顺序下标");
    b.add_template("Cover", PageSize::A4);

    let doc = build_and_parse(&mut b);

    let names = names_dict(&doc);
    let tree = names
        .get("Templates")
        .and_then(Value::as_dict)
        .expect("/Names /Templates");
    assert!(tree.get("Kids").is_none(), "模板名称树应为单层叶子节点");
    let entries = tree
        .get("Names")
        .and_then(Value::as_array)
        .expect("/Templates /Names 数组");
    assert_eq!(entries.len(), 4, "两个模板各占键值一对");
    // 键按 PDF 文本串编码升序：Cover 排在 Zeta 之前。
    assert_eq!(entries.0[0], Value::HexString(hex_of("Cover")));
    assert_eq!(entries.0[2], Value::HexString(hex_of("Zeta")));

    let (first_num, first_generation) = entries.0[1].as_ref().expect("模板页间接引用");
    assert_eq!(first_generation, 0);
    let cover = object_dict(&doc, first_num);
    assert_eq!(cover.get("Type"), Some(&Value::Name("Template".into())));
    assert_eq!(cover.get("Parent"), None, "模板页不得写 /Parent");
    assert_eq!(cover.get("B"), None, "模板页不得写 /B");
    assert_eq!(cover.get("Contents"), None, "本切片不生成模板页内容流");
    let media_box = cover
        .get("MediaBox")
        .and_then(Value::as_array)
        .expect("/MediaBox");
    assert_eq!(
        media_box.0,
        vec![
            Value::Float(0.0),
            Value::Float(0.0),
            Value::Float(595.0),
            Value::Float(842.0),
        ]
    );

    // 页面树不受影响：Kids 只有真实页面，Count 不变。
    let (pages_num, _) = catalog(&doc)
        .get("Pages")
        .and_then(Value::as_ref)
        .expect("catalog /Pages");
    let pages = object_dict(&doc, pages_num);
    assert_eq!(pages.get("Count"), Some(&Value::Integer(1)));
    assert_eq!(
        pages
            .get("Kids")
            .and_then(Value::as_array)
            .map(|kids| kids.len()),
        Some(1)
    );

    // 读端 API 能逐项回读名称与 MediaBox。
    let templates = doc.page_templates().expect("read templates");
    assert_eq!(templates.len(), 2);
    assert_eq!(
        templates["Cover"]
            .media_box
            .map(|rect| (rect.llx(), rect.lly(), rect.urx(), rect.ury())),
        Some((0.0, 0.0, 595.0, 842.0))
    );
    assert_eq!(
        templates["Zeta"]
            .media_box
            .map(|rect| (rect.llx(), rect.lly(), rect.urx(), rect.ury())),
        Some((0.0, 0.0, 144.0, 72.0))
    );
}

#[test]
fn templates_coexist_with_destinations_and_embedded_files() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_3);
    b.page(PageSize::A4);
    b.embed_file(EmbeddedFile::new("report.pdf", b"report".to_vec()));
    b.named_destination("intro", Destination::Fit { page: 0 });
    b.named_page("chapter", 0);
    b.add_template("Cover", PageSize::Letter);

    let doc = build_and_parse(&mut b);

    let names = names_dict(&doc);
    for category in ["EmbeddedFiles", "Dests", "Pages", "Templates"] {
        assert!(
            names.get(category).is_some(),
            "名称字典应保留 /{category} 类别"
        );
    }
    assert_eq!(doc.page_count().unwrap(), 1, "模板页不进入页面树");
    assert_eq!(doc.page_templates().unwrap().len(), 1);
    assert_eq!(
        names
            .get("Pages")
            .and_then(Value::as_dict)
            .and_then(|tree| tree.get("Names"))
            .and_then(Value::as_array)
            .map(|pairs| pairs.len()),
        Some(2),
        "命名页面名称树应仍有一条键值对"
    );
}

#[test]
fn invalid_templates_are_rejected_without_output() {
    fn reject(name: &str, size: PageSize, label: &str) {
        let mut b = PdfBuilder::with_version(PdfVersion::V1_3);
        b.page(PageSize::A4);
        b.add_template(name, size);
        let mut out = Vec::new();
        let error = b.build(&mut out).expect_err(label);
        assert!(
            matches!(error, WriteError::InvalidObject(_)),
            "{label}: unexpected error {error:?}"
        );
        assert!(out.is_empty(), "{label}: 非法输入必须零字节");
    }

    reject("", PageSize::A4, "空名称");
    reject("bad\0name", PageSize::A4, "名称含 NUL");
    reject("Zero", PageSize::Custom(0.0, 100.0), "零宽 MediaBox");

    let mut b = PdfBuilder::with_version(PdfVersion::V1_3);
    b.page(PageSize::A4);
    b.add_template("dup", PageSize::A4);
    b.add_template("dup", PageSize::Letter);
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("重名");
    assert!(
        matches!(error, WriteError::InvalidObject(_)),
        "unexpected error {error:?}"
    );
    assert!(out.is_empty(), "重名必须零字节");
}

#[test]
fn templates_require_pdf_1_3() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_2);
    b.page(PageSize::A4);
    b.add_template("Cover", PageSize::A4);
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("PDF 1.2 must reject page templates");
    assert!(
        matches!(error, WriteError::UnsupportedVersion(_)),
        "unexpected error {error:?}"
    );
    assert!(out.is_empty(), "版本门禁失败必须零字节");

    let mut b = PdfBuilder::with_version(PdfVersion::V1_3);
    b.page(PageSize::A4);
    b.add_template("Cover", PageSize::A4);
    let mut out = Vec::new();
    b.build(&mut out).expect("PDF 1.3 supports page templates");
    assert!(!out.is_empty());
}

#[test]
fn template_feature_is_registered() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_3);
    b.page(PageSize::A4);
    b.add_template("Cover", PageSize::A4);
    let doc = build_and_parse(&mut b);
    // 读端按需解析对象；强制全量加载后特性画像才完整。
    doc.load_all_objects().expect("load all objects");
    assert!(
        features(&doc).contains(&PdfFeature::PageTemplates),
        "写出模板页应登记 PageTemplates 特性"
    );

    let mut plain = PdfBuilder::with_version(PdfVersion::V1_3);
    plain.page(PageSize::A4);
    let doc = build_and_parse(&mut plain);
    doc.load_all_objects().expect("load all objects");
    assert!(!features(&doc).contains(&PdfFeature::PageTemplates));
}
