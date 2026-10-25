use crate::core::types::{Array, Dict, Document, Object, Value};

use super::{BlendMode, DashPattern, ExtGStateFont, GraphicsStateError, parse_ext_g_state};

fn document_with_objects(objects: Vec<(i64, Value)>) -> Document {
    let document = Document::new();
    for (num, value) in objects {
        document.set_object(Object::new(num, 0, value));
    }
    document
}

fn state(entries: Vec<(&str, Value)>) -> Dict {
    let mut dict = Dict::new();
    for (key, value) in entries {
        dict = dict.insert(key, value);
    }
    dict
}

fn assert_invalid(document: &Document, entries: Vec<(&str, Value)>, needle: &str) {
    let dict = state(entries);
    match parse_ext_g_state(document, &dict) {
        Err(GraphicsStateError::Invalid(message)) => assert!(
            message.contains(needle),
            "error {message:?} should mention {needle}"
        ),
        other => panic!("expected an invalid graphics state, got {other:?}"),
    }
}

#[test]
fn parses_scalar_graphics_state_parameters() {
    let document = Document::new();
    let dict = state(vec![
        ("Type", Value::Name("ExtGState".into())),
        ("LW", Value::Float(2.5)),
        ("LC", Value::Integer(1)),
        ("LJ", Value::Integer(2)),
        ("ML", Value::Integer(4)),
        ("RI", Value::Name("RelativeColorimetric".into())),
        ("OP", Value::Bool(true)),
        ("op", Value::Bool(false)),
        ("OPM", Value::Integer(1)),
        ("FL", Value::Integer(50)),
        ("SM", Value::Float(0.25)),
        ("SA", Value::Bool(true)),
        ("CA", Value::Float(0.8)),
        ("ca", Value::Float(0.6)),
        ("AIS", Value::Bool(true)),
        ("TK", Value::Bool(false)),
    ]);

    let state = parse_ext_g_state(&document, &dict).expect("state should parse");

    assert_eq!(state.line_width, Some(2.5));
    assert_eq!(state.line_cap, Some(1));
    assert_eq!(state.line_join, Some(2));
    assert_eq!(state.miter_limit, Some(4.0));
    assert_eq!(
        state.rendering_intent.as_deref(),
        Some("RelativeColorimetric")
    );
    assert_eq!(state.overprint, Some(true));
    assert_eq!(state.nonstroking_overprint, Some(false));
    assert_eq!(state.overprint_mode, Some(1));
    assert_eq!(state.flatness, Some(50.0));
    assert_eq!(state.smoothness, Some(0.25));
    assert_eq!(state.stroke_adjustment, Some(true));
    assert_eq!(state.stroke_alpha, Some(0.8));
    assert_eq!(state.fill_alpha, Some(0.6));
    assert_eq!(state.alpha_is_shape, Some(true));
    assert_eq!(state.text_knockout, Some(false));
}

#[test]
fn leaves_missing_entries_unset() {
    let document = Document::new();
    let dict = state(vec![("Type", Value::Name("ExtGState".into()))]);

    let state = parse_ext_g_state(&document, &dict).expect("empty state should parse");

    assert_eq!(state.line_width, None);
    assert_eq!(state.dash_pattern, None);
    assert_eq!(state.font, None);
    assert_eq!(state.blend_mode, None);
    assert_eq!(state.soft_mask, None);
    assert!(state.other.is_empty());
}

#[test]
fn parses_dash_pattern_and_blend_modes() {
    let document = Document::new();
    let dict = state(vec![
        (
            "D",
            Value::Array(Array(vec![
                Value::Array(Array(vec![Value::Integer(2), Value::Integer(1)])),
                Value::Float(0.5),
            ])),
        ),
        (
            "BM",
            Value::Array(Array(vec![
                Value::Name("Normal".into()),
                Value::Name("Compatible".into()),
            ])),
        ),
    ]);

    let state = parse_ext_g_state(&document, &dict).expect("state should parse");

    assert_eq!(
        state.dash_pattern,
        Some(DashPattern {
            array: vec![2.0, 1.0],
            phase: 0.5,
        })
    );
    assert_eq!(
        state.blend_mode,
        Some(BlendMode::Array(vec![
            "Normal".to_string(),
            "Compatible".to_string(),
        ]))
    );
}

