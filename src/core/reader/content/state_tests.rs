//! Graphics state machine tests (C3).

use super::*;
use crate::core::types::{Array, Dict, Document, Object, StreamData, Trailer, Value};

/// Run a synthetic content stream through the runner with no ExtGState
/// resources available.
fn trace_ops(input: &[u8]) -> Result<Vec<ContentGraphicsState>, ContentStateError> {
    let ops = parse_content_stream(input).expect("content stream parses");
    let mut resolver = |_name: &str| Ok(None);
    run_content_state(&ops, &mut resolver)
}

fn states_with_ext_g_states(content: &[u8], ext: Dict) -> (Document, Vec<ContentGraphicsState>) {
    let mut doc = Document::new();
    doc.set_object(Object::new(
        1,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Catalog".into()))
                .insert("Pages", Value::Ref(2, 0)),
        ),
    ));
    doc.set_object(Object::new(
        2,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Pages".into()))
                .insert("Kids", Value::Array(Array::new().push(Value::Ref(3, 0))))
                .insert("Count", Value::Integer(1)),
        ),
    ));
    doc.set_object(Object::new(
        3,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Page".into()))
                .insert("Parent", Value::Ref(2, 0))
                .insert(
                    "Resources",
                    Value::Dict(Dict::new().insert("ExtGState", Value::Dict(ext))),
                )
                .insert("Contents", Value::Ref(4, 0)),
        ),
    ));
    doc.set_object(Object::new(
        4,
        0,
        Value::Stream(Dict::new(), StreamData::Owned(content.to_vec())),
    ));
    doc.trailer = Trailer {
        dict: Dict::new()
            .insert("Root", Value::Ref(1, 0))
            .insert("Size", Value::Integer(5)),
    };
    let trace = doc
        .page_content_state_trace(0)
        .expect("state trace succeeds");
    (doc, trace)
}

#[test]
fn q_q_stack_restores_ctm_and_line_width() {
    let trace = trace_ops(b"1 w q 2 w 2 0 0 2 0 0 cm Q").unwrap();
    assert_eq!(trace.len(), 5);
    assert_eq!(trace[0].line_width, 1.0);
    // Inside the saved section both parameters diverge from the initial state.
    assert_eq!(trace[2].line_width, 2.0);
    assert_eq!(trace[3].ctm, [2.0, 0.0, 0.0, 2.0, 0.0, 0.0]);
    // Q restores line width and CTM together.
    assert_eq!(trace[4].line_width, 1.0);
    assert_eq!(trace[4].ctm, IDENTITY_MATRIX);
}

#[test]
fn q_stack_underflow_has_no_effect() {
    let trace = trace_ops(b"2 w Q Q").unwrap();
    // ISO 32000-1 Table 56: Q with an empty stack does nothing, no error.
    assert_eq!(trace[1].line_width, 2.0);
    assert_eq!(trace[2].line_width, 2.0);
}

#[test]
fn cm_concatenates_matrices_in_prepend_order() {
    let trace = trace_ops(b"2 0 0 2 0 0 cm 1 0 0 1 10 20 cm").unwrap();
    // The translate applies before the existing scale: (10,20) → (20,40).
    assert_eq!(trace[1].ctm, [2.0, 0.0, 0.0, 2.0, 20.0, 40.0]);
}

#[test]
fn save_stack_depth_limit_returns_error() {
    let mut input = Vec::new();
    for _ in 0..MAX_SAVE_DEPTH + 10 {
        input.extend_from_slice(b"q ");
    }
    let err = trace_ops(&input).unwrap_err();
    assert_eq!(
        err,
        ContentStateError::SaveStackTooDeep {
            limit: MAX_SAVE_DEPTH
        }
    );
}

#[test]
fn gs_applies_ext_g_state_parameters() {
    let ext = Dict::new().insert(
        "GS1",
        Value::Dict(
            Dict::new()
                .insert("LW", Value::Float(4.0))
                .insert("CA", Value::Float(0.5))
                .insert("ca", Value::Float(0.25))
                .insert("BM", Value::Name("Multiply".into()))
                .insert("OP", Value::Bool(true)),
        ),
    );
    let (_doc, trace) = states_with_ext_g_states(b"/GS1 gs", ext);
    let state = &trace[0];
    assert_eq!(state.line_width, 4.0);
    assert_eq!(state.stroke_alpha, 0.5);
    assert_eq!(state.fill_alpha, 0.25);
    assert_eq!(state.blend_mode, vec!["Multiply".to_string()]);
    assert!(state.stroke_overprint);
}

