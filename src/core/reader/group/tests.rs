use crate::core::reader::color_space::ColorSpaceModel;
use crate::core::types::{Array, Dict, Document, Object, Trailer, Value};

use super::{GroupError, GroupSubtype};

fn dict(entries: Vec<(&str, Value)>) -> Dict {
    let mut dict = Dict::new();
    for (key, value) in entries {
        dict = dict.insert(key, value);
    }
    dict
}

fn document_with_page_group(page_group: Option<Value>) -> Document {
    let mut document = Document::new();
    document.set_object(Object::new(
        1,
        0,
        Value::Dict(dict(vec![
            ("Type", Value::Name("Catalog".into())),
            ("Pages", Value::Ref(2, 0)),
        ])),
    ));
    document.set_object(Object::new(
        2,
        0,
        Value::Dict(dict(vec![
            ("Type", Value::Name("Pages".into())),
            ("Kids", Value::Array(Array(vec![Value::Ref(3, 0)]))),
            ("Count", Value::Integer(1)),
        ])),
    ));
    let mut page = dict(vec![
        ("Type", Value::Name("Page".into())),
        ("Parent", Value::Ref(2, 0)),
    ]);
    if let Some(group) = page_group {
        page = page.insert("Group", group);
    }
    document.set_object(Object::new(3, 0, Value::Dict(page)));
    document.trailer = Trailer {
        dict: Dict::new()
            .insert("Root", Value::Ref(1, 0))
            .insert("Size", Value::Integer(4)),
    };
    document
}

fn assert_group_invalid(value: Value, needle: &str) {
    let document = Document::new();
    match document.group(&value) {
        Err(error) => {
            let message = error.to_string();
            assert!(
                message.contains(needle),
                "error {message:?} should mention {needle}"
            );
        }
        Ok(group) => panic!("expected an invalid group, got {group:?}"),
    }
}

#[test]
fn parses_transparency_group_with_color_space() {
    let document = Document::new();
    let group = document
        .group(&Value::Dict(dict(vec![
            ("Type", Value::Name("Group".into())),
            ("S", Value::Name("Transparency".into())),
            ("CS", Value::Name("DeviceRGB".into())),
            ("I", Value::Bool(true)),
            ("K", Value::Bool(false)),
            (
                "B",
                Value::Array(Array(vec![
                    Value::Integer(0),
                    Value::Integer(0),
                    Value::Integer(100),
                    Value::Integer(100),
                ])),
            ),
        ])))
        .expect("transparency group should parse");

    assert_eq!(group.subtype, GroupSubtype::Transparency);
    assert_eq!(group.color_space, Some(ColorSpaceModel::DeviceRGB));
    assert_eq!(group.color_space_raw, Some(Value::Name("DeviceRGB".into())));
    assert!(group.isolated);
    assert_eq!(group.knockout, Some(false));
    assert_eq!(
        group.other.get("B"),
        Some(&Value::Array(Array(vec![
            Value::Integer(0),
            Value::Integer(0),
            Value::Integer(100),
            Value::Integer(100),
        ])))
    );
    assert_eq!(group.other.len(), 1, "only /B stays unmodeled");
}

#[test]
fn parses_deprecated_form_group_and_applies_isolated_default() {
    let document = document_with_objects(vec![
        (
            9,
            Value::Dict(dict(vec![
                ("S", Value::Name("Form".into())),
                ("CS", Value::Ref(10, 0)),
            ])),
        ),
        (10, Value::Name("DeviceCMYK".into())),
    ]);
    let group = document
        .group(&Value::Ref(9, 0))
        .expect("form subtype group should parse");

    assert_eq!(group.subtype, GroupSubtype::Form);
    assert_eq!(group.color_space, Some(ColorSpaceModel::DeviceCMYK));
    assert!(!group.isolated, "/I absent defaults to false");
    assert_eq!(group.knockout, None);
}

fn document_with_objects(objects: Vec<(i64, Value)>) -> Document {
    let document = Document::new();
    for (num, value) in objects {
        document.set_object(Object::new(num, 0, value));
    }
    document
}

#[test]
fn indirect_group_reference_is_resolved() {
    let document = document_with_objects(vec![(
        9,
        Value::Dict(dict(vec![
            ("S", Value::Name("Transparency".into())),
            ("K", Value::Bool(true)),
        ])),
    )]);
    let group = document
        .group(&Value::Ref(9, 0))
        .expect("indirect group should resolve");
    assert_eq!(group.subtype, GroupSubtype::Transparency);
    assert_eq!(group.knockout, Some(true));
}

#[test]
fn page_group_reads_the_page_dictionary() {
    let document = document_with_page_group(Some(Value::Dict(dict(vec![
        ("S", Value::Name("Transparency".into())),
        ("CS", Value::Name("DeviceGray".into())),
        ("K", Value::Bool(true)),
    ]))));
    let group = document
        .page_group(0)
        .expect("page group should parse")
        .expect("page has a group");
    assert_eq!(group.subtype, GroupSubtype::Transparency);
    assert_eq!(group.color_space, Some(ColorSpaceModel::DeviceGray));
    assert_eq!(group.knockout, Some(true));

    let plain = document_with_page_group(None);
    assert!(plain.page_group(0).expect("parse").is_none());
    let error = plain.page_group(5).expect_err("missing page");
    assert!(matches!(error, GroupError::PageContent(_)));
}

#[test]
fn rejects_malformed_groups() {
    assert_group_invalid(Value::Integer(1), "group must be a dictionary");
    assert_group_invalid(Value::Dict(dict(vec![])), "/S is required");
    assert_group_invalid(
        Value::Dict(dict(vec![("S", Value::Name("Layer".into()))])),
        "group /S must be /Transparency or /Form, got /Layer",
    );
    assert_group_invalid(
        Value::Dict(dict(vec![
            ("S", Value::Name("Transparency".into())),
            ("CS", Value::Array(Array(vec![Value::Name("Lab".into())]))),
        ])),
        "invalid group /CS",
    );
    assert_group_invalid(
        Value::Dict(dict(vec![
            ("S", Value::Name("Transparency".into())),
            ("I", Value::Integer(1)),
        ])),
        "group /I must be a boolean",
    );
    assert_group_invalid(
        Value::Dict(dict(vec![
            ("S", Value::Name("Transparency".into())),
            ("K", Value::Name("true".into())),
        ])),
        "group /K must be a boolean",
    );
    let document = Document::new();
    let error = document
        .group(&Value::Ref(404, 0))
        .expect_err("unresolved reference must fail");
    assert!(matches!(error, GroupError::Invalid(_)));
    assert!(
        error
            .to_string()
            .contains("unresolved object reference 404 0 R")
    );
}
