//! 元数据写端测试：trailer `/Info` 文档信息字典与 `/ID` 文件标识符
//! （ISO 32000-1 14.3.3、14.4）。

use super::*;

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
