use super::error::{ContentStreamError, Result};
use super::value::token_to_value;
use crate::core::reader::content::TextElement;
use crate::core::reader::text::tokenizer::CsToken;
use crate::core::types::Value;

/// One content-stream operand: a tokenizer token, or a `num gen R` triplet
/// that has already been coalesced into an indirect reference.
///
/// The tokenizer emits the three pieces of an indirect reference separately
/// and treats every keyword — `R` included — as a potential operator, so a
/// `BDC`/`DP` property operand written as `/Tag 5 0 R BDC` would otherwise
/// abort the whole stream parse with a bogus `R` operator plus a truncated
/// `BDC`. Folding happens at the operand-collection boundary so the
/// `expect_*` helpers below see one operand, never a stray `R` keyword.
///
/// ISO 32000-1 14.6.2 (raw text lines 29087–29091 of
/// `docs/PDF32000_2008.pdf`) only authorizes an inline dictionary or a
/// `/Properties` resource *name* as the property operand; the indirect
/// reference form is read here for interoperability with producers that emit
/// it and is reported as an unresolved operand when it cannot be
/// dereferenced (see [`crate::core::reader::content::MarkedSection`]).
#[derive(Clone, Debug)]
pub(super) enum OperandToken {
    /// A token taken straight from the content-stream tokenizer.
    Token(CsToken),
    /// A coalesced `object generation R` indirect reference operand.
    Reference { object: i64, generation: i64 },
}

impl OperandToken {
    /// The underlying token, or `None` for a coalesced reference.
    pub(super) fn token(&self) -> Option<&CsToken> {
        match self {
            Self::Token(token) => Some(token),
            Self::Reference { .. } => None,
        }
    }

    /// Whether this operand opens an array literal.
    fn is_array_open(&self) -> bool {
        matches!(self, Self::Token(CsToken::ArrayOpen))
    }

    /// Whether this operand closes an array literal.
    fn is_array_close(&self) -> bool {
        matches!(self, Self::Token(CsToken::ArrayClose))
    }
}

/// Borrow the operand at `index`, or report a truncated operator.
///
/// Content streams are untrusted input: an operator may be written with fewer
/// operands than its arity, so every positional access must answer with the
/// existing [`ContentStreamError::InvalidOperandCount`] instead of indexing
/// panic.
fn operand<'a>(
    tokens: &'a [OperandToken],
    operator: &str,
    index: usize,
) -> Result<&'a OperandToken> {
    tokens
        .get(index)
        .ok_or_else(|| ContentStreamError::InvalidOperandCount {
            operator: operator.into(),
            expected: (index + 1).to_string(),
            actual: tokens.len(),
        })
}

pub(super) fn expect_number(tokens: &[OperandToken], operator: &str, index: usize) -> Result<f64> {
    number_value(operand(tokens, operator, index)?, operator, index)
}

pub(super) fn expect_integer(tokens: &[OperandToken], operator: &str, index: usize) -> Result<i64> {
    integer_value(operand(tokens, operator, index)?, operator, index)
}

pub(super) fn expect_name(tokens: &[OperandToken], operator: &str, index: usize) -> Result<String> {
    name_value(operand(tokens, operator, index)?, operator, index)
}

pub(super) fn expect_string(
    tokens: &[OperandToken],
    operator: &str,
    index: usize,
) -> Result<Vec<u8>> {
    string_value(operand(tokens, operator, index)?, operator, index)
}

/// Invalid-operand error for an operand that is not the requested shape
/// (including a coalesced indirect reference).
fn invalid_operand(operator: &str, index: usize, message: &str) -> ContentStreamError {
    ContentStreamError::InvalidOperandValue {
        operator: operator.into(),
        operand_index: index,
        message: message.into(),
    }
}

fn number_value(operand: &OperandToken, operator: &str, index: usize) -> Result<f64> {
    match operand.token() {
        Some(CsToken::Number(n)) => Ok(*n),
        _ => Err(invalid_operand(operator, index, "expected number")),
    }
}

fn integer_value(operand: &OperandToken, operator: &str, index: usize) -> Result<i64> {
    match operand.token() {
        Some(CsToken::Number(n)) if n.fract() == 0.0 => Ok(*n as i64),
        _ => Err(invalid_operand(operator, index, "expected integer")),
    }
}

