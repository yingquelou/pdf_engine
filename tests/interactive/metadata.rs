//! E5 元数据与文件标识：`/Info`、trailer `/ID` 与 XMP `/Metadata` 的写出→解析往返。

use super::*;

/// 写入 → 重新解析：trailer `/Info` 与 `/ID` 应经 XRef 流路径完整回读。
#[test]
fn builder_metadata_roundtrips_through_xref_stream_parser() {
    use pdf_engine::core::builder::{DocumentInfo, FileIdentifier, PageSize, PdfBuilder};
    use pdf_engine::core::types::Value;

    let mut builder = PdfBuilder::new();
    builder.page(PageSize::A4);
    builder.set_document_info(
        DocumentInfo::new()
            .title("报告")
            .author("Qoder")
            .producer("pdf_engine"),
    );
    builder.set_file_identifier(FileIdentifier::new([0x11; 16], [0x22; 16]));
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");

    let doc = parse_buf(&buf);
    let info = doc.document_info().expect("/Info should roundtrip");
    match info.title {
        Some(Value::HexString(hex)) => {
            assert!(hex.starts_with("FEFF"), "UTF-16BE with BOM: {hex}")
        }
        other => panic!("expected hex string title, got {other:?}"),
    }
    let (perm, upd) = doc.file_identifier().expect("/ID should roundtrip");
    assert_eq!(perm, [0x11; 16]);
    assert_eq!(upd, [0x22; 16]);
}

/// 写入 → 重新解析：经典 xref 表路径（1.4）也须透传 trailer `/Info` 与 `/ID`。
#[test]
fn builder_metadata_roundtrips_through_classic_table_parser() {
    use pdf_engine::core::builder::{DocumentInfo, FileIdentifier, PageSize, PdfBuilder};
    use pdf_engine::core::types::PdfVersion;

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_4);
    builder.page(PageSize::A4);
    builder.set_document_info(DocumentInfo::new().title("Plain"));
    builder.set_file_identifier(FileIdentifier::generate(b"material".as_slice()));
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");

    let doc = parse_buf(&buf);
    let info = doc.document_info().expect("/Info via classic trailer");
    assert!(info.title.is_some());
    let (perm, upd) = doc.file_identifier().expect("/ID via classic trailer");
    assert_eq!(perm, upd, "generated identifier keeps IDs equal");
}

/// 写入 → 重新解析：XMP `/Metadata` 流应可经读端解回原文字节。
#[test]
fn builder_xmp_metadata_roundtrips_through_parser() {
    use pdf_engine::core::builder::{PageSize, PdfBuilder};

    let mut builder = PdfBuilder::new();
    builder.page(PageSize::A4);
    let xml = br##"<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?><x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"/></x:xmpmeta><?xpacket end="w"?>"##.to_vec();
    builder.set_xmp_metadata(xml.clone());
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");

    let doc = parse_buf(&buf);
    assert_eq!(doc.xmp_metadata().as_deref(), Some(xml.as_slice()));
}

/// 写入 → 重新解析：类型化日期与 `/Trapped` 经读端还原（14.3.3 Table 317）。
#[test]
fn builder_metadata_typed_dates_and_trapped_roundtrip_through_parser() {
    use pdf_engine::core::builder::{DocumentInfo, PageSize, PdfBuilder};
    use pdf_engine::core::reader::metadata::TrappedState;
    use pdf_engine::core::types::PdfDate;

    let created = PdfDate::parse("D:20261029120000+08'00'").expect("date");
    let modified = PdfDate::parse("D:20261030093015Z").expect("date");
    let mut builder = PdfBuilder::new();
    builder.page(PageSize::A4);
    builder.set_document_info(
        DocumentInfo::new()
            .title("Dated")
            .creation_date_pdf(created)
            .mod_date_pdf(modified)
            .trapped(TrappedState::Unknown),
    );
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");

    let doc = parse_buf(&buf);
    let info = doc.document_info().expect("/Info should roundtrip");
    assert_eq!(info.creation_date_parsed(), Some(created));
    assert_eq!(info.mod_date_parsed(), Some(modified));
    assert_eq!(info.trapped_state(), Some(TrappedState::Unknown));
    assert_eq!(
        info.dict.get("Trapped"),
        Some(&pdf_engine::core::types::Value::Name("Unknown".into()))
    );
}

