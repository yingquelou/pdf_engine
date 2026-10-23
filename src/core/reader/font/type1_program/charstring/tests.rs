use super::super::{Type1FontProgram, Type1Header};
use super::*;

fn push_number(output: &mut Vec<u8>, value: i32) {
    match value {
        -107..=107 => output.push((value + 139) as u8),
        108..=1131 => {
            let encoded = value - 108;
            output.push((247 + (encoded >> 8)) as u8);
            output.push((encoded & 0xFF) as u8);
        }
        -1131..=-108 => {
            let encoded = -value - 108;
            output.push((251 + (encoded >> 8)) as u8);
            output.push((encoded & 0xFF) as u8);
        }
        _ => {
            output.push(255);
            output.extend_from_slice(&value.to_be_bytes());
        }
    }
}

pub(super) fn numbers(output: &mut Vec<u8>, values: &[i32]) {
    for &value in values {
        push_number(output, value);
    }
}

pub(super) fn op(output: &mut Vec<u8>, code: u8) {
    output.push(code);
}

pub(super) fn program(
    charstrings: &[(&str, Vec<u8>)],
    subrs: &[(u32, Vec<u8>)],
) -> Type1FontProgram {
    Type1FontProgram {
        header: Type1Header::default(),
        len_iv: -1,
        charstrings: charstrings
            .iter()
            .map(|(name, data)| ((*name).to_string(), data.clone()))
            .collect(),
        subrs: subrs.iter().cloned().collect(),
    }
}

fn triangle() -> Vec<u8> {
    let mut data = Vec::new();
    numbers(&mut data, &[20, 600]);
    op(&mut data, 13);
    numbers(&mut data, &[100, 0]);
    op(&mut data, 21);
    numbers(&mut data, &[0, 700]);
    op(&mut data, 5);
    numbers(&mut data, &[-100, 0]);
    op(&mut data, 5);
    numbers(&mut data, &[0, -700]);
    op(&mut data, 5);
    op(&mut data, 9);
    op(&mut data, 14);
    data
}

#[test]
fn decodes_variable_length_numbers() {
    let mut small = Vec::new();
    numbers(&mut small, &[-200, 500]);
    op(&mut small, 13);
    op(&mut small, 14);

    let mut large = Vec::new();
    numbers(&mut large, &[-1131, 12345]);
    op(&mut large, 13);
    op(&mut large, 14);

    let program = program(&[("small", small), ("large", large)], &[]);
    let small = program.glyph("small").unwrap().unwrap();
    assert_eq!(small.width, 500.0);
    assert_eq!(small.side_bearing, [-200.0, 0.0]);

    let large = program.glyph("large").unwrap().unwrap();
    assert_eq!(large.width, 12345.0);
    assert_eq!(large.side_bearing, [-1131.0, 0.0]);
}

#[test]
fn interprets_width_and_line_outline() {
    let program = program(&[("A", triangle())], &[]);
    let glyph = program.glyph("A").unwrap().unwrap();

    assert_eq!(glyph.width, 600.0);
    assert_eq!(glyph.side_bearing, [20.0, 0.0]);
    assert_eq!(
        glyph.path,
        vec![
            Type1PathSegment::MoveTo { x: 120.0, y: 0.0 },
            Type1PathSegment::LineTo { x: 120.0, y: 700.0 },
            Type1PathSegment::LineTo { x: 20.0, y: 700.0 },
            Type1PathSegment::LineTo { x: 20.0, y: 0.0 },
            Type1PathSegment::ClosePath,
        ]
    );
    assert_eq!(glyph.seac, None);
}

