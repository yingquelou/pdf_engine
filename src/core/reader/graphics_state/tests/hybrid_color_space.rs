//! `/CS` hybrid colour space and `/CSS` style reader tests.
//!
//! Covers the two accepted shapes of `/CS` (bare name, two-element array
//! of names and/or finite numbers), the two accepted names of `/CSS`
//! (`/Knockout`, `/Native`), the indirect-reference resolution path, and
//! the *preserve-and-report* contract: an untypable value is preserved in
//! `ExtGState::other` under the same key and a matching
//! `GraphicsStateIssue` lands in `ExtGState::issues`, without failing the
//! rest of the dictionary.

use super::*;

#[test]
fn parses_hybrid_color_space_name_form() {
    let document = Document::new();
    let dict = state(vec![("CS", Value::Name("CalRGB".into()))]);

    let state = parse_ext_g_state(&document, &dict).expect("state should parse");

    assert_eq!(
        state.hybrid_color_space,
        Some(HybridColorSpace::Name("CalRGB".into()))
    );
    assert!(state.hybrid_color_space_style.is_none());
    assert!(state.issues.is_empty());
    assert!(state.other.is_empty());
}

#[test]
fn parses_hybrid_color_space_pair_of_names() {
    let document = Document::new();
    let dict = state(vec![(
        "CS",
        Value::Array(Array(vec![
            Value::Name("CalRGB".into()),
            Value::Name("DeviceCMYK".into()),
        ])),
    )]);

    let state = parse_ext_g_state(&document, &dict).expect("state should parse");

    assert_eq!(
        state.hybrid_color_space,
        Some(HybridColorSpace::Pair(
            HybridColorSpaceElement::Name("CalRGB".into()),
            HybridColorSpaceElement::Name("DeviceCMYK".into()),
        ))
    );
    assert!(state.issues.is_empty());
}

#[test]
fn parses_hybrid_color_space_pair_name_and_number() {
    let document = Document::new();
    let dict = state(vec![(
        "CS",
        Value::Array(Array(vec![
            Value::Name("CalGray".into()),
            Value::Float(0.25),
        ])),
    )]);

    let state = parse_ext_g_state(&document, &dict).expect("state should parse");

    assert_eq!(
        state.hybrid_color_space,
        Some(HybridColorSpace::Pair(
            HybridColorSpaceElement::Name("CalGray".into()),
            HybridColorSpaceElement::Number(0.25),
        ))
    );
}

#[test]
fn parses_hybrid_color_space_pair_indirect_elements() {
    let document = document_with_objects(vec![
        (11, Value::Name("CalRGB".into())),
        (12, Value::Integer(3)),
    ]);
    let dict = state(vec![(
        "CS",
        Value::Array(Array(vec![Value::Ref(11, 0), Value::Ref(12, 0)])),
    )]);

    let state = parse_ext_g_state(&document, &dict).expect("indirect pair should parse");

    assert_eq!(
        state.hybrid_color_space,
        Some(HybridColorSpace::Pair(
            HybridColorSpaceElement::Name("CalRGB".into()),
            HybridColorSpaceElement::Number(3.0),
        ))
    );
    assert!(state.issues.is_empty());
}

#[test]
fn parses_hybrid_color_space_style_knockout_and_native() {
    let document = Document::new();
    for (name, expected) in [
        ("Knockout", HybridColorSpaceStyle::Knockout),
        ("Native", HybridColorSpaceStyle::Native),
    ] {
        let dict = state(vec![
            ("CS", Value::Name("CalRGB".into())),
            ("CSS", Value::Name(name.into())),
        ]);
        let state = parse_ext_g_state(&document, &dict).expect("style should parse");
        assert_eq!(state.hybrid_color_space_style, Some(expected));
        assert!(state.issues.is_empty(), "issues: {:?}", state.issues);
    }
}

#[test]
fn preserves_malformed_cs_with_issue() {
    let document = Document::new();
    let dict = state(vec![("CS", Value::Integer(7))]);

    let state = parse_ext_g_state(&document, &dict).expect("malformed /CS must not hard-fail");

    assert_eq!(state.hybrid_color_space, None);
    assert_eq!(state.other.get("CS"), Some(&Value::Integer(7)));
    assert_eq!(state.issues.len(), 1);
    assert_eq!(state.issues[0].key, "CS");
    assert!(state.issues[0].message.contains("/CS"));
    assert!(state.issues[0].message.contains("name"));
}