/// 读端：无 `/Metadata` 的文档返回 `None`。
#[test]
fn xmp_metadata_absent_returns_none() {
    use pdf_engine::core::builder::{PageSize, PdfBuilder};

    let mut builder = PdfBuilder::new();
    builder.page(PageSize::A4);
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let doc = parse_buf(&buf);
    assert!(doc.xmp_metadata().is_none());
}

/// 类型化 XMP 包：文本、语言备选列表、嵌套资源与类型化值写出后，
/// 经读端 `Document::xmp()` 还原为相同的 `XmpValue`。
#[test]
fn builder_typed_xmp_packet_roundtrips_through_reader() {
    use pdf_engine::core::builder::{
        PageSize, PdfBuilder, XmpListItem, XmpListKind, XmpPacket, XmpValue,
    };
    use pdf_engine::core::reader::xmp::{
        XmpListItem as ReaderListItem, XmpListKind as ReaderListKind, XmpValue as ReaderValue,
    };
    use pdf_engine::core::types::PdfVersion;
    use std::collections::BTreeMap;

    const DC: &str = "http://purl.org/dc/elements/1.1/";
    const EX: &str = "urn:example:typed";
    const W3CDTF: &str = "http://purl.org/dc/terms/W3CDTF";

    fn clark(namespace: &str, local: &str) -> String {
        format!("{{{namespace}}}{local}")
    }

    let mut packet = XmpPacket::new();
    packet
        .set(clark(DC, "title"), XmpValue::Text("Roundtrip".into()))
        .set(
            clark(DC, "description"),
            XmpValue::List {
                kind: XmpListKind::Alt,
                items: vec![
                    XmpListItem {
                        lang: Some("x-default".into()),
                        value: "Default".into(),
                    },
                    XmpListItem {
                        lang: Some("fr".into()),
                        value: "Défaut".into(),
                    },
                ],
            },
        )
        .set(
            clark(EX, "author"),
            XmpValue::Resource(BTreeMap::from([
                (clark(EX, "name"), XmpValue::Text("Ada".into())),
                (
                    clark(EX, "address"),
                    XmpValue::Resource(BTreeMap::from([(
                        clark(EX, "city"),
                        XmpValue::Text("London".into()),
                    )])),
                ),
            ])),
        )
        .set(
            clark(EX, "created"),
            XmpValue::Typed {
                value: Box::new(XmpValue::Text("2024-05-01T09:00:00Z".into())),
                datatype: Some(W3CDTF.into()),
                language: Some("en".into()),
            },
        );

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    builder.page(PageSize::A4);
    builder.set_xmp_packet(packet);
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");

    let doc = parse_buf(&buf);
    let xmp = doc.xmp().expect("parse XMP").expect("/Metadata stream");
    assert!(xmp.packet.starts_with("<?xpacket begin="));
    assert!(xmp.packet.contains("<x:xmpmeta"));
    assert!(xmp.packet.ends_with("<?xpacket end=\"w\"?>"));
    assert_eq!(xmp.text(&clark(DC, "title")), Some("Roundtrip"));
    assert_eq!(
        xmp.properties.get(&clark(DC, "description")),
        Some(&ReaderValue::List {
            kind: ReaderListKind::Alt,
            items: vec![
                ReaderListItem {
                    lang: Some("x-default".into()),
                    value: "Default".into(),
                },
                ReaderListItem {
                    lang: Some("fr".into()),
                    value: "Défaut".into(),
                },
            ],
        })
    );
    let Some(ReaderValue::Resource(author)) = xmp.properties.get(&clark(EX, "author")) else {
        panic!("author must roundtrip as a resource");
    };
    assert_eq!(
        author.get(&clark(EX, "name")),
        Some(&ReaderValue::Text("Ada".into()))
    );
    let Some(ReaderValue::Resource(address)) = author.get(&clark(EX, "address")) else {
        panic!("address must roundtrip as a nested resource");
    };
    assert_eq!(
        address.get(&clark(EX, "city")),
        Some(&ReaderValue::Text("London".into()))
    );
    assert_eq!(
        xmp.properties.get(&clark(EX, "created")),
        Some(&ReaderValue::Typed {
            value: Box::new(ReaderValue::Text("2024-05-01T09:00:00Z".into())),
            datatype: Some(W3CDTF.into()),
            language: Some("en".into()),
        })
    );
}