#[test]
fn interprets_alternating_hlineto_and_vlineto() {
    let mut horizontal = Vec::new();
    numbers(&mut horizontal, &[0, 0]);
    op(&mut horizontal, 13);
    numbers(&mut horizontal, &[10, 20, 30]);
    op(&mut horizontal, 6);
    op(&mut horizontal, 14);

    let mut vertical = Vec::new();
    numbers(&mut vertical, &[0, 0]);
    op(&mut vertical, 13);
    numbers(&mut vertical, &[10, 20, 30]);
    op(&mut vertical, 7);
    op(&mut vertical, 14);

    let program = program(&[("H", horizontal), ("V", vertical)], &[]);
    assert_eq!(
        program.glyph("H").unwrap().unwrap().path,
        vec![
            Type1PathSegment::MoveTo { x: 0.0, y: 0.0 },
            Type1PathSegment::LineTo { x: 10.0, y: 0.0 },
            Type1PathSegment::LineTo { x: 10.0, y: 20.0 },
            Type1PathSegment::LineTo { x: 40.0, y: 20.0 },
        ]
    );
    assert_eq!(
        program.glyph("V").unwrap().unwrap().path,
        vec![
            Type1PathSegment::MoveTo { x: 0.0, y: 0.0 },
            Type1PathSegment::LineTo { x: 0.0, y: 10.0 },
            Type1PathSegment::LineTo { x: 20.0, y: 10.0 },
            Type1PathSegment::LineTo { x: 20.0, y: 40.0 },
        ]
    );
}

#[test]
fn interprets_relative_curves_and_hv_operators() {
    let mut cubic = Vec::new();
    numbers(&mut cubic, &[0, 0]);
    op(&mut cubic, 13);
    numbers(&mut cubic, &[100, 100]);
    op(&mut cubic, 21);
    numbers(&mut cubic, &[10, 10, 20, 0, 30, -10]);
    op(&mut cubic, 8);
    op(&mut cubic, 14);

    let mut hv = Vec::new();
    numbers(&mut hv, &[0, 0]);
    op(&mut hv, 13);
    numbers(&mut hv, &[100, 100]);
    op(&mut hv, 21);
    numbers(&mut hv, &[10, 20, 30, 40]);
    op(&mut hv, 31);
    op(&mut hv, 14);

    let mut vh = Vec::new();
    numbers(&mut vh, &[0, 0]);
    op(&mut vh, 13);
    numbers(&mut vh, &[100, 100]);
    op(&mut vh, 21);
    numbers(&mut vh, &[10, 20, 30, 40]);
    op(&mut vh, 30);
    op(&mut vh, 14);

    let program = program(&[("C", cubic), ("H", hv), ("V", vh)], &[]);
    assert_eq!(
        program.glyph("C").unwrap().unwrap().path,
        vec![
            Type1PathSegment::MoveTo { x: 100.0, y: 100.0 },
            Type1PathSegment::CurveTo {
                control1: [110.0, 110.0],
                control2: [130.0, 110.0],
                to: [160.0, 100.0],
            },
        ]
    );
    assert_eq!(
        program.glyph("H").unwrap().unwrap().path,
        vec![
            Type1PathSegment::MoveTo { x: 100.0, y: 100.0 },
            Type1PathSegment::CurveTo {
                control1: [110.0, 100.0],
                control2: [130.0, 130.0],
                to: [130.0, 170.0],
            },
        ]
    );
    assert_eq!(
        program.glyph("V").unwrap().unwrap().path,
        vec![
            Type1PathSegment::MoveTo { x: 100.0, y: 100.0 },
            Type1PathSegment::CurveTo {
                control1: [100.0, 110.0],
                control2: [120.0, 140.0],
                to: [160.0, 140.0],
            },
        ]
    );
}

#[test]
fn interprets_hvcurveto_sequences_with_optional_final_delta() {
    let mut data = Vec::new();
    numbers(&mut data, &[0, 0]);
    op(&mut data, 13);
    numbers(&mut data, &[100, 100]);
    op(&mut data, 21);
    // hv 曲线（无第五参数）后接 vh 曲线（第五参数 5 调整终点 y）。
    numbers(&mut data, &[10, 20, 30, 40, 50, 60, 70, 80, 5]);
    op(&mut data, 31);
    op(&mut data, 14);

    let program = program(&[("A", data)], &[]);
    // 9 个参数 = 两条曲线，第二条的额外参数位于末尾。
    assert_eq!(
        program.glyph("A").unwrap().unwrap().path,
        vec![
            Type1PathSegment::MoveTo { x: 100.0, y: 100.0 },
            Type1PathSegment::CurveTo {
                control1: [110.0, 100.0],
                control2: [130.0, 130.0],
                to: [130.0, 170.0],
            },
            Type1PathSegment::CurveTo {
                control1: [130.0, 220.0],
                control2: [190.0, 290.0],
                to: [270.0, 295.0],
            },
        ]
    );
}

