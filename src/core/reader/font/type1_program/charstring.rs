//! Type 1 CharString interpretation: widths, path construction, and subrs.

use std::collections::BTreeMap;

use super::super::{FontError, FontResult, encoding_data};

mod flex;

const MAX_SUBR_DEPTH: usize = 10;
const MAX_SEAC_DEPTH: usize = 10;

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
    /// StandardEncoding character code of the base glyph.
    pub base_char: u8,
    /// StandardEncoding character code of the accent glyph.
    pub accent_char: u8,
}

/// The interpreted result of one Type 1 CharString.
#[derive(Debug, Clone, PartialEq)]
pub struct Type1CharString {
    /// Advance width from `hsbw`/`sbw`, in glyph-space units.
    pub width: f64,
    /// Left side bearing from `hsbw`/`sbw`, in glyph-space units.
    pub side_bearing: [f64; 2],
    /// Absolute path segments in glyph space. Hints are not applied, and
    /// `seac` component outlines are flattened into this path.
    pub path: Vec<Type1PathSegment>,
    /// The `seac` operands when the glyph is a composite.
    pub seac: Option<Type1Seac>,
}

pub(super) fn interpret(
    charstring: &[u8],
    charstrings: &BTreeMap<String, Vec<u8>>,
    subrs: &BTreeMap<u32, Vec<u8>>,
    glyph_name: &str,
) -> FontResult<Type1CharString> {
    interpret_component(charstring, charstrings, subrs, glyph_name, 0)
}

fn interpret_component(
    charstring: &[u8],
    charstrings: &BTreeMap<String, Vec<u8>>,
    subrs: &BTreeMap<u32, Vec<u8>>,
    glyph_name: &str,
    seac_depth: usize,
) -> FontResult<Type1CharString> {
    let mut interpreter = Interpreter {
        charstrings,
        subrs,
        glyph_name,
        seac_depth,
        stack: Vec::new(),
        x: 0.0,
        y: 0.0,
        width: 0.0,
        side_bearing: [0.0; 2],
        saw_width: false,
        contour_start: None,
        positioned_by_moveto: false,
        flexing: false,
        pending_pops: 0,
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
    if interpreter.flexing {
        return Err(interpreter.error("flex is missing its 3-argument exit call"));
    }
    if interpreter.pending_pops != 0 {
        return Err(interpreter.error("OtherSubrs results are not consumed by `pop`"));
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
    charstrings: &'a BTreeMap<String, Vec<u8>>,
    subrs: &'a BTreeMap<u32, Vec<u8>>,
    glyph_name: &'a str,
    seac_depth: usize,
    stack: Vec<f64>,
    x: f64,
    y: f64,
    width: f64,
    side_bearing: [f64; 2],
    saw_width: bool,
    contour_start: Option<[f64; 2]>,
    positioned_by_moveto: bool,
    flexing: bool,
    pending_pops: usize,
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
                if self.flexing {
                    return self.flex_vmoveto();
                }
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
                if self.flexing {
                    return self.flex_rmoveto();
                }
                let args = self.take(2, "rmoveto")?;
                self.x += args[0];
                self.y += args[1];
                self.move_to();
                Ok(Flow::Continue)
            }
            22 => {
                if self.flexing {
                    return self.flex_hmoveto();
                }
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
                let seac = Type1Seac {
                    side_bearing: args[0],
                    dx: args[1],
                    dy: args[2],
                    base_char,
                    accent_char,
                };
                self.expand_seac(&seac)?;
                self.seac = Some(seac);
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
            16 => self.call_other_subr(),
            17 => self.pop_other_subr_result(),
            33 => {
                let args = self.take(2, "setcurrentpoint")?;
                self.x = args[0];
                self.y = args[1];
                // 轮廓已由 flex 定位，这里只同步 CharString 坐标。
                Ok(Flow::Continue)
            }
            _ => Err(self.error(format!("unsupported escape operator 12 {escape}"))),
        }
    }

    fn expand_seac(&mut self, seac: &Type1Seac) -> FontResult<()> {
        if self.seac_depth >= MAX_SEAC_DEPTH {
            return Err(self.error("seac nesting is too deep"));
        }
        let base_name = self.seac_glyph_name(seac.base_char, "base")?;
        let accent_name = self.seac_glyph_name(seac.accent_char, "accent")?;
        let base = self.component(&base_name, "base")?;
        let accent = self.component(&accent_name, "accent")?;
        // `seac` 以重音字符的左边界定位；字符轮廓已含其 hsbw 侧边界。
        let dx = seac.dx - seac.side_bearing;
        self.path.extend(base.path);
        self.path.extend(
            accent
                .path
                .into_iter()
                .map(|segment| translate_segment(segment, dx, seac.dy)),
        );
        Ok(())
    }

    fn seac_glyph_name(&self, code: u8, role: &str) -> FontResult<String> {
        encoding_data::standard_glyph_name(code).ok_or_else(|| {
            self.error(format!(
                "seac {role} character code {code} has no StandardEncoding glyph name"
            ))
        })
    }

    fn component(&self, glyph_name: &str, role: &str) -> FontResult<Type1CharString> {
        let charstring = self
            .charstrings
            .get(glyph_name)
            .ok_or_else(|| self.error(format!("seac {role} glyph `{glyph_name}` is missing")))?;
        interpret_component(
            charstring,
            self.charstrings,
            self.subrs,
            glyph_name,
            self.seac_depth + 1,
        )
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

fn translate_segment(segment: Type1PathSegment, dx: f64, dy: f64) -> Type1PathSegment {
    match segment {
        Type1PathSegment::MoveTo { x, y } => Type1PathSegment::MoveTo {
            x: x + dx,
            y: y + dy,
        },
        Type1PathSegment::LineTo { x, y } => Type1PathSegment::LineTo {
            x: x + dx,
            y: y + dy,
        },
        Type1PathSegment::CurveTo {
            control1,
            control2,
            to,
        } => Type1PathSegment::CurveTo {
            control1: [control1[0] + dx, control1[1] + dy],
            control2: [control2[0] + dx, control2[1] + dy],
            to: [to[0] + dx, to[1] + dy],
        },
        Type1PathSegment::ClosePath => Type1PathSegment::ClosePath,
    }
}

fn byte_code(value: f64) -> Option<u8> {
    (value.fract() == 0.0 && (0.0..=255.0).contains(&value)).then_some(value as u8)
}

#[cfg(test)]
mod tests;