#[test]
fn parses_single_name_blend_mode() {
    let document = Document::new();
    let dict = state(vec![("BM", Value::Name("Multiply".into()))]);

    let state = parse_ext_g_state(&document, &dict).expect("state should parse");

    assert_eq!(state.blend_mode, Some(BlendMode::Name("Multiply".into())));
}

#[test]
fn parses_font_entry_without_resolving_font_reference() {
    let document = document_with_objects(vec![(
        7,
        Value::Dict(Dict::new().insert("Type", Value::Name("Font".into()))),
    )]);
    let dict = state(vec![(
        "Font",
        Value::Array(Array(vec![Value::Ref(7, 0), Value::Integer(12)])),
    )]);

    let state = parse_ext_g_state(&document, &dict).expect("state should parse");

    assert_eq!(
        state.font,
        Some(ExtGStateFont {
            font: Value::Ref(7, 0),
            size: 12.0,
        })
    );
}

#[test]
fn resolves_indirect_entries_and_dictionary() {
    let document = document_with_objects(vec![
        (3, Value::Float(0.4)),
        (
            5,
            Value::Array(Array(vec![Value::Integer(2), Value::Integer(1)])),
        ),
        (
            6,
            Value::Array(Array(vec![Value::Ref(5, 0), Value::Integer(0)])),
        ),
        (
            9,
            Value::Dict(
                Dict::new()
                    .insert("LW", Value::Integer(3))
                    .insert("ca", Value::Ref(3, 0))
                    .insert("D", Value::Ref(6, 0)),
            ),
        ),
    ]);

    let state = document
        .ext_g_state(&Value::Ref(9, 0))
        .expect("indirect dictionary should resolve");

    assert_eq!(state.line_width, Some(3.0));
    assert_eq!(state.fill_alpha, Some(0.4));
    assert_eq!(
        state.dash_pattern,
        Some(DashPattern {
            array: vec![2.0, 1.0],
            phase: 0.0,
        })
    );
}

#[test]
fn keeps_complex_and_unknown_entries() {
    let document = document_with_objects(vec![
        (
            8,
            Value::Dict(Dict::new().insert("S", Value::Name("Alpha".into()))),
        ),
        (9, Value::Integer(42)),
    ]);
    let dict = state(vec![
        ("SMask", Value::Ref(8, 0)),
        ("TR", Value::Name("Identity".into())),
        (
            "HT",
            Value::Dict(Dict::new().insert("Type", Value::Name("Halftone".into()))),
        ),
        ("XY", Value::Ref(9, 0)),
    ]);

    let state = parse_ext_g_state(&document, &dict).expect("state should parse");

    assert_eq!(
        state.soft_mask,
        Some(Value::Dict(
            Dict::new().insert("S", Value::Name("Alpha".into()))
        ))
    );
    assert_eq!(state.transfer, Some(Value::Name("Identity".into())));
    assert!(state.halftone.is_some());
    assert_eq!(state.other.get("XY"), Some(&Value::Ref(9, 0)));
}

#[test]
fn parses_ext_g_state_resource_dictionary() {
    let document = document_with_objects(vec![(
        11,
        Value::Dict(Dict::new().insert("ca", Value::Integer(1))),
    )]);
    let resources = Dict::new().insert(
        "ExtGState",
        Value::Dict(
            Dict::new()
                .insert(
                    "GS1",
                    Value::Dict(Dict::new().insert("LW", Value::Integer(1))),
                )
                .insert("GS2", Value::Ref(11, 0)),
        ),
    );

    let states = document
        .ext_g_states(&resources)
        .expect("resource dictionary should parse");

    assert_eq!(states.len(), 2);
    assert_eq!(states["GS1"].line_width, Some(1.0));
    assert_eq!(states["GS2"].fill_alpha, Some(1.0));
}

#[test]
fn returns_no_states_for_resources_without_ext_g_state() {
    let document = Document::new();
    let resources = Dict::new().insert("Font", Value::Dict(Dict::new()));

    let states = document
        .ext_g_states(&resources)
        .expect("absent /ExtGState should not fail");

    assert!(states.is_empty());
}

