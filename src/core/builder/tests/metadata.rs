//! 元数据写端测试：trailer `/Info` 文档信息字典与 `/ID` 文件标识符
//! （ISO 32000-1 14.3.3、14.4）。

use super::*;

/// `build` 后重新解析，供目录 `/Metadata` 断言使用。
fn build_and_parse(b: &mut PdfBuilder) -> Document {
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT: AtomicU64 = AtomicU64::new(0);
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
        "pdf-engine-metadata-{}-{}.pdf",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&path, &out).expect("write temp pdf");
    let file = std::fs::File::open(&path).expect("open temp pdf");
    let mut parser = crate::core::reader::Parser::new(file).expect("parser");
    let doc = parser.parse_document().expect("parse");
    let _ = std::fs::remove_file(&path);
    doc
}

#[test]
fn document_info_is_written_as_indirect_object() {
    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    b.set_document_info(
        DocumentInfo::new()
            .title("My Title")
            .author("Jane")
            .producer("pdf_engine"),
    );
    let doc = b.assemble_document().expect("assemble");
    let info_ref = doc
        .trailer
        .dict
        .get("Info")
        .and_then(|v| v.as_ref())
        .expect("trailer /Info should be a reference");
    let info_obj = doc.get_object(info_ref.0).expect("object").expect("exist");
    let dict = info_obj.value.as_dict().expect("dict");
    assert!(dict.get("Title").is_some());
    assert!(dict.get("Author").is_some());
    assert!(dict.get("Producer").is_some());
    assert!(dict.get("Subject").is_none());
}

#[test]
fn empty_document_info_keeps_trailer_unchanged() {
    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    b.set_document_info(DocumentInfo::new());
    let doc = b.assemble_document().expect("assemble");
    assert!(doc.trailer.dict.get("Info").is_none());
}

#[test]
fn file_identifier_is_written_to_trailer() {
    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    b.set_file_identifier(FileIdentifier::new([0xAB; 16], [0xCD; 16]));
    let doc = b.assemble_document().expect("assemble");
    let id = doc.trailer.dict.get("ID").and_then(|v| v.as_array());
    let array = id.expect("trailer /ID array");
    assert_eq!(array.len(), 2);
    match &array.0[0] {
        crate::core::types::Value::HexString(hex) => assert_eq!(hex, &"AB".repeat(16)),
        other => panic!("expected hex string, got {other:?}"),
    }
}

#[test]
fn default_builder_writes_no_id_or_info() {
    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    let doc = b.assemble_document().expect("assemble");
    assert!(doc.trailer.dict.get("Info").is_none());
    assert!(doc.trailer.dict.get("ID").is_none());
}

#[test]
fn generated_identifier_is_deterministic_across_builds() {
    let a = FileIdentifier::generate(b"doc-a".as_slice());
    let b = FileIdentifier::generate(b"doc-a".as_slice());
    assert_eq!(a.permanent, b.permanent);
    assert_ne!(
        a.permanent,
        FileIdentifier::generate(b"doc-b".as_slice()).permanent
    );
}

#[test]
fn xmp_metadata_stream_written_to_catalog() {
    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    let xml = b"<x:xmpmeta/>".to_vec();
    b.set_xmp_metadata(xml.clone());
    let doc = b.assemble_document().expect("assemble");
    let catalog = doc
        .get_object(1)
        .expect("catalog object")
        .expect("exist")
        .value;
    let catalog_dict = catalog.as_dict().expect("catalog dict");
    let meta_ref = catalog_dict
        .get("Metadata")
        .and_then(|v| v.as_ref())
        .expect("catalog /Metadata reference");
    let stream_obj = doc
        .get_object(meta_ref.0)
        .expect("object")
        .expect("exist")
        .value;
    let dict = match &stream_obj {
        crate::core::types::Value::Stream(dict, _data) => dict,
        other => panic!("expected stream, got {other:?}"),
    };
    use crate::core::types::Value;
    assert_eq!(dict.get("Type"), Some(&Value::Name("Metadata".into())));
    assert_eq!(dict.get("Subtype"), Some(&Value::Name("XML".into())));
    assert_eq!(dict.get("Length"), Some(&Value::Integer(xml.len() as i64)));
}

#[test]
fn info_trapped_requires_pdf_1_3() {
    use crate::core::reader::metadata::TrappedState;

    let mut b = PdfBuilder::with_version(PdfVersion::V1_2);
    b.page(PageSize::A4);
    b.set_document_info(DocumentInfo::new().trapped(TrappedState::Unknown));
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("should reject 1.2");
    assert!(
        matches!(error, WriteError::UnsupportedVersion(_)),
        "{error}"
    );
    assert_eq!(out.len(), 0);
}

#[test]
fn xmp_metadata_requires_pdf_1_4() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_3);
    b.page(PageSize::A4);
    b.set_xmp_metadata(b"<x:xmpmeta/>".to_vec());
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("should reject 1.3");
    assert!(
        matches!(error, WriteError::UnsupportedVersion(_)),
        "{error}"
    );
    assert_eq!(out.len(), 0);
}

