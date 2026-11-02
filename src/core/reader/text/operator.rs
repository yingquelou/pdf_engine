//! Text/graphics operator execution over the operand stack.

use super::matrix::Matrix;
use super::state::{BreakKind, Operand, TextState};

pub(super) fn execute_operator(kw: &str, operands: &[Operand], state: &mut TextState) {
    match kw {
        "Tf" => {
            if let Some(Operand::Name(name)) = operands.first() {
                state.set_font(name);
            }
        }
        "Tj" => {
            if let Some(bytes) = last_string(operands) {
                state.show_bytes(bytes);
            }
        }
        "TJ" => {
            for op in operands {
                if let Operand::Array(items) = op {
                    for item in items {
                        match item {
                            Operand::String(bytes) => state.show_bytes(bytes),
                            Operand::Number(n) if *n < -100.0 => {
                                state.add_break(BreakKind::Space);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        "'" => {
            state.next_line();
            state.add_break(BreakKind::Newline);
            if let Some(bytes) = last_string(operands) {
                state.show_bytes(bytes);
            }
        }
        "\"" => {
            // The first two operands are word and character spacing, which
            // do not affect the run's origin.
            state.next_line();
            state.add_break(BreakKind::Newline);
            if let Some(bytes) = last_string(operands) {
                state.show_bytes(bytes);
            }
        }
        "Td" | "TD" => {
            if let Some([tx, ty]) = trailing_numbers::<2>(operands) {
                if kw == "TD" {
                    state.leading = -ty;
                }
                state.move_line(tx, ty);
            }
            state.add_break(BreakKind::Newline);
        }
        "T*" => {
            state.next_line();
            state.add_break(BreakKind::Newline);
        }
        "Tm" => {
            if let Some(values) = trailing_numbers::<6>(operands) {
                state.set_text_matrix(Matrix::from_values(values));
            }
            state.add_break(BreakKind::Newline);
        }
        "TL" => {
            if let Some([leading]) = trailing_numbers::<1>(operands) {
                state.leading = leading;
            }
        }
        "Ts" => {
            if let Some([rise]) = trailing_numbers::<1>(operands) {
                state.rise = rise;
            }
        }
        "BT" => {
            state.flush_fragment();
            state.begin_text();
        }
        "ET" => state.flush_fragment(),
        "cm" => {
            if let Some(values) = trailing_numbers::<6>(operands) {
                state.concatenate_ctm(Matrix::from_values(values));
            }
        }
        "q" => state.save_state(),
        "Q" => state.restore_state(),
        _ => {}
    }
}

pub(super) fn name_operand(operand: Option<&Operand>) -> Option<&str> {
    match operand {
        Some(Operand::Name(name)) => Some(name.as_str()),
        _ => None,
    }
}

/// Parse the trailing `N` operands as numbers (`Td`, `Tm`, `cm`, `TL`, `Ts`).
fn trailing_numbers<const N: usize>(operands: &[Operand]) -> Option<[f64; N]> {
    if operands.len() < N {
        return None;
    }
    let mut values = [0.0; N];
    for (slot, operand) in values.iter_mut().zip(&operands[operands.len() - N..]) {
        let Operand::Number(number) = operand else {
            return None;
        };
        *slot = *number;
    }
    Some(values)
}

fn last_string(operands: &[Operand]) -> Option<&[u8]> {
    operands.iter().rev().find_map(|op| match op {
        Operand::String(b) => Some(b.as_slice()),
        _ => None,
    })
}
