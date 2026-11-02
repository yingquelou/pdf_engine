//! Tests for the `/OCProperties` reader model: group and default
//! configuration parsing, rejection paths, and OCMD policy and `/VE`
//! visibility semantics.

use super::*;
use crate::core::types::{Array, Dict, Document, Object, PdfTextString, Trailer, Value};

fn doc_with_catalog(catalog: Dict) -> Document {
    let mut doc = Document::new();
    doc.set_object(Object::new(1, 0, Value::Dict(catalog)));
    doc.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(1, 0)),
    };
    doc
}

fn hex_of(text: &str) -> String {
    let bytes = PdfTextString::from(text).encode();
    let mut hex = String::new();
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02X}");
    }
    hex
}

fn sample_document() -> Document {
    let mut doc = Document::new();
    doc.set_object(Object::new(
        1,
        0,
        Value::Dict(
            Dict::new().insert(
                "OCProperties",
                Value::Dict(
                    Dict::new()
                        .insert(
                            "OCGs",
                            Value::Array(
                                Array::new().push(Value::Ref(10, 0)).push(Value::Ref(11, 0)),
                            ),
                        )
                        .insert(
                            "D",
                            Value::Dict(
                                Dict::new()
                                    .insert("BaseState", Value::Name("ON".into()))
                                    .insert(
                                        "ON",
                                        Value::Array(Array::new().push(Value::Ref(10, 0))),
                                    )
                                    .insert(
                                        "OFF",
                                        Value::Array(Array::new().push(Value::Ref(11, 0))),
                                    )
                                    .insert(
                                        "Order",
                                        Value::Array(Array::new().push(Value::Ref(10, 0)).push(
                                            Value::Array(Array::new().push(Value::Ref(11, 0))),
                                        )),
                                    ),
                            ),
                        ),
                ),
            ),
        ),
    ));
    doc.set_object(Object::new(
        10,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("OCG".into()))
                .insert("Name", Value::String("Layer 1".into()))
                .insert(
                    "Intent",
                    Value::Array(
                        Array::new()
                            .push(Value::Name("View".into()))
                            .push(Value::Name("Print".into())),
                    ),
                ),
        ),
    ));
    doc.set_object(Object::new(
        11,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("OCG".into()))
                .insert("Name", Value::HexString(hex_of("图层")))
                .insert("Intent", Value::Name("View".into())),
        ),
    ));
    doc.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(1, 0)),
    };
    doc
}

#[test]
fn parses_groups_and_default_configuration() {
    let doc = sample_document();
    let properties = doc
        .optional_content_properties()
        .expect("parse /OCProperties")
        .expect("expected /OCProperties");
    assert_eq!(properties.groups.len(), 2);
    assert_eq!(properties.groups[0].object, 10);
    assert_eq!(properties.groups[0].name, "Layer 1");
    assert_eq!(properties.groups[0].intent, ["View", "Print"]);
    assert_eq!(properties.groups[1].object, 11);
    assert_eq!(properties.groups[1].name, "图层");
    assert_eq!(properties.groups[1].intent, ["View"]);

    let configuration = properties.default.as_ref().expect("/D");
    assert_eq!(configuration.base_state, BaseState::On);
    assert_eq!(configuration.on, [10]);
    assert_eq!(configuration.off, [11]);
    assert_eq!(configuration.order, [10, 11]);
    assert_eq!(properties.is_visible(10), Some(true));
    assert_eq!(properties.is_visible(11), Some(false));
    assert_eq!(
        properties.group(11).map(|group| group.name.as_str()),
        Some("图层")
    );
}

#[test]
fn base_state_controls_groups_outside_on_and_off() {
    let configuration = OptionalContentConfiguration {
        base_state: BaseState::Off,
        on: vec![10],
        off: vec![10],
        order: Vec::new(),
    };
    assert_eq!(
        configuration.is_visible(10),
        Some(true),
        "/ON wins over /OFF"
    );
    assert_eq!(configuration.is_visible(12), Some(false));
    let unchanged = OptionalContentConfiguration {
        base_state: BaseState::Unchanged,
        on: Vec::new(),
        off: Vec::new(),
        order: Vec::new(),
    };
    assert_eq!(unchanged.is_visible(10), None);
}

#[test]
fn missing_ocproperties_returns_none() {
    let doc = doc_with_catalog(Dict::new().insert("Type", Value::Name("Catalog".into())));
    assert_eq!(doc.optional_content_properties().expect("parse"), None);
}

#[test]
fn rejects_malformed_optional_content() {
    let assert_invalid = |catalog: Dict, expected: &str| {
        let doc = doc_with_catalog(catalog);
        let error = doc
            .optional_content_properties()
            .expect_err("malformed structure should be rejected");
        assert!(
            matches!(&error, OptionalContentError::Invalid(_)),
            "{error}"
        );
        assert!(error.to_string().contains(expected), "{error}");
    };

    assert_invalid(
        Dict::new().insert("OCProperties", Value::Dict(Dict::new())),
        "/OCGs is required",
    );
    assert_invalid(
        Dict::new().insert(
            "OCProperties",
            Value::Dict(
                Dict::new()
                    .insert("OCGs", Value::Array(Array::new()))
                    .insert(
                        "D",
                        Value::Dict(Dict::new().insert("BaseState", Value::Name("Maybe".into()))),
                    ),
            ),
        ),
        "/BaseState",
    );
}

