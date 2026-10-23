use super::super::Type1PathSegment;
use super::super::tests::{numbers, op, program};

fn call_other_subr(data: &mut Vec<u8>, argument_count: i32, procedure: i32) {
    numbers(data, &[argument_count, procedure]);
    op(data, 12);
    op(data, 16);
}

fn pop(data: &mut Vec<u8>) {
    op(data, 12);
    op(data, 17);
}

fn flex_epilogue(data: &mut Vec<u8>) {
    pop(data);
    pop(data);
    op(data, 12);
    op(data, 33);
}

#[derive(Clone, Copy)]
enum FlexPoint {
    Move(i32, i32),
    Horizontal(i32),
    Vertical(i32),
}

impl FlexPoint {
    fn push(self, data: &mut Vec<u8>) {
        match self {
            Self::Move(x, y) => {
                numbers(data, &[x, y]);
                op(data, 21);
            }
            Self::Horizontal(x) => {
                numbers(data, &[x]);
                op(data, 22);
            }
            Self::Vertical(y) => {
                numbers(data, &[y]);
                op(data, 4);
            }
        }
    }
}

fn flex_charstring(points: [FlexPoint; 7], final_point: [i32; 2], flex_depth: i32) -> Vec<u8> {
    let mut data = Vec::new();
    numbers(&mut data, &[0, 500]);
    op(&mut data, 13);
    numbers(&mut data, &[100, 200]);
    op(&mut data, 21);
    call_other_subr(&mut data, 0, 1);
    for point in points {
        point.push(&mut data);
    }
    numbers(&mut data, &[flex_depth, final_point[0], final_point[1]]);
    call_other_subr(&mut data, 3, 0);
    data
}

fn diagonal_points() -> [FlexPoint; 7] {
    [
        FlexPoint::Move(10, 20),
        FlexPoint::Move(30, 40),
        FlexPoint::Move(50, 60),
        FlexPoint::Move(70, 80),
        FlexPoint::Move(90, 100),
        FlexPoint::Move(110, 120),
        FlexPoint::Move(130, 140),
    ]
}

#[test]
fn expands_flex_into_two_curves_from_seven_points() {
    let mut data = flex_charstring(diagonal_points(), [590, 760], 50);
    flex_epilogue(&mut data);
    op(&mut data, 14);

    let program = program(&[("A", data)], &[]);
    let glyph = program.glyph("A").unwrap().unwrap();
    assert_eq!(glyph.width, 500.0);
    assert_eq!(
        glyph.path,
        vec![
            Type1PathSegment::MoveTo { x: 100.0, y: 200.0 },
            Type1PathSegment::CurveTo {
                control1: [140.0, 260.0],
                control2: [190.0, 320.0],
                to: [260.0, 400.0],
            },
            Type1PathSegment::CurveTo {
                control1: [350.0, 500.0],
                control2: [460.0, 620.0],
                to: [590.0, 760.0],
            },
        ]
    );
}

#[test]
fn flex_accepts_horizontal_and_vertical_points_and_keeps_current_point() {
    let mut data = flex_charstring(
        [
            FlexPoint::Vertical(20),
            FlexPoint::Horizontal(30),
            FlexPoint::Move(50, 60),
            FlexPoint::Vertical(80),
            FlexPoint::Move(90, 100),
            FlexPoint::Horizontal(110),
            FlexPoint::Move(130, 140),
        ],
        [510, 600],
        0,
    );
    flex_epilogue(&mut data);
    numbers(&mut data, &[10, 0]);
    op(&mut data, 5);
    op(&mut data, 14);

    let program = program(&[("A", data)], &[]);
    let glyph = program.glyph("A").unwrap().unwrap();
    assert_eq!(
        glyph.path,
        vec![
            Type1PathSegment::MoveTo { x: 100.0, y: 200.0 },
            Type1PathSegment::CurveTo {
                control1: [130.0, 220.0],
                control2: [180.0, 280.0],
                to: [180.0, 360.0],
            },
            Type1PathSegment::CurveTo {
                control1: [270.0, 460.0],
                control2: [380.0, 460.0],
                to: [510.0, 600.0],
            },
            Type1PathSegment::LineTo { x: 520.0, y: 600.0 },
        ]
    );
}

