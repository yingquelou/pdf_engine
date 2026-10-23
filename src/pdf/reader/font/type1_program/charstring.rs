//! Type 1 CharString interpretation: widths, path construction, and subrs.

use std::collections::BTreeMap;

use super::super::{FontError, FontResult};

const MAX_SUBR_DEPTH: usize = 10;

/// One path segment produced by interpreting a Type 1 CharString.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Type1PathSegment {
    /// Start a new contour at an absolute glyph-space position.
    MoveTo { x: f64, y: f64 },
    /// Draw a line to an absolute glyph-space position.
    LineTo { x: f64, y: f64 },
    /// Draw a cubic Bézier curve with absolute control and end points.
    CurveTo {
        control1: [f64; 2],
        control2: [f64; 2],
        to: [f64; 2],
    },
    /// Close the current contour back to its first point.
    ClosePath,
}

/// The `seac` operands recorded by a Type 1 CharString.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Type1Seac {
    /// Side bearing of the accent character.
    pub side_bearing: f64,
    /// Horizontal displacement from the base character's origin.
    pub dx: f64,
    /// Vertical displacement from the base character's origin.
    pub dy: f64,
    /// Character code of the base glyph.
    pub base_char: u8,
    /// Character code of the accent glyph.
    pub accent_char: u8,
}

/// The interpreted result of one Type 1 CharString.
#[derive(Debug, Clone, PartialEq)]
pub struct Type1CharString {
    /// Advance width from `hsbw`/`sbw`, in glyph-space units.
    pub width: f64,
    /// Left side bearing from `hsbw`/`sbw`, in glyph-space units.
    pub side_bearing: [f64; 2],
    /// Absolute path segments in glyph space. Hints are not applied.
    pub path: Vec<Type1PathSegment>,
    /// `seac` composite operands; such a glyph has no local outline.
    pub seac: Option<Type1Seac>,
}

