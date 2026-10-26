//! Content stream parser unit tests.

use super::Op;
use super::parse_content_stream;

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