#[test]
fn unknown_gs_name_returns_typed_error() {
    let mut doc = Document::new();
    doc.set_object(Object::new(
        1,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Catalog".into()))
                .insert("Pages", Value::Ref(2, 0)),
        ),
    ));
    doc.set_object(Object::new(
        2,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Pages".into()))
                .insert("Kids", Value::Array(Array::new().push(Value::Ref(3, 0))))
                .insert("Count", Value::Integer(1)),
        ),
    ));
    doc.set_object(Object::new(
        3,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Page".into()))
                .insert("Parent", Value::Ref(2, 0))
                .insert(
                    "Contents",
                    Value::Stream(Dict::new(), StreamData::Owned(b"/GSX gs".to_vec())),
                ),
        ),
    ));
    doc.trailer = Trailer {
        dict: Dict::new()
            .insert("Root", Value::Ref(1, 0))
            .insert("Size", Value::Integer(4)),
    };
    let err = doc.page_content_state_trace(0).unwrap_err();
    assert_eq!(
        err,
        ContentStateError::UnknownExtGState { name: "GSX".into() }
    );
}

#[test]
fn gs_font_name_entry_sets_text_font() {
    let ext = Dict::new().insert(
        "GS1",
        Value::Dict(
            Dict::new().insert(
                "Font",
                Value::Array(
                    Array::new()
                        .push(Value::Name("F1".into()))
                        .push(Value::Float(10.0)),
                ),
            ),
        ),
    );
    let (_doc, trace) = states_with_ext_g_states(b"/GS1 gs", ext);
    assert_eq!(trace[0].font, Some(("F1".into(), 10.0)));
}

#[test]
fn invalid_state_parameters_return_typed_errors() {
    let cases: Vec<(&[u8], &str)> = vec![
        (b"5 J", "J"),
        (b"-1 w", "w"),
        (b"0.5 M", "M"),
        (b"[3 -1] 0 d", "d"),
        (b"/Bogus ri", "ri"),
        (b"4 Tr", "Tr"),
        (b"101 i", "i"),
        (b"/F1 0 Tf", "Tf"),
    ];
    for (input, operator) in cases {
        let err = trace_ops(input).expect_err("invalid parameter should error");
        match &err {
            ContentStateError::InvalidValue { operator: got, .. } => {
                assert_eq!(got, operator, "for input {input:?}: {err}");
            }
            other => panic!("expected InvalidValue for {input:?}, got {other:?}"),
        }
    }
}

#[test]
fn non_finite_matrix_returns_typed_error() {
    // A parsed tokenizer cannot produce an infinite literal, so exercise the
    // runner validation directly through the operator AST.
    let ops = vec![Op::ConcatMatrix([f64::INFINITY, 0.0, 0.0, 1.0, 0.0, 0.0])];
    let mut resolver = |_name: &str| Ok(None);
    let err = run_content_state(&ops, &mut resolver).unwrap_err();
    assert!(
        matches!(&err, ContentStateError::InvalidValue { operator, .. } if operator == "cm"),
        "{err}"
    );
}

#[test]
fn bt_resets_text_matrices_keeps_text_state() {
    let trace = trace_ops(b"BT 1 0 0 1 50 60 Tm /F1 12 Tf ET BT 0 0 Td").unwrap();
    // Tm inside a text object sets both matrices.
    assert_eq!(trace[1].text_matrix, [1.0, 0.0, 0.0, 1.0, 50.0, 60.0]);
    assert_eq!(trace[1].text_line_matrix, [1.0, 0.0, 0.0, 1.0, 50.0, 60.0]);
    // The second BT resets matrices but keeps the font selection.
    assert_eq!(trace[4].text_matrix, IDENTITY_MATRIX);
    assert_eq!(trace[4].text_line_matrix, IDENTITY_MATRIX);
    assert_eq!(trace[4].font, Some(("F1".into(), 12.0)));
    // Td after BT moves from identity by (0,0).
    assert_eq!(trace[5].text_matrix, IDENTITY_MATRIX);
}

