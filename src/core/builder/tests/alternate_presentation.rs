//! 备用演示写端测试（ISO 32000-1 13.5、Table 297）：幻灯片对象
//! （`/Type /SlideShow`、`/Subtype /Embedded`、直接 `/Resources` 名称树、
//! `/StartResource`）、目录 `/Names /AlternatePresentations` 名称树与其他
//! 名称树类别共存、按键排序、嵌入文件引用、非法输入与 PDF 1.4 门禁。

use std::sync::atomic::{AtomicU64, Ordering};

use super::*;
use crate::core::builder::Slideshow;
use crate::core::reader::navigation::Destination;
use crate::core::types::{PdfFeature, PdfTextString, PdfVersion, Value};

fn hex_of(text: &str) -> String {
    PdfTextString::from(text)
        .encode()
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect()
}

fn hex_bytes(hex: &str) -> Vec<u8> {
    assert!(
        hex.len().is_multiple_of(2),
        "hex string should have an even length: {hex}"
    );
    (0..hex.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).expect("hex digit"))
        .collect()
}

/// 名称树叶子节点的解码条目：键按 PDF 文本串还原。
fn entry_pairs(node: &Dict) -> Vec<(String, Value)> {
    let array = node
        .get("Names")
        .and_then(Value::as_array)
        .expect("leaf /Names");
    assert!(
        array.len().is_multiple_of(2),
        "name tree entries must pair up"
    );
    array
        .0
        .chunks(2)
        .map(|pair| {
            let bytes: Vec<u8> = match &pair[0] {
                Value::String(text) => text.chars().map(|ch| ch as u32 as u8).collect(),
                Value::HexString(hex) => hex_bytes(hex),
                other => panic!("name tree key is not a string: {other:?}"),
            };
            (
                PdfTextString::decode(&bytes)
                    .expect("decode name tree key")
                    .into_string(),
                pair[1].clone(),
            )
        })
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
        "pdf-engine-alternate-presentation-{}-{}.pdf",
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

/// 幻灯片名称树与 `/EmbeddedFiles`、`/Dests`、`/Pages`、`/Templates` 共存。
#[test]
fn alternate_presentations_coexist_with_other_names_categories() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    b.page(PageSize::A4);
    b.embed_file(EmbeddedFile::new("slides.svg", b"<svg/>".to_vec()));
    b.named_destination("intro", Destination::Fit { page: 0 });
    b.named_page("chapter", 0);
    b.add_template("Cover", PageSize::A4);
    b.add_alternate_presentation(
        Slideshow::new("MySlideShow", "slides.svg").with_resource("slides.svg", "slides.svg"),
    );

    let doc = build_and_parse(&mut b);

    let names = names_dict(&doc);
    for category in [
        "EmbeddedFiles",
        "Dests",
        "Pages",
        "Templates",
        "AlternatePresentations",
    ] {
        assert!(
            names.get(category).is_some(),
            "名称字典应保留 /{category} 类别"
        );
    }

    let embedded = names
        .get("EmbeddedFiles")
        .and_then(Value::as_dict)
        .expect("/EmbeddedFiles node");
    let embedded_entries = entry_pairs(embedded);
    assert_eq!(embedded_entries.len(), 1, "单个嵌入文件一条键值对");
    assert_eq!(embedded_entries[0].0, "slides.svg");

    // 读端 API 回读幻灯片与已解引用的文件规格。
    let shows = doc.alternate_presentations().expect("read shows");
    assert_eq!(shows.len(), 1);
    let show = &shows["MySlideShow"];
    assert_eq!(show.start_resource, "slides.svg");
    assert_eq!(show.resources.len(), 1);
    assert_eq!(
        show.dict.get("Type").and_then(Value::as_name),
        Some("SlideShow")
    );
    assert_eq!(
        show.dict.get("Subtype").and_then(Value::as_name),
        Some("Embedded")
    );
    let filespec = show.resources["slides.svg"]
        .as_dict()
        .expect("资源值解析为文件规格字典");
    assert_eq!(filespec.get("Type"), Some(&Value::Name("Filespec".into())));
    assert!(
        filespec.get("EF").and_then(Value::as_dict).is_some(),
        "文件规格应带 /EF"
    );

    let (filespec_num, _) = embedded_entries[0]
        .1
        .as_ref()
        .expect("EmbeddedFiles 值应为引用");
    let embedded_spec = object_dict(&doc, filespec_num);
    assert_eq!(
        &embedded_spec, filespec,
        "资源值应与 /Names /EmbeddedFiles 指向同一文件规格"
    );
}

