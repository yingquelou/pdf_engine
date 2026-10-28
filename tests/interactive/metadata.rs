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