#[test]
fn typed_xmp_packet_is_written_to_catalog() {
    use crate::core::builder::{XmpListItem, XmpListKind, XmpPacket, XmpValue};
    use crate::core::reader::xmp::{
        XmpListItem as ReaderListItem, XmpListKind as ReaderListKind, XmpValue as ReaderValue,
    };

    const DC: &str = "http://purl.org/dc/elements/1.1/";
    let mut packet = XmpPacket::new();
    packet
        .set(
            format!("{{{DC}}}title"),
            XmpValue::Text("Typed packet".into()),
        )
        .set(
            format!("{{{DC}}}creator"),
            XmpValue::List {
                kind: XmpListKind::Seq,
                items: vec![
                    XmpListItem {
                        lang: None,
                        value: "Ada".into(),
                    },
                    XmpListItem {
                        lang: None,
                        value: "Grace".into(),
                    },
                ],
            },
        );

    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    b.set_xmp_packet(packet);
    let doc = build_and_parse(&mut b);

    let xmp = doc.xmp().expect("parse XMP").expect("metadata stream");
    assert!(xmp.packet.contains("<x:xmpmeta"), "{}", xmp.packet);
    assert_eq!(xmp.text(&format!("{{{DC}}}title")), Some("Typed packet"));
    assert_eq!(
        xmp.properties.get(&format!("{{{DC}}}creator")),
        Some(&ReaderValue::List {
            kind: ReaderListKind::Seq,
            items: vec![
                ReaderListItem {
                    lang: None,
                    value: "Ada".into(),
                },
                ReaderListItem {
                    lang: None,
                    value: "Grace".into(),
                },
            ],
        })
    );
}

#[test]
fn invalid_xmp_packet_fails_without_output() {
    use crate::core::builder::{XmpPacket, XmpValue};

    let mut packet = XmpPacket::new();
    packet.set("{urn:example}text", XmpValue::Text("bad\u{0}".into()));
    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    b.set_xmp_packet(packet);

    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("invalid packet should fail");
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert_eq!(out.len(), 0);
}

#[test]
fn typed_xmp_packet_requires_pdf_1_4() {
    use crate::core::builder::{XmpPacket, XmpValue};

    let mut packet = XmpPacket::new();
    packet.set("{urn:example}title", XmpValue::Text("v".into()));
    let mut b = PdfBuilder::with_version(PdfVersion::V1_3);
    b.page(PageSize::A4);
    b.set_xmp_packet(packet);

    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("should reject 1.3");
    assert!(
        matches!(error, WriteError::UnsupportedVersion(_)),
        "{error}"
    );
    assert_eq!(out.len(), 0);
}

#[test]
fn raw_and_typed_xmp_are_mutually_exclusive() {
    use crate::core::builder::{XmpPacket, XmpValue};

    let mut packet = XmpPacket::new();
    packet.set("{urn:example}title", XmpValue::Text("v".into()));
    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    b.set_xmp_metadata(b"<x:xmpmeta/>".to_vec());
    b.set_xmp_packet(packet);

    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("ambiguous XMP source should fail");
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert_eq!(out.len(), 0);
}

#[test]
fn typed_xmp_written_to_metadata_stream_has_xpacket_wrapper() {
    use crate::core::builder::{XmpPacket, XmpValue};

    let mut packet = XmpPacket::new();
    packet.set("{urn:example}title", XmpValue::Text("wrapped".into()));
    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    b.set_xmp_packet(packet);
    let doc = build_and_parse(&mut b);

    let xmp = doc.xmp().expect("parse XMP").expect("metadata stream");
    assert!(
        xmp.packet
            .starts_with("<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>"),
        "{}",
        xmp.packet
    );
    assert!(
        xmp.packet.ends_with("<?xpacket end=\"w\"?>"),
        "{}",
        xmp.packet
    );
    assert_eq!(
        xmp.text("{urn:example}title"),
        Some("wrapped"),
        "包装不影响读端属性解析"
    );
}

#[test]
fn catalog_version_writes_override_parsed_by_reader() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_4);
    b.page(PageSize::A4);
    // 目录 /Version 高于文件头时，读端据此升版（1.4 → 1.6）。
    b.set_catalog_version(PdfVersion::V1_6);
    let doc = build_and_parse(&mut b);

    let catalog = doc
        .get_object(1)
        .expect("lookup")
        .expect("catalog")
        .value
        .as_dict()
        .expect("catalog dict")
        .clone();
    assert_eq!(
        catalog.get("Version").and_then(Value::as_name),
        Some("1.6")
    );
    // 读端按目录 /Version 覆盖文件头版本（`source_version()` 方法）。
    assert_eq!(doc.source_version(), PdfVersion::V1_6);
}

#[test]
fn catalog_version_requires_pdf_1_4() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_3);
    b.page(PageSize::A4);
    b.set_catalog_version(PdfVersion::V1_4);
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("低于 1.4 应拒绝目录 /Version");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(_)),
        "{error:?}"
    );
    assert!(error.to_string().contains("/Version"), "{error}");
    assert_eq!(out.len(), 0, "零字节契约");
}