#[test]
fn interprets_subr_calls_and_div() {
    let mut subr = Vec::new();
    op(&mut subr, 12);
    op(&mut subr, 12);
    numbers(&mut subr, &[0]);
    op(&mut subr, 5);
    op(&mut subr, 11);

    let mut data = Vec::new();
    numbers(&mut data, &[0, 600]);
    op(&mut data, 13);
    numbers(&mut data, &[100, 0]);
    op(&mut data, 21);
    numbers(&mut data, &[100, 2]);
    numbers(&mut data, &[0]);
    op(&mut data, 10);
    op(&mut data, 14);

    let program = program(&[("A", data)], &[(0, subr)]);
    let glyph = program.glyph("A").unwrap().unwrap();
    assert_eq!(glyph.width, 600.0);
    assert_eq!(
        glyph.path,
        vec![
            Type1PathSegment::MoveTo { x: 100.0, y: 0.0 },
            Type1PathSegment::LineTo { x: 150.0, y: 0.0 },
        ]
    );
}

fn acute() -> Vec<u8> {
    let mut data = Vec::new();
    numbers(&mut data, &[30, 250]);
    op(&mut data, 13);
    numbers(&mut data, &[70, 400]);
    op(&mut data, 21);
    numbers(&mut data, &[40, 100]);
    op(&mut data, 5);
    numbers(&mut data, &[-80, 0]);
    op(&mut data, 5);
    op(&mut data, 9);
    op(&mut data, 14);
    data
}

fn seac_charstring(asb: i32, dx: i32, dy: i32, base: i32, accent: i32) -> Vec<u8> {
    let mut data = Vec::new();
    numbers(&mut data, &[0, 500]);
    op(&mut data, 13);
    numbers(&mut data, &[asb, dx, dy, base, accent]);
    op(&mut data, 12);
    op(&mut data, 6);
    data
}

#[test]
fn expands_seac_into_base_and_translated_accent_outlines() {
    let program = program(
        &[
            ("A", triangle()),
            ("acute", acute()),
            ("Aacute", seac_charstring(30, 250, 300, 65, 194)),
        ],
        &[],
    );
    let glyph = program.glyph("Aacute").unwrap().unwrap();

    assert_eq!(glyph.width, 500.0);
    assert_eq!(glyph.side_bearing, [0.0, 0.0]);
    assert_eq!(
        glyph.seac,
        Some(Type1Seac {
            side_bearing: 30.0,
            dx: 250.0,
            dy: 300.0,
            base_char: 65,
            accent_char: 194,
        })
    );
    assert_eq!(
        glyph.path,
        vec![
            Type1PathSegment::MoveTo { x: 120.0, y: 0.0 },
            Type1PathSegment::LineTo { x: 120.0, y: 700.0 },
            Type1PathSegment::LineTo { x: 20.0, y: 700.0 },
            Type1PathSegment::LineTo { x: 20.0, y: 0.0 },
            Type1PathSegment::ClosePath,
            // 重音轮廓平移 (adx - asb, ady) = (220, 300)。
            Type1PathSegment::MoveTo { x: 320.0, y: 700.0 },
            Type1PathSegment::LineTo { x: 360.0, y: 800.0 },
            Type1PathSegment::LineTo { x: 280.0, y: 800.0 },
            Type1PathSegment::ClosePath,
        ]
    );
}

#[test]
fn expands_nested_seac_components_recursively() {
    let mut space = Vec::new();
    numbers(&mut space, &[0, 250]);
    op(&mut space, 13);
    op(&mut space, 14);

    let program = program(
        &[
            ("space", space),
            ("acute", acute()),
            // `A` 自己也是合成字形：space + acute。
            ("A", seac_charstring(0, 100, 200, 32, 194)),
            ("Aacute", seac_charstring(0, 50, 50, 65, 194)),
        ],
        &[],
    );
    let glyph = program.glyph("Aacute").unwrap().unwrap();
    assert_eq!(
        glyph.path,
        vec![
            Type1PathSegment::MoveTo { x: 200.0, y: 600.0 },
            Type1PathSegment::LineTo { x: 240.0, y: 700.0 },
            Type1PathSegment::LineTo { x: 160.0, y: 700.0 },
            Type1PathSegment::ClosePath,
            Type1PathSegment::MoveTo { x: 150.0, y: 450.0 },
            Type1PathSegment::LineTo { x: 190.0, y: 550.0 },
            Type1PathSegment::LineTo { x: 110.0, y: 550.0 },
            Type1PathSegment::ClosePath,
        ]
    );
}