#[test]
fn preserves_wrong_length_cs_array_with_issue() {
    let document = Document::new();
    let raw = Value::Array(Array(vec![Value::Name("CalRGB".into())]));
    let dict = state(vec![("CS", raw.clone())]);

    let state = parse_ext_g_state(&document, &dict).expect("wrong-length /CS must not hard-fail");

    assert_eq!(state.hybrid_color_space, None);
    assert_eq!(state.other.get("CS"), Some(&raw));
    assert_eq!(state.issues.len(), 1);
    assert!(state.issues[0].message.contains("two elements"));
}

#[test]
fn preserves_cs_array_with_invalid_element() {
    let document = Document::new();
    let raw = Value::Array(Array(vec![Value::Name("CalRGB".into()), Value::Bool(true)]));
    let dict = state(vec![("CS", raw.clone())]);

    let state = parse_ext_g_state(&document, &dict).expect("invalid element must not hard-fail");

    assert_eq!(state.hybrid_color_space, None);
    assert_eq!(state.other.get("CS"), Some(&raw));
    assert_eq!(state.issues.len(), 1);
    assert!(state.issues[0].message.contains("finite number"));
}

#[test]
fn preserves_unknown_cs_style_with_issue() {
    let document = Document::new();
    let dict = state(vec![
        ("CS", Value::Name("CalRGB".into())),
        ("CSS", Value::Name("Surrender".into())),
    ]);

    let state = parse_ext_g_state(&document, &dict).expect("unknown /CSS must not hard-fail");

    assert_eq!(
        state.hybrid_color_space,
        Some(HybridColorSpace::Name("CalRGB".into()))
    );
    assert_eq!(state.hybrid_color_space_style, None);
    assert_eq!(
        state.other.get("CSS"),
        Some(&Value::Name("Surrender".into()))
    );
    assert_eq!(state.issues.len(), 1);
    assert_eq!(state.issues[0].key, "CSS");
    assert!(state.issues[0].message.contains("Knockout"));
    assert!(state.issues[0].message.contains("Native"));
}

#[test]
fn preserves_non_name_cs_style_with_issue() {
    let document = Document::new();
    let dict = state(vec![("CSS", Value::Integer(1))]);

    let state = parse_ext_g_state(&document, &dict).expect("non-name /CSS must not hard-fail");

    assert_eq!(state.hybrid_color_space_style, None);
    assert_eq!(state.other.get("CSS"), Some(&Value::Integer(1)));
    assert_eq!(state.issues.len(), 1);
    assert_eq!(state.issues[0].key, "CSS");
}

#[test]
fn parses_hybrid_entries_with_other_standard_entries() {
    let document = Document::new();
    let dict = state(vec![
        ("Type", Value::Name("ExtGState".into())),
        ("LW", Value::Float(1.5)),
        ("CS", Value::Name("CalRGB".into())),
        ("CSS", Value::Name("Native".into())),
        ("BM", Value::Name("Multiply".into())),
    ]);

    let state = parse_ext_g_state(&document, &dict).expect("combined state should parse");

    assert_eq!(state.line_width, Some(1.5));
    assert_eq!(state.blend_mode, Some(BlendMode::Name("Multiply".into())));
    assert_eq!(
        state.hybrid_color_space,
        Some(HybridColorSpace::Name("CalRGB".into()))
    );
    assert_eq!(
        state.hybrid_color_space_style,
        Some(HybridColorSpaceStyle::Native)
    );
    assert!(state.issues.is_empty());
    assert!(state.other.is_empty());
}

#[test]
fn cs_and_css_are_excluded_from_other_when_typed() {
    // Regression: before `/CS`/`/CSS` joined `STANDARD_ENTRIES`, the raw
    // values leaked into `other` alongside the typed view. Adding them to
    // `STANDARD_ENTRIES` means a well-typed value appears ONLY as a typed
    // field, not also under `other`.
    let document = Document::new();
    let dict = state(vec![
        ("CS", Value::Name("CalRGB".into())),
        ("CSS", Value::Name("Knockout".into())),
    ]);

    let state = parse_ext_g_state(&document, &dict).expect("combined state should parse");

    assert!(
        !state.other.contains_key("CS"),
        "typed /CS must not also appear in `other`",
    );
    assert!(!state.other.contains_key("CSS"));
}
