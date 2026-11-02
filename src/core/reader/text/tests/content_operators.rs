//! Content-stream operator basics: `TJ` arrays, block layout, `Td` separators,
//! page-level convenience entry and tokenizer edge cases.

use crate::core::types::{
    Array, Dict, Document, Object, StartXref, StreamData, Trailer, Value, Xref,
};

use super::support::{resources_with_font, winansi_type1_font};

#[test]
fn tj_array_with_positioning() {
    let document = Document::new();
    let resources = resources_with_font("F1", winansi_type1_font());
    let content = b"BT /F1 12 Tf [(He) -200 (llo)] TJ ET";
    let text = document.extract_text(content, &resources);
    assert_eq!(text, "He llo");
}

#[test]
fn multiple_text_blocks() {
    let document = Document::new();
    let resources = resources_with_font("F1", winansi_type1_font());
    let content = b"BT /F1 12 Tf (First) Tj ET BT /F1 12 Tf (Second) Tj ET";
    let text = document.extract_text(content, &resources);
    assert_eq!(text, "FirstSecond");
}

#[test]
fn td_inserts_newline() {
    let document = Document::new();
    let resources = resources_with_font("F1", winansi_type1_font());
    let content = b"BT /F1 12 Tf (Line1) Tj 0 -14 Td (Line2) Tj ET";
    let text = document.extract_text(content, &resources);
    assert_eq!(text, "Line1\nLine2");
}

#[test]
fn hex_string_decoded() {
    let document = Document::new();
    let resources = resources_with_font("F1", winansi_type1_font());
    let content = b"BT /F1 12 Tf <48656C6C6F> Tj ET";
    let text = document.extract_text(content, &resources);
    assert_eq!(text, "Hello");
}

#[test]
fn tick_operator_inserts_newline() {
    let document = Document::new();
    let resources = resources_with_font("F1", winansi_type1_font());
    let content = b"BT /F1 12 Tf (A) Tj (B) ' ET";
    let text = document.extract_text(content, &resources);
    assert_eq!(text, "A\nB");
}

#[test]
fn extract_page_text_from_constructed_document() {
    let font = winansi_type1_font();
    let resources = Value::Dict(resources_with_font("F1", font));
    let content = Value::Stream(
        Dict::new(),
        StreamData::Owned(b"BT /F1 12 Tf (Page1) Tj ET".to_vec()),
    );
    let page_dict = Value::Dict(
        Dict::new()
            .insert("Type", Value::Name("Page".into()))
            .insert("Contents", content)
            .insert("Resources", resources),
    );
    let pages_dict = Value::Dict(
        Dict::new()
            .insert("Type", Value::Name("Pages".into()))
            .insert("Kids", Value::Array(Array::new().push(Value::Ref(2, 0))))
            .insert("Count", Value::Integer(1)),
    );
    let catalog = Value::Dict(
        Dict::new()
            .insert("Type", Value::Name("Catalog".into()))
            .insert("Pages", pages_dict),
    );

    let mut trailer = Trailer::new();
    trailer.dict = Dict::new().insert("Root", Value::Ref(1, 0));

    let document = Document::from_objects(
        vec![Object::new(1, 0, catalog), Object::new(2, 0, page_dict)],
        Xref::new(),
        trailer,
        StartXref(0),
    );

    let text = document.extract_page_text(0).unwrap();
    assert_eq!(text, "Page1");
}

#[test]
fn tokenizer_handles_comments_and_whitespace() {
    let document = Document::new();
    let resources = resources_with_font("F1", winansi_type1_font());
    let content = b"% this is a comment\nBT /F1 12 Tf (OK) Tj ET";
    let text = document.extract_text(content, &resources);
    assert_eq!(text, "OK");
}

#[test]
fn empty_content_stream_returns_empty() {
    let document = Document::new();
    let resources = Dict::new();
    let text = document.extract_text(b"", &resources);
    assert_eq!(text, "");
}

#[test]
fn inline_image_data_is_skipped_without_losing_following_text() {
    let document = Document::new();
    let resources = resources_with_font("F1", winansi_type1_font());
    // The image data contains an unmatched "(" that would otherwise consume
    // the rest of the stream as a literal string.
    let content = b"BT /F1 12 Tf (A) Tj ET \
                    BI /W 1 /H 1 /BPC 8 /CS /G ID ( EI \
                    BT /F1 12 Tf (B) Tj ET";

    assert_eq!(document.extract_text(content, &resources), "AB");
}
