//! Content stream parser unit tests.

use super::Op;
use super::parse_content_stream;
use super::{ContentStreamError, TextElement};

#[test]
fn parse_graphics_state_operators() {
    let input = b"q 1 0 0 1 100 200 cm Q";
    let ops = parse_content_stream(input).unwrap();
    assert_eq!(ops.len(), 3);
    assert!(matches!(ops[0], Op::SaveState));
    assert!(matches!(
        ops[1],
        Op::ConcatMatrix([1.0, 0.0, 0.0, 1.0, 100.0, 200.0])
    ));
    assert!(matches!(ops[2], Op::RestoreState));
}

#[test]
fn parse_path_construction_operators() {
    let input = b"100 200 m 300 400 l h";
    let ops = parse_content_stream(input).unwrap();
    assert_eq!(ops.len(), 3);
    assert!(matches!(ops[0], Op::MoveTo(100.0, 200.0)));
    assert!(matches!(ops[1], Op::LineTo(300.0, 400.0)));
    assert!(matches!(ops[2], Op::CloseSubpath));
}

#[test]
fn parse_path_painting_operators() {
    let input = b"S s f f* B B* b b* n";
    let ops = parse_content_stream(input).unwrap();
    assert_eq!(ops.len(), 9);
    assert!(matches!(ops[0], Op::Stroke));
    assert!(matches!(ops[1], Op::CloseAndStroke));
    assert!(matches!(ops[2], Op::Fill));
    assert!(matches!(ops[3], Op::FillEvenOdd));
    assert!(matches!(ops[4], Op::FillAndStroke));
    assert!(matches!(ops[5], Op::FillAndStrokeEvenOdd));
    assert!(matches!(ops[6], Op::CloseFillAndStroke));
    assert!(matches!(ops[7], Op::CloseFillAndStrokeEvenOdd));
    assert!(matches!(ops[8], Op::EndPath));
}

#[test]
fn parse_text_object_operators() {
    let input = b"BT /F1 12 Tf 100 200 Td (Hello) Tj ET";
    let ops = parse_content_stream(input).unwrap();
    assert_eq!(ops.len(), 5);
    assert!(matches!(ops[0], Op::BeginText));
    assert!(matches!(ops[1], Op::SetFont(ref name, 12.0) if name == "F1"));
    assert!(matches!(ops[2], Op::MoveTextPosition(100.0, 200.0)));
    assert!(matches!(ops[3], Op::ShowText(_)));
    assert!(matches!(ops[4], Op::EndText));
}

#[test]
fn parse_color_operators() {
    let input = b"1 0 0 RG 0 1 0 rg 0.5 G 0.3 g";
    let ops = parse_content_stream(input).unwrap();
    assert_eq!(ops.len(), 4);
    assert!(matches!(ops[0], Op::SetStrokeRgb(1.0, 0.0, 0.0)));
    assert!(matches!(ops[1], Op::SetFillRgb(0.0, 1.0, 0.0)));
    assert!(matches!(ops[2], Op::SetStrokeGray(0.5)));
    assert!(matches!(ops[3], Op::SetFillGray(0.3)));
}

#[test]
fn parse_unmodeled_operator() {
    let input = b"1 2 3 UnknownOp";
    let ops = parse_content_stream(input).unwrap();
    assert_eq!(ops.len(), 1);
    match &ops[0] {
        Op::Unmodeled { operator, operands } => {
            assert_eq!(operator, "UnknownOp");
            assert_eq!(operands.len(), 3);
        }
        _ => panic!("expected Unmodeled operator"),
    }
}

#[test]
fn parse_empty_stream() {
    let input = b"";
    let ops = parse_content_stream(input).unwrap();
    assert_eq!(ops.len(), 0);
}

#[test]
fn parse_comments_and_whitespace() {
    let input = b"% This is a comment\nq % Another comment\nQ";
    let ops = parse_content_stream(input).unwrap();
    assert_eq!(ops.len(), 2);
    assert!(matches!(ops[0], Op::SaveState));
    assert!(matches!(ops[1], Op::RestoreState));
}

#[test]
fn parse_tj_array_operand() {
    let input = b"BT [ (Hello) -250 (World) ] TJ ET";
    let ops = parse_content_stream(input).unwrap();
    assert_eq!(ops.len(), 3);
    match &ops[1] {
        Op::ShowTextWithPositioning(elements) => {
            assert_eq!(
                *elements,
                vec![
                    TextElement::String(b"Hello".to_vec()),
                    TextElement::Adjustment(-250.0),
                    TextElement::String(b"World".to_vec()),
                ]
            );
        }
        other => panic!("expected ShowTextWithPositioning, got {other:?}"),
    }
}

#[test]
fn parse_dash_pattern_array_operand() {
    let input = b"[3 2 1] 0 d";
    let ops = parse_content_stream(input).unwrap();
    assert_eq!(ops.len(), 1);
    match &ops[0] {
        Op::SetDashPattern(dash, phase) => {
            assert_eq!(dash, &vec![3.0, 2.0, 1.0]);
            assert_eq!(*phase, 0.0);
        }
        other => panic!("expected SetDashPattern, got {other:?}"),
    }
}

#[test]
fn parse_empty_dash_pattern_array_operand() {
    let input = b"[] 0 d";
    let ops = parse_content_stream(input).unwrap();
    assert!(matches!(
        &ops[0],
        Op::SetDashPattern(dash, 0.0) if dash.is_empty()
    ));
}

