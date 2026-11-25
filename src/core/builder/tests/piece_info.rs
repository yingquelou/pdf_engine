//! 页面件字典（14.5 Table 28/30/318/319）读写往返、版本门禁与校验。

use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::core::reader::piece_info::PieceInfoError;
use crate::core::types::{Document, PdfDate, PdfVersion};

fn build_and_parse(b: &mut PdfBuilder) -> Document {
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
        "pdf-engine-piece-info-{}-{}.pdf",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&path, &out).expect("write temp pdf");
    let file = std::fs::File::open(&path).expect("open temp pdf");
    let mut parser = crate::core::reader::Parser::new(file).expect("parser");
    let doc = parser.parse_document().expect("parse");
    let _ = std::fs::remove_file(&path);
    doc
}

fn date() -> PdfDate {
    PdfDate::parse("D:20261030093015Z").expect("date")
}

#[test]
fn page_and_catalog_piece_info_roundtrip_through_parser() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    b.set_page_piece_info(
        page,
        PieceInfo::new().with(
            "Acme.Editor",
            PieceInfoData::new(date()).with_private(Value::Integer(7)),
        ),
    );
    b.set_catalog_piece_info(PieceInfo::new().with("Acme.Doc", PieceInfoData::new(date())));
    let doc = build_and_parse(&mut b);

    let page_info = doc
        .page_piece_info(page)
        .expect("page piece info")
        .expect("present");
    let data = page_info.get("Acme.Editor").expect("product entry");
    assert_eq!(data.last_modified.as_deref(), Some("D:20261030093015Z"));
    assert!(data.last_modified_parsed.is_some());
    assert_eq!(data.private, Some(Value::Integer(7)));

    let catalog_info = doc
        .catalog_piece_info()
        .expect("catalog piece info")
        .expect("present");
    assert!(catalog_info.get("Acme.Doc").is_some());
    assert!(catalog_info.get("Acme.Doc").unwrap().private.is_none());
}

#[test]
fn page_piece_info_requires_pdf_1_3() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_2);
    let page = b.page(PageSize::A4);
    b.set_page_piece_info(
        page,
        PieceInfo::new().with("Acme", PieceInfoData::new(date())),
    );
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("page /PieceInfo needs 1.3");
    assert!(
        matches!(error, WriteError::UnsupportedVersion(_)),
        "{error}"
    );
    assert!(out.is_empty());
}

#[test]
fn catalog_piece_info_requires_pdf_1_4() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_3);
    let _ = b.page(PageSize::A4);
    b.set_catalog_piece_info(PieceInfo::new().with("Acme", PieceInfoData::new(date())));
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("catalog /PieceInfo needs 1.4");
    assert!(
        matches!(error, WriteError::UnsupportedVersion(_)),
        "{error}"
    );
    assert!(out.is_empty());
}

#[test]
fn duplicate_piece_info_products_are_rejected() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    b.set_page_piece_info(
        page,
        PieceInfo::new()
            .with("Acme", PieceInfoData::new(date()))
            .with("Acme", PieceInfoData::new(date())),
    );
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("duplicate product must be rejected");
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert!(
        error.to_string().contains("appears more than once"),
        "{error}"
    );
    assert!(out.is_empty());
}

#[test]
fn form_xobject_piece_info_roundtrips() {
    use crate::core::content::ContentStream;
    let mut content = ContentStream::new();
    content
        .save_state()
        .rect(0.0, 0.0, 10.0, 10.0)
        .fill()
        .restore_state();
    let form = FormXObject::new([0.0, 0.0, 10.0, 10.0], content)
        .with_piece_info(PieceInfo::new().with("Acme.Form", PieceInfoData::new(date())));
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let handle = b.form_xobject(&form);
    b.draw_form(page, handle, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    let doc = build_and_parse(&mut b);

    let page_obj = doc.get_object(3).unwrap().expect("page object");
    let resources = page_obj
        .value
        .as_dict()
        .unwrap()
        .get("Resources")
        .and_then(Value::as_dict)
        .expect("/Resources");
    let xobjects = resources
        .get("XObject")
        .and_then(Value::as_dict)
        .expect("/XObject");
    let (num, _) = xobjects
        .get("Fm1")
        .and_then(Value::as_ref)
        .expect("form reference");
    let form_obj = doc.get_object(num).unwrap().expect("form object");
    let info = doc
        .form_piece_info(&form_obj.value)
        .expect("form piece info")
        .expect("present");
    assert!(info.get("Acme.Form").is_some());
}

#[test]
fn piece_info_rejects_non_string_last_modified() {
    let doc = Document::new();
    let value = Value::Dict(Dict::new().insert(
        "Acme",
        Value::Dict(Dict::new().insert("LastModified", Value::Integer(1))),
    ));
    let error = doc.piece_info(&value).expect_err("integer date is invalid");
    assert!(matches!(error, PieceInfoError::Malformed(_)), "{error}");
}