#[test]
fn flex_state_and_points_survive_subroutine_calls() {
    let mut subr = Vec::new();
    for point in diagonal_points() {
        point.push(&mut subr);
    }
    numbers(&mut subr, &[50, 590, 760]);
    call_other_subr(&mut subr, 3, 0);
    flex_epilogue(&mut subr);
    op(&mut subr, 11);

    let mut data = Vec::new();
    numbers(&mut data, &[0, 500]);
    op(&mut data, 13);
    numbers(&mut data, &[100, 200]);
    op(&mut data, 21);
    call_other_subr(&mut data, 0, 1);
    numbers(&mut data, &[0]);
    op(&mut data, 10);
    op(&mut data, 14);

    let program = program(&[("A", data)], &[(0, subr)]);
    let glyph = program.glyph("A").unwrap().unwrap();
    assert_eq!(
        glyph.path,
        vec![
            Type1PathSegment::MoveTo { x: 100.0, y: 200.0 },
            Type1PathSegment::CurveTo {
                control1: [140.0, 260.0],
                control2: [190.0, 320.0],
                to: [260.0, 400.0],
            },
            Type1PathSegment::CurveTo {
                control1: [350.0, 500.0],
                control2: [460.0, 620.0],
                to: [590.0, 760.0],
            },
        ]
    );
}

#[test]
fn rejects_malformed_flex_sequences() {
    let mut exit_without_entry = Vec::new();
    numbers(&mut exit_without_entry, &[0, 500]);
    op(&mut exit_without_entry, 13);
    numbers(&mut exit_without_entry, &[50, 500, 600]);
    call_other_subr(&mut exit_without_entry, 3, 0);
    op(&mut exit_without_entry, 14);

    let mut double_entry = Vec::new();
    numbers(&mut double_entry, &[0, 500]);
    op(&mut double_entry, 13);
    call_other_subr(&mut double_entry, 0, 1);
    call_other_subr(&mut double_entry, 0, 1);
    op(&mut double_entry, 14);

    let mut missing_operands = Vec::new();
    numbers(&mut missing_operands, &[0, 500]);
    op(&mut missing_operands, 13);
    call_other_subr(&mut missing_operands, 0, 1);
    call_other_subr(&mut missing_operands, 3, 0);
    op(&mut missing_operands, 14);

    let mut missing_exit = Vec::new();
    numbers(&mut missing_exit, &[0, 500]);
    op(&mut missing_exit, 13);
    call_other_subr(&mut missing_exit, 0, 1);
    op(&mut missing_exit, 14);

    let mut missing_pops = flex_charstring(diagonal_points(), [590, 760], 50);
    op(&mut missing_pops, 14);

    let program = program(
        &[
            ("exit", exit_without_entry),
            ("double", double_entry),
            ("few", missing_operands),
            ("no-exit", missing_exit),
            ("no-pops", missing_pops),
        ],
        &[],
    );
    let error = program.glyph("exit").unwrap_err();
    assert!(error.to_string().contains("preceding"), "{error}");
    let error = program.glyph("double").unwrap_err();
    assert!(error.to_string().contains("already active"), "{error}");
    let error = program.glyph("few").unwrap_err();
    assert!(error.to_string().contains("operands"), "{error}");
    let error = program.glyph("no-exit").unwrap_err();
    assert!(error.to_string().contains("exit call"), "{error}");
    let error = program.glyph("no-pops").unwrap_err();
    assert!(error.to_string().contains("not consumed"), "{error}");
}

#[test]
fn rejects_unsupported_other_subrs_and_stray_pop() {
    let mut hint_replacement = Vec::new();
    numbers(&mut hint_replacement, &[0, 500]);
    op(&mut hint_replacement, 13);
    call_other_subr(&mut hint_replacement, 1, 3);
    op(&mut hint_replacement, 14);

    let mut unknown_procedure = Vec::new();
    numbers(&mut unknown_procedure, &[0, 500]);
    op(&mut unknown_procedure, 13);
    call_other_subr(&mut unknown_procedure, 0, 9);
    op(&mut unknown_procedure, 14);

    let mut stray_pop = Vec::new();
    numbers(&mut stray_pop, &[0, 500]);
    op(&mut stray_pop, 13);
    pop(&mut stray_pop);
    op(&mut stray_pop, 14);

    let mut short_rmoveto = Vec::new();
    numbers(&mut short_rmoveto, &[0, 500]);
    op(&mut short_rmoveto, 13);
    call_other_subr(&mut short_rmoveto, 0, 1);
    numbers(&mut short_rmoveto, &[10]);
    op(&mut short_rmoveto, 21);
    op(&mut short_rmoveto, 14);

    let program = program(
        &[
            ("hint", hint_replacement),
            ("unknown", unknown_procedure),
            ("pop", stray_pop),
            ("short", short_rmoveto),
        ],
        &[],
    );
    let error = program.glyph("hint").unwrap_err();
    assert!(error.to_string().contains("hint replacement"), "{error}");
    let error = program.glyph("unknown").unwrap_err();
    assert!(error.to_string().contains("unsupported"), "{error}");
    let error = program.glyph("pop").unwrap_err();
    assert!(error.to_string().contains("pending"), "{error}");
    let error = program.glyph("short").unwrap_err();
    assert!(
        error.to_string().contains("flex rmoveto requires 2"),
        "{error}"
    );
}