/// 幻灯片与资源名称树按键排序写出，资源值复用嵌入文件规格引用。
#[test]
fn alternate_presentations_write_sorted_name_trees() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_4);
    b.page(PageSize::A4);
    b.embed_file(EmbeddedFile::new("a.svg", b"a".to_vec()));
    b.embed_file(EmbeddedFile::new("b.svg", b"b".to_vec()));
    b.add_alternate_presentation(
        Slideshow::new("Zeta", "b.svg")
            .with_resource("b.svg", "b.svg")
            .with_resource("a.svg", "a.svg"),
    );
    b.add_alternate_presentation(Slideshow::new("Alpha", "a.svg").with_resource("a.svg", "a.svg"));

    let doc = build_and_parse(&mut b);

    let names = names_dict(&doc);
    let tree = names
        .get("AlternatePresentations")
        .and_then(Value::as_dict)
        .expect("/Names /AlternatePresentations");
    assert!(tree.get("Kids").is_none(), "幻灯片名称树应为单层叶子节点");
    let shows = entry_pairs(tree);
    let show_names: Vec<&str> = shows.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(show_names, ["Alpha", "Zeta"], "幻灯片键应按编码升序写出");

    let (zeta_num, _) = shows[1].1.as_ref().expect("幻灯片应为间接引用");
    let zeta = object_dict(&doc, zeta_num);
    assert_eq!(zeta.get("Type"), Some(&Value::Name("SlideShow".into())));
    assert_eq!(zeta.get("Subtype"), Some(&Value::Name("Embedded".into())));
    assert_eq!(
        zeta.get("StartResource"),
        Some(&Value::HexString(hex_of("b.svg"))),
        "/StartResource 写成 PDF 文本串"
    );
    let resources = zeta
        .get("Resources")
        .and_then(Value::as_dict)
        .expect("/Resources 应为直接名称树字典");
    assert!(resources.get("Kids").is_none(), "资源名称树应为叶子节点");
    let resource_entries = entry_pairs(resources);
    let resource_keys: Vec<&str> = resource_entries
        .iter()
        .map(|(name, _)| name.as_str())
        .collect();
    assert_eq!(resource_keys, ["a.svg", "b.svg"], "资源键应按编码升序");
    for (_, value) in &resource_entries {
        assert!(value.as_ref().is_some(), "资源值应引用文件规格: {value:?}");
    }

    let (alpha_num, _) = shows[0].1.as_ref().expect("幻灯片应为间接引用");
    let alpha = object_dict(&doc, alpha_num);
    assert_eq!(
        alpha.get("StartResource"),
        Some(&Value::HexString(hex_of("a.svg")))
    );
    let alpha_entries = entry_pairs(
        alpha
            .get("Resources")
            .and_then(Value::as_dict)
            .expect("Alpha /Resources"),
    );
    assert_eq!(alpha_entries.len(), 1);
    let zeta_a = resource_entries
        .iter()
        .find(|(name, _)| name == "a.svg")
        .expect("Zeta 声明 a.svg");
    assert_eq!(
        alpha_entries[0].1, zeta_a.1,
        "同名嵌入文件应复用同一文件规格引用"
    );
}

/// PDF 1.3 拒绝备用演示并零字节输出；PDF 1.4 接受。
#[test]
fn alternate_presentations_require_pdf_1_4() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_3);
    b.page(PageSize::A4);
    b.embed_file(EmbeddedFile::new("slides.svg", b"<svg/>".to_vec()));
    b.add_alternate_presentation(
        Slideshow::new("Show", "slides.svg").with_resource("slides.svg", "slides.svg"),
    );
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("PDF 1.3 must reject alternate presentations");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(_)),
        "unexpected error {error:?}"
    );
    assert!(
        error.to_string().contains("alternate presentations"),
        "{error}"
    );
    assert!(out.is_empty(), "版本门禁失败必须零字节");

    let mut b = PdfBuilder::with_version(PdfVersion::V1_4);
    b.page(PageSize::A4);
    b.embed_file(EmbeddedFile::new("slides.svg", b"<svg/>".to_vec()));
    b.add_alternate_presentation(
        Slideshow::new("Show", "slides.svg").with_resource("slides.svg", "slides.svg"),
    );
    let s = render(&mut b);
    assert!(s.contains("/Type /SlideShow"), "{s}");
    assert!(s.contains("/Subtype /Embedded"), "{s}");
    assert!(s.contains("/AlternatePresentations"), "{s}");
}

