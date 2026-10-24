//! Type 2 charstring width operands and path-drawing detection.

use super::parse::{read_i16, read_i32, read_u8};
use crate::core::reader::font::{FontError, FontResult};

/// Maximum number of operands Type 2 allows on the stack.
const MAX_CHARSTRING_OPERANDS: usize = 48;

/// Return the explicit width operand of a Type 2 charstring, if present.
///
/// The width precedes the first stack-clearing operator and is encoded
/// relative to `nominalWidthX`. Its presence follows the operand-count parity
/// rule: an operator that normally takes an even number of operands has a
/// width when the operand count is odd, and vice versa.
pub(super) fn charstring_width_operand(charstring: &[u8]) -> FontResult<Option<f64>> {
    let mut operands: Vec<f64> = Vec::new();
    let mut position = 0;
    while position < charstring.len() {
        let first = charstring[position];
        position += 1;
        match first {
            28 => {
                let value = read_i16(charstring, position)?;
                position += 2;
                operands.push(f64::from(value));
            }
            32..=246 => operands.push(f64::from(first) - 139.0),
            247..=250 => {
                let second = read_u8(charstring, position)?;
                position += 1;
                operands.push((f64::from(first) - 247.0) * 256.0 + f64::from(second) + 108.0);
            }
            251..=254 => {
                let second = read_u8(charstring, position)?;
                position += 1;
                operands.push(-((f64::from(first) - 251.0) * 256.0) - f64::from(second) - 108.0);
            }
            255 => {
                let value = read_i32(charstring, position)?;
                position += 4;
                operands.push(f64::from(value) / 65536.0);
            }
            12 => {
                let second = read_u8(charstring, position)?;
                return Ok(match escaped_operator_parity(second) {
                    Some(parity) => width_from_operands(&operands, parity),
                    // Arithmetic operators are not stack-clearing; a width
                    // cannot be located before the first drawing operator.
                    None => None,
                });
            }
            operator => {
                return Ok(match operator_parity(operator) {
                    Some(parity) => width_from_operands(&operands, parity),
                    None => None,
                });
            }
        }
        if operands.len() > MAX_CHARSTRING_OPERANDS {
            return Err(FontError::Invalid(
                "CFF charstring exceeds the operand stack limit".into(),
            ));
        }
    }
    Ok(None)
}

/// Expected operand-count parity of a stack-clearing Type 2 operator:
/// `true` when the operator normally takes an odd number of operands.
fn operator_parity(operator: u8) -> Option<bool> {
    match operator {
        // hstem, vstem, rlineto, rrcurveto, return, endchar, hstemhm,
        // hintmask, cntrmask, rmoveto, vstemhm, rcurveline, rlinecurve,
        // vvcurveto, hhcurveto, vhcurveto, hvcurveto
        1 | 3 | 5 | 8 | 11 | 14 | 18 | 19 | 20 | 21 | 23 | 24 | 25 | 26 | 27 | 30 | 31 => {
            Some(false)
        }
        // vmoveto, hlineto, vlineto, callsubr, hmoveto, callgsubr
        4 | 6 | 7 | 10 | 22 | 29 => Some(true),
        // Reserved or non-clearing bytes.
        _ => None,
    }
}

/// Expected operand-count parity of a stack-clearing escaped (`12 x`) operator.
fn escaped_operator_parity(operator: u8) -> Option<bool> {
    match operator {
        // hflex (7), flex (13), hflex1 (9), flex1 (11) all take an odd number
        // of operands.
        34..=37 => Some(true),
        _ => None,
    }
}

fn width_from_operands(operands: &[f64], odd: bool) -> Option<f64> {
    let expected_parity = if odd { 1 } else { 0 };
    if operands.len() % 2 == expected_parity {
        return None;
    }
    operands.first().copied()
}

/// Return whether a charstring contains an operator that draws a path.
///
/// Move operators alone produce an empty outline, so they do not count.
pub(super) fn charstring_draws_path(charstring: &[u8]) -> bool {
    let mut position = 0;
    while position < charstring.len() {
        let first = charstring[position];
        position += 1;
        match first {
            28 => position += 2,
            32..=246 => {}
            247..=254 => position += 1,
            255 => position += 4,
            12 => {
                let Some(second) = charstring.get(position) else {
                    return false;
                };
                position += 1;
                if (34..=37).contains(second) {
                    return true;
                }
            }
            5..=8 | 24..=27 | 30 | 31 => return true,
            _ => {}
        }
    }
    false
}