#[test]
fn td_td_and_t_star_update_matrices() {
    let trace = trace_ops(b"BT 12 -20 Td 0 -30 TD 5 TL T*").unwrap();
    // Td: line matrix translates by (12,-20).
    assert_eq!(trace[1].text_matrix, [1.0, 0.0, 0.0, 1.0, 12.0, -20.0]);
    // TD also sets leading to -ty = 30, then translates by (0,-30).
    assert_eq!(trace[2].leading, 30.0);
    assert_eq!(trace[2].text_matrix, [1.0, 0.0, 0.0, 1.0, 12.0, -50.0]);
    // T* translates by (0, -TL) = (0, -5).
    assert_eq!(trace[4].text_matrix, [1.0, 0.0, 0.0, 1.0, 12.0, -55.0]);
}

#[test]
fn tick_and_quote_show_text_move_to_next_line() {
    let trace = trace_ops(b"BT 20 TL (a) ' 5 10 (b) \"").unwrap();
    // ' moves by (0,-TL) = (0,-20).
    assert_eq!(trace[2].text_matrix, [1.0, 0.0, 0.0, 1.0, 0.0, -20.0]);
    // " sets word/char spacing then moves another (0,-20).
    assert_eq!(trace[3].word_spacing, 5.0);
    assert_eq!(trace[3].char_spacing, 10.0);
    assert_eq!(trace[3].text_matrix, [1.0, 0.0, 0.0, 1.0, 0.0, -40.0]);
}

#[test]
fn color_operators_track_current_values() {
    let trace =
        trace_ops(b"/DeviceRGB cs 0.2 0.4 0.6 scn 0.5 g 1 0 0 rg /DeviceCMYK CS 0 0 0 1 SCN")
            .unwrap();
    assert_eq!(trace[0].fill_color.space.as_deref(), Some("DeviceRGB"));
    assert_eq!(trace[1].fill_color.components, vec![0.2, 0.4, 0.6]);
    // Shorthand g switches the fill space to DeviceGray.
    assert_eq!(trace[2].fill_color.space.as_deref(), Some("DeviceGray"));
    assert_eq!(trace[2].fill_color.components, vec![0.5]);
    assert_eq!(trace[3].fill_color.components, vec![1.0, 0.0, 0.0]);
    assert_eq!(trace[4].stroke_color.space.as_deref(), Some("DeviceCMYK"));
    assert_eq!(trace[5].stroke_color.components, vec![0.0, 0.0, 0.0, 1.0]);
}

#[test]
fn named_pattern_color_operators_are_modeled() {
    let content = b"/Pattern CS /Pattern cs 0.2 0.5 /P2 SCN /P1 scn";
    let ops = parse_content_stream(content).unwrap();
    assert_eq!(
        ops[2],
        Op::SetStrokeColorPatternWithComponents(vec![0.2, 0.5], "P2".into())
    );
    assert_eq!(ops[3], Op::SetFillColorPattern("P1".into()));

    let trace = trace_ops(content).unwrap();
    assert_eq!(trace[2].stroke_color.space.as_deref(), Some("Pattern"));
    assert_eq!(trace[2].stroke_color.pattern.as_deref(), Some("P2"));
    assert_eq!(trace[2].stroke_color.components, vec![0.2, 0.5]);
    assert_eq!(trace[3].fill_color.pattern.as_deref(), Some("P1"));
    assert!(trace[3].fill_color.components.is_empty());
}

#[test]
fn out_of_range_device_color_components_return_typed_errors() {
    let err = trace_ops(b"2 g").unwrap_err();
    assert!(matches!(err, ContentStateError::InvalidValue { operator, .. } if operator == "g"));
    let err = trace_ops(b"1 2 3 rg").unwrap_err();
    assert!(matches!(err, ContentStateError::InvalidValue { operator, .. } if operator == "rg"));
}