/// 非法幻灯片配置返回 `InvalidObject` 且零字节输出。
#[test]
fn invalid_alternate_presentations_are_rejected_without_output() {
    fn reject(show: Slideshow, label: &str, expected: &str) {
        let mut b = PdfBuilder::with_version(PdfVersion::V1_4);
        b.page(PageSize::A4);
        b.embed_file(EmbeddedFile::new("slides.svg", b"<svg/>".to_vec()));
        b.add_alternate_presentation(show);
        let mut out = Vec::new();
        let error = b.build(&mut out).expect_err(label);
        assert!(
            matches!(&error, WriteError::InvalidObject(_)),
            "{label}: unexpected error {error:?}"
        );
        assert!(error.to_string().contains(expected), "{label}: {error}");
        assert!(out.is_empty(), "{label}: 非法输入必须零字节");
    }

    reject(
        Slideshow::new("", "slides.svg").with_resource("slides.svg", "slides.svg"),
        "空幻灯片名",
        "name is empty",
    );
    reject(
        Slideshow::new("Show", "").with_resource("slides.svg", "slides.svg"),
        "空起始资源",
        "start resource is empty",
    );
    reject(
        Slideshow::new("Show", "poster.svg").with_resource("slides.svg", "slides.svg"),
        "未声明的起始资源",
        "is not a declared resource",
    );
    reject(
        Slideshow::new("Show", "poster.svg").with_resource("poster.svg", "missing.svg"),
        "未知嵌入文件",
        "unknown embedded file",
    );
    reject(
        Slideshow::new("Show", "slides.svg")
            .with_resource("slides.svg", "slides.svg")
            .with_resource("slides.svg", "slides.svg"),
        "重复资源键",
        "declares resource",
    );
    reject(
        Slideshow::new("Show", "slides.svg").with_resource("", "slides.svg"),
        "空资源键",
        "resource name is empty",
    );

    let mut b = PdfBuilder::with_version(PdfVersion::V1_4);
    b.page(PageSize::A4);
    b.embed_file(EmbeddedFile::new("slides.svg", b"<svg/>".to_vec()));
    b.add_alternate_presentation(
        Slideshow::new("Show", "slides.svg").with_resource("slides.svg", "slides.svg"),
    );
    b.add_alternate_presentation(
        Slideshow::new("Show", "slides.svg").with_resource("slides.svg", "slides.svg"),
    );
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("重复幻灯片名");
    assert!(
        matches!(&error, WriteError::InvalidObject(_)),
        "unexpected error {error:?}"
    );
    assert!(
        error
            .to_string()
            .contains("duplicate alternate presentation name"),
        "{error}"
    );
    assert!(out.is_empty(), "重复幻灯片名必须零字节");
}

/// 写出幻灯片后特性画像登记 `AlternatePresentations`。
#[test]
fn alternate_presentation_feature_is_registered() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_4);
    b.page(PageSize::A4);
    b.embed_file(EmbeddedFile::new("slides.svg", b"<svg/>".to_vec()));
    b.add_alternate_presentation(
        Slideshow::new("Show", "slides.svg").with_resource("slides.svg", "slides.svg"),
    );
    let doc = build_and_parse(&mut b);
    // 读端按需解析对象；强制全量加载后特性画像才完整。
    doc.load_all_objects().expect("load all objects");
    assert!(
        features(&doc).contains(&PdfFeature::AlternatePresentations),
        "写出幻灯片应登记 AlternatePresentations 特性"
    );

    let mut plain = PdfBuilder::with_version(PdfVersion::V1_4);
    plain.page(PageSize::A4);
    let doc = build_and_parse(&mut plain);
    doc.load_all_objects().expect("load all objects");
    assert!(!features(&doc).contains(&PdfFeature::AlternatePresentations));
}