#[test]
fn rejects_non_dictionary_values() {
    let document = Document::new();

    let error = document
        .ext_g_state(&Value::Integer(1))
        .expect_err("must fail");
    assert!(
        matches!(&error, GraphicsStateError::Invalid(message) if message.contains("must be a dictionary")),
        "{error:?}"
    );

    let error = document
        .ext_g_state(&Value::Ref(42, 0))
        .expect_err("missing reference must fail");
    assert!(
        matches!(&error, GraphicsStateError::Invalid(message) if message.contains("unresolved object reference")),
        "{error:?}"
    );

    let resources = Dict::new().insert("ExtGState", Value::Integer(1));
    let error = document.ext_g_states(&resources).expect_err("must fail");
    assert!(
        matches!(&error, GraphicsStateError::Invalid(message) if message.contains("not a dictionary")),
        "{error:?}"
    );
}

#[test]
fn rejects_invalid_type_entry() {
    let document = Document::new();
    assert_invalid(
        &document,
        vec![("Type", Value::Name("Font".into()))],
        "/Type",
    );
}

#[test]
fn rejects_out_of_range_numbers() {
    let document = Document::new();
    for (key, value, needle) in [
        ("LW", Value::Integer(-1), "/LW"),
        ("ML", Value::Float(0.5), "/ML"),
        ("FL", Value::Integer(101), "/FL"),
        ("SM", Value::Float(-0.1), "/SM"),
        ("CA", Value::Float(1.5), "/CA"),
        ("ca", Value::Float(-0.1), "/ca"),
    ] {
        assert_invalid(&document, vec![(key, value)], needle);
    }
}

#[test]
fn rejects_out_of_range_codes() {
    let document = Document::new();
    for (key, value) in [
        ("LC", Value::Integer(3)),
        ("LJ", Value::Integer(-1)),
        ("OPM", Value::Integer(2)),
        ("LC", Value::Float(1.5)),
    ] {
        assert_invalid(&document, vec![(key, value)], &format!("/{key}"));
    }
}

#[test]
fn rejects_mismatched_value_types() {
    let document = Document::new();
    for (key, value) in [
        ("LW", Value::Name("thin".into())),
        ("OP", Value::Integer(1)),
        ("RI", Value::String("AbsoluteColorimetric".into())),
        ("BM", Value::Integer(1)),
        ("Font", Value::Dict(Dict::new())),
    ] {
        assert_invalid(&document, vec![(key, value)], &format!("/{key}"));
    }
}

#[test]
fn rejects_malformed_dash_patterns() {
    let document = Document::new();
    for dash in [
        Value::Integer(1),
        Value::Array(Array(vec![Value::Array(Array::new())])),
        Value::Array(Array(vec![
            Value::Array(Array(vec![Value::Integer(-1)])),
            Value::Integer(0),
        ])),
        Value::Array(Array(vec![
            Value::Array(Array(vec![Value::Integer(1)])),
            Value::Integer(-1),
        ])),
        Value::Array(Array(vec![
            Value::Array(Array(vec![Value::Name("dash".into())])),
            Value::Integer(0),
        ])),
    ] {
        assert_invalid(&document, vec![("D", dash)], "/D");
    }
}

#[test]
fn rejects_malformed_blend_mode_arrays() {
    let document = Document::new();
    for blend_mode in [
        Value::Array(Array::new()),
        Value::Array(Array(vec![Value::Integer(1)])),
    ] {
        assert_invalid(&document, vec![("BM", blend_mode)], "/BM");
    }
}

#[test]
fn rejects_malformed_font_entries() {
    let document = document_with_objects(vec![(9, Value::Integer(1))]);
    for font in [
        Value::Array(Array(vec![Value::Ref(9, 0), Value::Integer(12)])),
        Value::Array(Array(vec![Value::Dict(Dict::new()), Value::Integer(0)])),
        Value::Array(Array(vec![Value::Dict(Dict::new())])),
        Value::Array(Array(vec![
            Value::Dict(Dict::new()),
            Value::Name("large".into()),
        ])),
    ] {
        assert_invalid(&document, vec![("Font", font)], "/Font");
    }
}