pub(super) fn interpret(
    charstring: &[u8],
    subrs: &BTreeMap<u32, Vec<u8>>,
    glyph_name: &str,
) -> FontResult<Type1CharString> {
    let mut interpreter = Interpreter {
        subrs,
        glyph_name,
        stack: Vec::new(),
        x: 0.0,
        y: 0.0,
        width: 0.0,
        side_bearing: [0.0; 2],
        saw_width: false,
        contour_start: None,
        positioned_by_moveto: false,
        path: Vec::new(),
        seac: None,
    };
    match interpreter.execute(charstring, 0)? {
        Flow::End => {}
        Flow::Return => return Err(interpreter.error("`return` outside a subroutine")),
        Flow::Continue => return Err(interpreter.error("charstring is missing endchar")),
    }
    if !interpreter.saw_width {
        return Err(interpreter.error("charstring is missing hsbw/sbw"));
    }
    Ok(Type1CharString {
        width: interpreter.width,
        side_bearing: interpreter.side_bearing,
        path: interpreter.path,
        seac: interpreter.seac,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Flow {
    Continue,
    Return,
    End,
}

struct Interpreter<'a> {
    subrs: &'a BTreeMap<u32, Vec<u8>>,
    glyph_name: &'a str,
    stack: Vec<f64>,
    x: f64,
    y: f64,
    width: f64,
    side_bearing: [f64; 2],
    saw_width: bool,
    contour_start: Option<[f64; 2]>,
    positioned_by_moveto: bool,
    path: Vec<Type1PathSegment>,
    seac: Option<Type1Seac>,
}

impl Interpreter<'_> {
    fn execute(&mut self, data: &[u8], depth: usize) -> FontResult<Flow> {
        let mut position = 0usize;
        while position < data.len() {
            let byte = data[position];
            position += 1;
            match byte {
                32..=246 => self.stack.push(f64::from(i16::from(byte) - 139)),
                247..=250 => {
                    let next = *data
                        .get(position)
                        .ok_or_else(|| self.error("number is truncated"))?;
                    position += 1;
                    let value = (i16::from(byte) - 247) * 256 + i16::from(next) + 108;
                    self.stack.push(f64::from(value));
                }
                251..=254 => {
                    let next = *data
                        .get(position)
                        .ok_or_else(|| self.error("number is truncated"))?;
                    position += 1;
                    let value = -(i16::from(byte) - 251) * 256 - i16::from(next) - 108;
                    self.stack.push(f64::from(value));
                }
                255 => {
                    let bytes: [u8; 4] = data
                        .get(position..position + 4)
                        .ok_or_else(|| self.error("32-bit number is truncated"))?
                        .try_into()
                        .expect("four-byte slice");
                    position += 4;
                    self.stack.push(f64::from(i32::from_be_bytes(bytes)));
                }
                operator => match self.operator(operator, data, &mut position, depth)? {
                    Flow::Continue => {}
                    flow => return Ok(flow),
                },
            }
        }
        Ok(Flow::Continue)
    }

    fn operator(
        &mut self,
        operator: u8,
        data: &[u8],
        position: &mut usize,
        depth: usize,
    ) -> FontResult<Flow> {
        match operator {
            1 | 3 => {
                self.stack.clear();
                Ok(Flow::Continue)
            }
            4 => {
                let args = self.take(1, "vmoveto")?;
                self.y += args[0];
                self.move_to();
                Ok(Flow::Continue)
            }
            5 => {
                let args = self.take_rest("rlineto", 2)?;
                if args.len() % 2 != 0 {
                    return Err(self.error("rlineto requires argument pairs"));
                }
                for pair in args.as_chunks::<2>().0 {
                    let x = self.x + pair[0];
                    let y = self.y + pair[1];
                    self.line_to(x, y);
                }
                Ok(Flow::Continue)
            }
            6 | 7 => {
                let name = if operator == 6 { "hlineto" } else { "vlineto" };
                let args = self.take_rest(name, 1)?;
                let mut horizontal = operator == 6;
                for &delta in &args {
                    let (x, y) = if horizontal {
                        (self.x + delta, self.y)
                    } else {
                        (self.x, self.y + delta)
                    };
                    self.line_to(x, y);
                    horizontal = !horizontal;
                }
                Ok(Flow::Continue)
            }
            8 => {
                let args = self.take_rest("rrcurveto", 6)?;
                if args.len() % 6 != 0 {
                    return Err(self.error("rrcurveto requires groups of six arguments"));
                }
                for curve in args.as_chunks::<6>().0 {
                    let control1 = [self.x + curve[0], self.y + curve[1]];
                    let control2 = [control1[0] + curve[2], control1[1] + curve[3]];
                    let end = [control2[0] + curve[4], control2[1] + curve[5]];
                    self.curve_to(control1, control2, end);
                }
                Ok(Flow::Continue)
            }
            9 => {
                self.stack.clear();
                self.path.push(Type1PathSegment::ClosePath);
                if let Some([x, y]) = self.contour_start {
                    self.x = x;
                    self.y = y;
                }
                self.positioned_by_moveto = false;
                Ok(Flow::Continue)
            }
            10 => {
                let index = self.pop_index("callsubr")?;
                let subrs = self.subrs;
                let subr = subrs.get(&index).ok_or_else(|| {
                    self.error(format!("callsubr references missing subroutine {index}"))
                })?;
                if depth >= MAX_SUBR_DEPTH {
                    return Err(self.error("subroutine nesting is too deep"));
                }
                match self.execute(subr, depth + 1)? {
                    Flow::End => Ok(Flow::End),
                    Flow::Continue | Flow::Return => Ok(Flow::Continue),
                }
            }
            11 => Ok(Flow::Return),
            13 => {
                let args = self.take(2, "hsbw")?;
                self.set_width(args[0], 0.0, args[1]);
                Ok(Flow::Continue)
            }
            14 => {
                self.stack.clear();
                Ok(Flow::End)
            }
            21 => {
                let args = self.take(2, "rmoveto")?;
                self.x += args[0];
                self.y += args[1];
                self.move_to();
                Ok(Flow::Continue)
            }
            22 => {
                let args = self.take(1, "hmoveto")?;
                self.x += args[0];
                self.move_to();
                Ok(Flow::Continue)
            }
            30 | 31 => {
                let name = if operator == 31 {
                    "hvcurveto"
                } else {
                    "vhcurveto"
                };
                let args = self.take_rest(name, 4)?;
                if args.len() % 4 > 1 {
                    return Err(self.error(format!("{name} requires groups of four arguments")));
                }
                let mut horizontal = operator == 31;
                let mut index = 0usize;
                while index + 4 <= args.len() {
                    let extra = args.len() - index == 5;
                    let final_delta = if extra { args[index + 4] } else { 0.0 };
                    let (control1, control2, end) = if horizontal {
                        let control1 = [self.x + args[index], self.y];
                        let control2 =
                            [control1[0] + args[index + 1], control1[1] + args[index + 2]];
                        let end = [control2[0] + final_delta, control2[1] + args[index + 3]];
                        (control1, control2, end)
                    } else {
                        let control1 = [self.x, self.y + args[index]];
                        let control2 =
                            [control1[0] + args[index + 1], control1[1] + args[index + 2]];
                        let end = [control2[0] + args[index + 3], control2[1] + final_delta];
                        (control1, control2, end)
                    };
                    self.curve_to(control1, control2, end);
                    index += if extra { 5 } else { 4 };
                    horizontal = !horizontal;
                }
                Ok(Flow::Continue)
            }
            12 => {
                let escape = *data
                    .get(*position)
                    .ok_or_else(|| self.error("escape operator is truncated"))?;
                *position += 1;
                self.escape(escape)
            }
            _ => Err(self.error(format!("unsupported charstring operator {operator}"))),
        }
    }

    fn escape(&mut self, escape: u8) -> FontResult<Flow> {
        match escape {
            0 => {
                self.stack.clear();
                Ok(Flow::Continue)
            }
            1 | 2 => {
                self.stack.clear();
                Ok(Flow::Continue)
            }
            6 => {
                let args = self.take(5, "seac")?;
                let base_char = byte_code(args[3])
                    .ok_or_else(|| self.error("seac base character is not a byte"))?;
                let accent_char = byte_code(args[4])
                    .ok_or_else(|| self.error("seac accent character is not a byte"))?;
                self.seac = Some(Type1Seac {
                    side_bearing: args[0],
                    dx: args[1],
                    dy: args[2],
                    base_char,
                    accent_char,
                });
                Ok(Flow::End)
            }
            7 => {
                let args = self.take(4, "sbw")?;
                self.set_width(args[0], args[1], args[2]);
                Ok(Flow::Continue)
            }
            12 => {
                let divisor = self.pop("div")?;
                let dividend = self.pop("div")?;
                if divisor == 0.0 {
                    return Err(self.error("div has a zero divisor"));
                }
                self.stack.push(dividend / divisor);
                Ok(Flow::Continue)
            }
            16 | 17 => {
                Err(self.error("`callothersubr`/`pop` (flex or hint replacement) is not supported"))
            }
            33 => {
                let args = self.take(2, "setcurrentpoint")?;
                self.x = args[0];
                self.y = args[1];
                self.positioned_by_moveto = false;
                Ok(Flow::Continue)
            }
            _ => Err(self.error(format!("unsupported escape operator 12 {escape}"))),
        }
    }

    fn set_width(&mut self, side_bearing_x: f64, side_bearing_y: f64, width: f64) {
        self.side_bearing = [side_bearing_x, side_bearing_y];
        self.width = width;
        self.x = side_bearing_x;
        self.y = side_bearing_y;
        self.saw_width = true;
        self.contour_start = Some([self.x, self.y]);
        self.positioned_by_moveto = false;
    }

    fn move_to(&mut self) {
        self.path.push(Type1PathSegment::MoveTo {
            x: self.x,
            y: self.y,
        });
        self.contour_start = Some([self.x, self.y]);
        self.positioned_by_moveto = true;
    }

    fn line_to(&mut self, x: f64, y: f64) {
        if !self.positioned_by_moveto {
            self.move_to();
        }
        self.path.push(Type1PathSegment::LineTo { x, y });
        self.x = x;
        self.y = y;
    }

    fn curve_to(&mut self, control1: [f64; 2], control2: [f64; 2], to: [f64; 2]) {
        if !self.positioned_by_moveto {
            self.move_to();
        }
        self.path.push(Type1PathSegment::CurveTo {
            control1,
            control2,
            to,
        });
        self.x = to[0];
        self.y = to[1];
    }

    fn take(&mut self, count: usize, operator: &str) -> FontResult<Vec<f64>> {
        if self.stack.len() < count {
            return Err(self.error(format!("{operator} requires {count} argument(s)")));
        }
        let args = self.stack.split_off(self.stack.len() - count);
        self.stack.clear();
        Ok(args)
    }

    fn take_rest(&mut self, operator: &str, minimum: usize) -> FontResult<Vec<f64>> {
        if self.stack.len() < minimum {
            return Err(self.error(format!(
                "{operator} requires at least {minimum} argument(s)"
            )));
        }
        Ok(std::mem::take(&mut self.stack))
    }

    fn pop(&mut self, operator: &str) -> FontResult<f64> {
        self.stack
            .pop()
            .ok_or_else(|| self.error(format!("{operator} requires more arguments")))
    }

    fn pop_index(&mut self, operator: &str) -> FontResult<u32> {
        let value = self.pop(operator)?;
        if value.fract() != 0.0 || value < 0.0 || value > f64::from(u32::MAX) {
            return Err(self.error(format!("{operator} index is not a non-negative integer")));
        }
        Ok(value as u32)
    }

    fn error(&self, message: impl std::fmt::Display) -> FontError {
        FontError::Invalid(format!(
            "invalid Type 1 charstring for glyph `{}`: {message}",
            self.glyph_name
        ))
    }
}

fn byte_code(value: f64) -> Option<u8> {
    (value.fract() == 0.0 && (0.0..=255.0).contains(&value)).then_some(value as u8)
}

#[cfg(test)]
mod tests {
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

    fn numbers(output: &mut Vec<u8>, values: &[i32]) {
        for &value in values {
            push_number(output, value);
        }
    }

    fn op(output: &mut Vec<u8>, code: u8) {
        output.push(code);
    }

    fn program(charstrings: &[(&str, Vec<u8>)], subrs: &[(u32, Vec<u8>)]) -> Type1FontProgram {
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

    #[test]
    fn records_seac_composites_without_an_outline() {
        let mut data = Vec::new();
        numbers(&mut data, &[0, 500]);
        op(&mut data, 13);
        numbers(&mut data, &[0, 100, 200, 65, 233]);
        op(&mut data, 12);
        op(&mut data, 6);

        let program = program(&[("Aacute", data)], &[]);
        let glyph = program.glyph("Aacute").unwrap().unwrap();
        assert_eq!(glyph.width, 500.0);
        assert!(glyph.path.is_empty());
        assert_eq!(
            glyph.seac,
            Some(Type1Seac {
                side_bearing: 0.0,
                dx: 100.0,
                dy: 200.0,
                base_char: 65,
                accent_char: 233,
            })
        );
    }

    #[test]
    fn rejects_othersubr_and_pop() {
        let mut other_subr = Vec::new();
        numbers(&mut other_subr, &[0, 0]);
        op(&mut other_subr, 13);
        op(&mut other_subr, 12);
        op(&mut other_subr, 16);

        let mut pop = Vec::new();
        numbers(&mut pop, &[0, 0]);
        op(&mut pop, 13);
        op(&mut pop, 12);
        op(&mut pop, 17);

        let program = program(&[("O", other_subr), ("P", pop)], &[]);
        let error = program.glyph("O").unwrap_err();
        assert!(error.to_string().contains("callothersubr"), "{error}");
        let error = program.glyph("P").unwrap_err();
        assert!(error.to_string().contains("pop"), "{error}");
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
}