/// `rdf:resource` 引用与 `rdf:parseType="Literal"` 原始标记写出后，
/// 经 `Document::xmp()` 逐字还原（14.3.2）。
#[test]
fn builder_xmp_references_and_literals_roundtrip() {
    use pdf_engine::core::builder::{PageSize, PdfBuilder, XmpPacket, XmpValue};
    use pdf_engine::core::reader::xmp::XmpValue as ReaderValue;
    use pdf_engine::core::types::PdfVersion;

    const EX: &str = "urn:example:values";
    let reference = "https://example.com/a?x=1&y=2";
    let literal = r#"<span xmlns="urn:ex">hi &amp; bye</span>"#;

    fn clark(namespace: &str, local: &str) -> String {
        format!("{{{namespace}}}{local}")
    }

    let mut packet = XmpPacket::new();
    packet
        .set(clark(EX, "link"), XmpValue::Reference(reference.into()))
        .set(clark(EX, "markup"), XmpValue::Literal(literal.into()))
        .set(clark(EX, "empty"), XmpValue::Literal(String::new()));

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    builder.page(PageSize::A4);
    builder.set_xmp_packet(packet);
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");

    let doc = parse_buf(&buf);
    let xmp = doc.xmp().expect("parse XMP").expect("/Metadata stream");
    assert_eq!(
        xmp.properties.get(&clark(EX, "link")),
        Some(&ReaderValue::Reference(reference.into()))
    );
    assert_eq!(
        xmp.properties.get(&clark(EX, "markup")),
        Some(&ReaderValue::Literal(literal.into()))
    );
    assert_eq!(
        xmp.properties.get(&clark(EX, "empty")),
        Some(&ReaderValue::Literal(String::new()))
    );
    assert_eq!(xmp.text(&clark(EX, "link")), None);
    assert_eq!(xmp.text(&clark(EX, "markup")), None);
}

/// `rdf:parseType="Collection"` 集合写出后经 `Document::xmp()` 还原为
/// 相同的条目序列（14.3.2）。
#[test]
fn builder_xmp_collection_roundtrips_through_reader() {
    use pdf_engine::core::builder::{PageSize, PdfBuilder, XmpPacket, XmpValue};
    use pdf_engine::core::reader::xmp::XmpValue as ReaderValue;
    use pdf_engine::core::types::PdfVersion;
    use std::collections::BTreeMap;

    const EX: &str = "urn:example:collection";

    fn clark(namespace: &str, local: &str) -> String {
        format!("{{{namespace}}}{local}")
    }

    let mut packet = XmpPacket::new();
    packet.set(
        clark(EX, "items"),
        XmpValue::Collection(vec![
            XmpValue::Text("plain".into()),
            XmpValue::Resource(BTreeMap::from([(
                clark(EX, "name"),
                XmpValue::Text("Ada".into()),
            )])),
            XmpValue::Reference("urn:example:ref".into()),
            XmpValue::Collection(vec![XmpValue::Text("nested".into())]),
        ]),
    );

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    builder.page(PageSize::A4);
    builder.set_xmp_packet(packet);
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");

    let doc = parse_buf(&buf);
    let xmp = doc.xmp().expect("parse XMP").expect("/Metadata stream");
    assert_eq!(
        xmp.properties.get(&clark(EX, "items")),
        Some(&ReaderValue::Collection(vec![
            ReaderValue::Text("plain".into()),
            ReaderValue::Resource(BTreeMap::from([(
                clark(EX, "name"),
                ReaderValue::Text("Ada".into()),
            )])),
            ReaderValue::Reference("urn:example:ref".into()),
            ReaderValue::Collection(vec![ReaderValue::Text("nested".into())]),
        ]))
    );
    assert_eq!(xmp.text(&clark(EX, "items")), None);
}