fn name_value(operand: &OperandToken, operator: &str, index: usize) -> Result<String> {
    match operand.token() {
        Some(CsToken::Name(n)) => Ok(n.clone()),
        _ => Err(invalid_operand(operator, index, "expected name")),
    }
}

fn string_value(operand: &OperandToken, operator: &str, index: usize) -> Result<Vec<u8>> {
    match operand.token() {
        Some(CsToken::LiteralString(s)) | Some(CsToken::HexString(s)) => Ok(s.clone()),
        _ => Err(invalid_operand(operator, index, "expected string")),
    }
}

pub(super) fn expect_numbers_exact(
    tokens: &[OperandToken],
    operator: &str,
    count: usize,
) -> Result<Vec<f64>> {
    if tokens.len() != count {
        return Err(ContentStreamError::InvalidOperandCount {
            operator: operator.into(),
            expected: count.to_string(),
            actual: tokens.len(),
        });
    }
    tokens
        .iter()
        .enumerate()
        .map(|(i, t)| number_value(t, operator, i))
        .collect()
}

pub(super) fn expect_numbers_variadic(tokens: &[OperandToken], operator: &str) -> Result<Vec<f64>> {
    tokens
        .iter()
        .enumerate()
        .map(|(i, t)| number_value(t, operator, i))
        .collect()
}

/// Split one `[ ... ]` array literal off the front of `tokens`.
///
/// Returns the operands between the brackets and the index just past `]`.
/// Nested arrays are not valid PDF content-stream operands, so the first
/// `ArrayClose` ends the array.
fn take_array<'a>(
    tokens: &'a [OperandToken],
    operator: &str,
    index: usize,
) -> Result<(&'a [OperandToken], usize)> {
    if !tokens.first().is_some_and(OperandToken::is_array_open) {
        return Err(invalid_operand(operator, index, "expected array"));
    }
    let close = tokens[1..]
        .iter()
        .position(OperandToken::is_array_close)
        .map(|p| p + 1)
        .ok_or_else(|| invalid_operand(operator, index, "unterminated array"))?;
    Ok((&tokens[1..close], close + 1))
}

/// Parse a leading numeric array operand; returns the values and the index
/// just past the closing bracket.
pub(super) fn expect_number_array(
    tokens: &[OperandToken],
    operator: &str,
    index: usize,
) -> Result<(Vec<f64>, usize)> {
    let (inner, next) = take_array(tokens, operator, index)?;
    let values = inner
        .iter()
        .enumerate()
        .map(|(i, t)| number_value(t, operator, i))
        .collect::<Result<Vec<_>>>()?;
    Ok((values, next))
}

pub(super) fn expect_text_array(
    tokens: &[OperandToken],
    operator: &str,
    index: usize,
) -> Result<Vec<TextElement>> {
    let (inner, next) = take_array(tokens, operator, index)?;
    if next != tokens.len() {
        return Err(ContentStreamError::InvalidOperandCount {
            operator: operator.into(),
            expected: "1".into(),
            actual: 2,
        });
    }
    inner
        .iter()
        .enumerate()
        .map(|(i, t)| match t.token() {
            Some(CsToken::LiteralString(s)) | Some(CsToken::HexString(s)) => {
                Ok(TextElement::String(s.clone()))
            }
            Some(CsToken::Number(n)) => Ok(TextElement::Adjustment(*n)),
            _ => Err(invalid_operand(
                operator,
                i,
                "expected string or number in text array",
            )),
        })
        .collect()
}

/// The property operand of `BDC`/`DP` (ISO 32000-1 14.6.2): an inline
/// dictionary, a `/Properties` resource name, or — tolerated for
/// interoperability — an indirect reference to the property dictionary.
pub(super) fn expect_properties(
    tokens: &[OperandToken],
    operator: &str,
    index: usize,
) -> Result<Value> {
    match tokens.get(index) {
        Some(OperandToken::Reference { object, generation }) => {
            Ok(Value::Ref(*object, *generation))
        }
        Some(OperandToken::Token(token)) => token_to_value(token),
        None => Err(ContentStreamError::InvalidOperandCount {
            operator: operator.into(),
            expected: "at least 2".into(),
            actual: tokens.len(),
        }),
    }
}