#[test]
fn membership_policy_matrix() {
    let is_visible = |object: i64| -> Option<bool> {
        match object {
            10 => Some(true),
            11 => Some(false),
            _ => None,
        }
    };
    let ocmd = |policy: &str| {
        Dict::new()
            .insert(
                "OCGs",
                Value::Array(Array::new().push(Value::Ref(10, 0)).push(Value::Ref(11, 0))),
            )
            .insert("P", Value::Name(policy.into()))
    };
    assert_eq!(
        membership_visibility(&ocmd("AllOn"), &is_visible),
        Some(false)
    );
    assert_eq!(
        membership_visibility(&ocmd("AnyOn"), &is_visible),
        Some(true)
    );
    assert_eq!(
        membership_visibility(&ocmd("AnyOff"), &is_visible),
        Some(true)
    );
    assert_eq!(
        membership_visibility(&ocmd("AllOff"), &is_visible),
        Some(false)
    );

    let defaulted = Dict::new().insert(
        "OCGs",
        Value::Array(Array::new().push(Value::Ref(10, 0)).push(Value::Ref(11, 0))),
    );
    assert_eq!(
        membership_visibility(&defaulted, &is_visible),
        Some(false),
        "/P defaults to /AllOn"
    );
    assert_eq!(membership_visibility(&ocmd("Maybe"), &is_visible), None);
    assert_eq!(membership_visibility(&Dict::new(), &is_visible), None);

    let unknown_group =
        Dict::new().insert("OCGs", Value::Array(Array::new().push(Value::Ref(99, 0))));
    assert_eq!(membership_visibility(&unknown_group, &is_visible), None);

    let empty = OptionalContentProperties {
        groups: Vec::new(),
        default: None,
    };
    assert_eq!(empty.membership_visible(&ocmd("AllOn")), None);
}

#[test]
fn membership_visibility_expression_overrides_policy() {
    let is_visible = |object: i64| -> Option<bool> {
        match object {
            10 => Some(true),
            11 => Some(false),
            _ => None,
        }
    };
    // /AnyOn alone is false (group 11 is off), but /Or 10 11 is true.
    let ve_wins = Dict::new()
        .insert("OCGs", Value::Array(Array::new().push(Value::Ref(11, 0))))
        .insert("P", Value::Name("AnyOn".into()))
        .insert(
            "VE",
            Value::Array(
                Array::new()
                    .push(Value::Name("Or".into()))
                    .push(Value::Ref(10, 0))
                    .push(Value::Ref(11, 0)),
            ),
        );
    assert_eq!(membership_visibility(&ve_wins, &is_visible), Some(true));

    let nested = Dict::new().insert(
        "VE",
        Value::Array(
            Array::new()
                .push(Value::Name("And".into()))
                .push(Value::Array(
                    Array::new()
                        .push(Value::Name("Or".into()))
                        .push(Value::Ref(10, 0))
                        .push(Value::Ref(11, 0)),
                ))
                .push(Value::Array(
                    Array::new()
                        .push(Value::Name("Not".into()))
                        .push(Value::Ref(11, 0)),
                )),
        ),
    );
    assert_eq!(membership_visibility(&nested, &is_visible), Some(true));

    let short_circuit = Dict::new().insert(
        "VE",
        Value::Array(
            Array::new()
                .push(Value::Name("And".into()))
                .push(Value::Ref(11, 0))
                .push(Value::Ref(99, 0)),
        ),
    );
    assert_eq!(
        membership_visibility(&short_circuit, &is_visible),
        Some(false),
        "a known false /And operand absorbs an unknown one"
    );
}

#[test]
fn rejects_invalid_group_entries() {
    let mut doc = Document::new();
    doc.set_object(Object::new(
        1,
        0,
        Value::Dict(Dict::new().insert(
            "OCProperties",
            Value::Dict(
                Dict::new().insert("OCGs", Value::Array(Array::new().push(Value::Integer(10)))),
            ),
        )),
    ));
    doc.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(1, 0)),
    };
    let error = doc
        .optional_content_properties()
        .expect_err("direct values are not OCG references");
    assert!(error.to_string().contains("indirect references"), "{error}");

    let mut doc = Document::new();
    doc.set_object(Object::new(
        1,
        0,
        Value::Dict(Dict::new().insert(
            "OCProperties",
            Value::Dict(
                Dict::new().insert("OCGs", Value::Array(Array::new().push(Value::Ref(10, 0)))),
            ),
        )),
    ));
    doc.set_object(Object::new(
        10,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("OCG".into()))
                .insert("Intent", Value::Name("View".into())),
        ),
    ));
    doc.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(1, 0)),
    };
    let error = doc
        .optional_content_properties()
        .expect_err("missing /Name should be rejected");
    assert!(error.to_string().contains("/Name"), "{error}");
}
