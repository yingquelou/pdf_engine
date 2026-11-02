//! Tests for parsing and validating the `/CIDSystemInfo` dictionary.

use crate::core::types::{Dict, Document, Value};

use super::cid_font;

#[test]
fn parses_cid_system_info() {
    let document = Document::new();
    let font = cid_font(vec![(
        "CIDSystemInfo",
        Value::Dict(
            Dict::new()
                .insert("Registry", Value::String("Adobe".into()))
                .insert("Ordering", Value::String("GB1".into()))
                .insert("Supplement", Value::Integer(5)),
        ),
    )]);

    let font = document.cid_font_type2(&font).unwrap().unwrap();
    let info = font.system_info();
    assert_eq!(info.registry, "Adobe");
    assert_eq!(info.ordering, "GB1");
    assert_eq!(info.supplement, 5);
}

#[test]
fn rejects_missing_cid_system_info() {
    let document = Document::new();
    let dict = Dict::new()
        .insert("Subtype", Value::Name("CIDFontType2".into()))
        .insert("BaseFont", Value::Name("ExampleCIDFont".into()))
        .insert(
            "FontDescriptor",
            Value::Dict(Dict::new().insert("Type", Value::Name("FontDescriptor".into()))),
        );
    // Missing CIDSystemInfo
    let font = Value::Dict(dict);
    assert!(document.cid_font_type2(&font).is_err());
}

#[test]
fn rejects_invalid_cid_system_info_fields() {
    let document = Document::new();

    // Missing Registry
    let font = cid_font(vec![(
        "CIDSystemInfo",
        Value::Dict(
            Dict::new()
                .insert("Ordering", Value::String("GB1".into()))
                .insert("Supplement", Value::Integer(0)),
        ),
    )]);
    assert!(document.cid_font_type2(&font).is_err());

    // Missing Ordering
    let font = cid_font(vec![(
        "CIDSystemInfo",
        Value::Dict(
            Dict::new()
                .insert("Registry", Value::String("Adobe".into()))
                .insert("Supplement", Value::Integer(0)),
        ),
    )]);
    assert!(document.cid_font_type2(&font).is_err());

    // Missing Supplement
    let font = cid_font(vec![(
        "CIDSystemInfo",
        Value::Dict(
            Dict::new()
                .insert("Registry", Value::String("Adobe".into()))
                .insert("Ordering", Value::String("GB1".into())),
        ),
    )]);
    assert!(document.cid_font_type2(&font).is_err());

    // Negative Supplement
    let font = cid_font(vec![(
        "CIDSystemInfo",
        Value::Dict(
            Dict::new()
                .insert("Registry", Value::String("Adobe".into()))
                .insert("Ordering", Value::String("GB1".into()))
                .insert("Supplement", Value::Integer(-1)),
        ),
    )]);
    assert!(document.cid_font_type2(&font).is_err());

    // Non-string Registry
    let font = cid_font(vec![(
        "CIDSystemInfo",
        Value::Dict(
            Dict::new()
                .insert("Registry", Value::Integer(123))
                .insert("Ordering", Value::String("GB1".into()))
                .insert("Supplement", Value::Integer(0)),
        ),
    )]);
    assert!(document.cid_font_type2(&font).is_err());
}