#[test]
fn tj_array_with_hex_string_elements() {
    let input = b"[<48656C6C6F> -100] TJ";
    let ops = parse_content_stream(input).unwrap();
    match &ops[0] {
        Op::ShowTextWithPositioning(elements) => {
            assert_eq!(
                *elements,
                vec![
                    TextElement::String(b"Hello".to_vec()),
                    TextElement::Adjustment(-100.0),
                ]
            );
        }
        other => panic!("expected ShowTextWithPositioning, got {other:?}"),
    }
}

#[test]
fn tj_rejects_non_array_operand() {
    let input = b"(literal) TJ";
    let err = parse_content_stream(input).unwrap_err();
    assert_eq!(
        err,
        ContentStreamError::InvalidOperandValue {
            operator: "TJ".into(),
            operand_index: 0,
            message: "expected array".into(),
        }
    );
}

#[test]
fn tj_rejects_unterminated_array() {
    let input = b"[ (a) -1 TJ";
    let err = parse_content_stream(input).unwrap_err();
    assert_eq!(
        err,
        ContentStreamError::InvalidOperandValue {
            operator: "TJ".into(),
            operand_index: 0,
            message: "unterminated array".into(),
        }
    );
}

#[test]
fn tj_rejects_name_element_in_array() {
    let input = b"[ /NotText ] TJ";
    let err = parse_content_stream(input).unwrap_err();
    assert_eq!(
        err,
        ContentStreamError::InvalidOperandValue {
            operator: "TJ".into(),
            operand_index: 0,
            message: "expected string or number in text array".into(),
        }
    );
}

#[test]
fn d_rejects_non_numeric_array_element() {
    let input = b"[3 (x)] 0 d";
    let err = parse_content_stream(input).unwrap_err();
    assert_eq!(
        err,
        ContentStreamError::InvalidOperandValue {
            operator: "d".into(),
            operand_index: 1,
            message: "expected number".into(),
        }
    );
}

#[test]
fn d_rejects_missing_phase_operand() {
    let input = b"[3 2] d";
    let err = parse_content_stream(input).unwrap_err();
    assert_eq!(
        err,
        ContentStreamError::InvalidOperandCount {
            operator: "d".into(),
            expected: "2".into(),
            actual: 4,
        }
    );
}

// ── Inline dictionary operands (C4) ───────────────────────

use super::Value;

#[test]
fn bdc_keeps_inline_dictionary_property_list() {
    let input = b"/Span <</ActualText (hi) /OC /MC0 /Lang /EN>> BDC EMC";
    let ops = parse_content_stream(input).unwrap();
    let Op::BeginMarkedContentWithProperties(tag, Value::Dict(dict)) = &ops[0] else {
        panic!("expected BDC with inline dictionary, got {:?}", ops[0]);
    };
    assert_eq!(tag, "Span");
    match dict.get("ActualText") {
        Some(Value::String(text)) => assert_eq!(text, "hi"),
        other => panic!("ActualText stored as {other:?}"),
    }
    assert_eq!(dict.get("OC"), Some(&Value::Name("MC0".into())));
    assert_eq!(dict.get("Lang"), Some(&Value::Name("EN".into())));
}

#[test]
fn bdc_property_dictionary_supports_nested_dict_array_and_reference() {
    let input = b"/P <</Prop <</A true>> /B [1 0 R /N] /R 7 0 R>> BDC EMC";
    let ops = parse_content_stream(input).unwrap();
    let Op::BeginMarkedContentWithProperties(_, Value::Dict(dict)) = &ops[0] else {
        panic!("expected BDC with inline dictionary, got {:?}", ops[0]);
    };
    match dict.get("Prop") {
        Some(Value::Dict(inner)) => assert_eq!(inner.get("A"), Some(&Value::Bool(true))),
        other => panic!("Prop stored as {other:?}"),
    }
    match dict.get("B") {
        Some(Value::Array(items)) => {
            assert_eq!(items.0[0], Value::Ref(1, 0));
            assert_eq!(items.0[1], Value::Name("N".into()));
        }
        other => panic!("B stored as {other:?}"),
    }
    assert_eq!(dict.get("R"), Some(&Value::Ref(7, 0)));
}

#[test]
fn dp_accepts_property_name_or_dictionary() {
    let ops = parse_content_stream(b"/Table /Props DP /Fig <</Alt (f)>> DP").unwrap();
    assert_eq!(
        ops[0],
        Op::MarkedContentPointWithProperties("Table".into(), Value::Name("Props".into()))
    );
    let Op::MarkedContentPointWithProperties(_, Value::Dict(dict)) = &ops[1] else {
        panic!("expected DP with inline dictionary, got {:?}", ops[1]);
    };
    assert_eq!(dict.get("Alt"), Some(&Value::String("f".into())));
}

#[test]
fn hex_string_operand_in_dictionary_keeps_hex_characters() {
    let ops = parse_content_stream(b"/P <</ActualText <FEFF0041>>> BDC EMC").unwrap();
    let Op::BeginMarkedContentWithProperties(_, Value::Dict(dict)) = &ops[0] else {
        panic!("expected BDC with inline dictionary, got {:?}", ops[0]);
    };
    assert_eq!(
        dict.get("ActualText"),
        Some(&Value::HexString("FEFF0041".into()))
    );
}

#[test]
fn dictionary_with_string_containing_angle_brackets_roundtrips_entries() {
    // `>` inside a literal string must not terminate the dictionary.
    let ops = parse_content_stream(b"/P <</ActualText (a>b) /E (c)>> BDC EMC").unwrap();
    let Op::BeginMarkedContentWithProperties(_, Value::Dict(dict)) = &ops[0] else {
        panic!("expected BDC with inline dictionary, got {:?}", ops[0]);
    };
    assert_eq!(dict.get("ActualText"), Some(&Value::String("a>b".into())));
    assert_eq!(dict.get("E"), Some(&Value::String("c".into())));
}