#[test]
fn rejects_seac_with_missing_or_unmapped_components() {
    let program = program(
        &[
            ("A", triangle()),
            ("missing-accent", seac_charstring(0, 0, 0, 65, 194)),
            ("unmapped", seac_charstring(0, 0, 0, 0, 194)),
        ],
        &[],
    );
    let error = program.glyph("missing-accent").unwrap_err();
    assert!(
        error
            .to_string()
            .contains("accent glyph `acute` is missing"),
        "{error}"
    );
    let error = program.glyph("unmapped").unwrap_err();
    assert!(
        error
            .to_string()
            .contains("character code 0 has no StandardEncoding glyph name"),
        "{error}"
    );
}

#[test]
fn rejects_excessively_nested_seac() {
    let mut recursive = Vec::new();
    numbers(&mut recursive, &[0, 500]);
    op(&mut recursive, 13);
    numbers(&mut recursive, &[0, 0, 0, 65, 194]);
    op(&mut recursive, 12);
    op(&mut recursive, 6);

    let program = program(&[("A", recursive)], &[]);
    let error = program.glyph("A").unwrap_err();
    assert!(error.to_string().contains("seac nesting"), "{error}");
}

#[test]
fn rejects_truncated_or_malformed_charstrings() {
    let mut no_endchar = Vec::new();
    numbers(&mut no_endchar, &[0, 0]);
    op(&mut no_endchar, 13);

    let mut no_width = Vec::new();
    op(&mut no_width, 14);

    let mut top_level_return = Vec::new();
    numbers(&mut top_level_return, &[0, 0]);
    op(&mut top_level_return, 13);
    op(&mut top_level_return, 11);

    let mut too_few = Vec::new();
    numbers(&mut too_few, &[0, 0]);
    op(&mut too_few, 13);
    op(&mut too_few, 21);
    op(&mut too_few, 14);

    let mut missing_subr = Vec::new();
    numbers(&mut missing_subr, &[0, 0]);
    op(&mut missing_subr, 13);
    numbers(&mut missing_subr, &[7]);
    op(&mut missing_subr, 10);
    op(&mut missing_subr, 14);

    let program = program(
        &[
            ("no-endchar", no_endchar),
            ("no-width", no_width),
            ("return", top_level_return),
            ("few", too_few),
            ("missing", missing_subr),
        ],
        &[],
    );
    assert!(
        program
            .glyph("no-endchar")
            .unwrap_err()
            .to_string()
            .contains("endchar")
    );
    assert!(
        program
            .glyph("no-width")
            .unwrap_err()
            .to_string()
            .contains("hsbw")
    );
    assert!(
        program
            .glyph("return")
            .unwrap_err()
            .to_string()
            .contains("outside")
    );
    assert!(
        program
            .glyph("few")
            .unwrap_err()
            .to_string()
            .contains("rmoveto")
    );
    assert!(
        program
            .glyph("missing")
            .unwrap_err()
            .to_string()
            .contains("subroutine 7")
    );
}

#[test]
fn rejects_excessive_subroutine_nesting() {
    let mut recursive = Vec::new();
    numbers(&mut recursive, &[0]);
    op(&mut recursive, 10);
    op(&mut recursive, 11);

    let mut data = Vec::new();
    numbers(&mut data, &[0, 0]);
    op(&mut data, 13);
    numbers(&mut data, &[0]);
    op(&mut data, 10);
    op(&mut data, 14);

    let program = program(&[("A", data)], &[(0, recursive)]);
    let error = program.glyph("A").unwrap_err();
    assert!(error.to_string().contains("nesting"), "{error}");
}

#[test]
fn returns_none_for_an_unknown_glyph() {
    let program = program(&[], &[]);
    assert!(program.glyph("missing").unwrap().is_none());
}
