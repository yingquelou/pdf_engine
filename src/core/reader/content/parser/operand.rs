use super::error::{ContentStreamError, Result};
use super::value::token_to_value;
use crate::core::reader::content::TextElement;
use crate::core::reader::text::tokenizer::CsToken;
use crate::core::types::Value;

/// Borrow the operand at `index`, or report a truncated operator.
///
/// Content streams are untrusted input: an operator may be written with fewer
/// operands than its arity, so every positional access must answer with the
/// existing [`ContentStreamError::InvalidOperandCount`] instead of indexing
/// panic.
fn operand<'a>(tokens: &'a [CsToken], operator: &str, index: usize) -> Result<&'a CsToken> {
    tokens
        .get(index)
        .ok_or_else(|| ContentStreamError::InvalidOperandCount {
            operator: operator.into(),
            expected: (index + 1).to_string(),
            actual: tokens.len(),
        })
}

pub(super) fn expect_number(tokens: &[CsToken], operator: &str, index: usize) -> Result<f64> {
    number_value(operand(tokens, operator, index)?, operator, index)
}

pub(super) fn expect_integer(tokens: &[CsToken], operator: &str, index: usize) -> Result<i64> {
    integer_value(operand(tokens, operator, index)?, operator, index)
}

pub(super) fn expect_name(tokens: &[CsToken], operator: &str, index: usize) -> Result<String> {
    name_value(operand(tokens, operator, index)?, operator, index)
}

pub(super) fn expect_string(tokens: &[CsToken], operator: &str, index: usize) -> Result<Vec<u8>> {
    string_value(operand(tokens, operator, index)?, operator, index)
}

fn number_value(token: &CsToken, operator: &str, index: usize) -> Result<f64> {
    match token {
        CsToken::Number(n) => Ok(*n),
        _ => Err(ContentStreamError::InvalidOperandValue {
            operator: operator.into(),
            operand_index: index,
            message: "expected number".into(),
        }),
    }
}

fn integer_value(token: &CsToken, operator: &str, index: usize) -> Result<i64> {
    match token {
        CsToken::Number(n) => Ok(*n as i64),
        _ => Err(ContentStreamError::InvalidOperandValue {
            operator: operator.into(),
            operand_index: index,
            message: "expected integer".into(),
        }),
    }
}

fn name_value(token: &CsToken, operator: &str, index: usize) -> Result<String> {
    match token {
        CsToken::Name(n) => Ok(n.clone()),
        _ => Err(ContentStreamError::InvalidOperandValue {
            operator: operator.into(),
            operand_index: index,
            message: "expected name".into(),
        }),
    }
}

fn string_value(token: &CsToken, operator: &str, index: usize) -> Result<Vec<u8>> {
    match token {
        CsToken::LiteralString(s) | CsToken::HexString(s) => Ok(s.clone()),
        _ => Err(ContentStreamError::InvalidOperandValue {
            operator: operator.into(),
            operand_index: index,
            message: "expected string".into(),
        }),
    }
}

pub(super) fn expect_numbers_exact(
    tokens: &[CsToken],
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

pub(super) fn expect_numbers_variadic(tokens: &[CsToken], operator: &str) -> Result<Vec<f64>> {
    tokens
        .iter()
        .enumerate()
        .map(|(i, t)| number_value(t, operator, i))
        .collect()
}

/// Split one `[ ... ]` array literal off the front of `tokens`.
///
/// Returns the tokens between the brackets and the index just past `]`.
/// Nested arrays are not valid PDF content-stream operands, so the first
/// `ArrayClose` ends the array.
fn take_array<'a>(
    tokens: &'a [CsToken],
    operator: &str,
    index: usize,
) -> Result<(&'a [CsToken], usize)> {
    if !matches!(tokens.first(), Some(CsToken::ArrayOpen)) {
        return Err(ContentStreamError::InvalidOperandValue {
            operator: operator.into(),
            operand_index: index,
            message: "expected array".into(),
        });
    }
    let close = tokens[1..]
        .iter()
        .position(|t| matches!(t, CsToken::ArrayClose))
        .map(|p| p + 1)
        .ok_or_else(|| ContentStreamError::InvalidOperandValue {
            operator: operator.into(),
            operand_index: index,
            message: "unterminated array".into(),
        })?;
    Ok((&tokens[1..close], close + 1))
}

/// Parse a leading numeric array operand; returns the values and the index
/// just past the closing bracket.
pub(super) fn expect_number_array(
    tokens: &[CsToken],
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
    tokens: &[CsToken],
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
        .map(|(i, t)| match t {
            CsToken::LiteralString(s) | CsToken::HexString(s) => Ok(TextElement::String(s.clone())),
            CsToken::Number(n) => Ok(TextElement::Adjustment(*n)),
            _ => Err(ContentStreamError::InvalidOperandValue {
                operator: operator.into(),
                operand_index: i,
                message: "expected string or number in text array".into(),
            }),
        })
        .collect()
}

pub(super) fn expect_properties(tokens: &[CsToken], operator: &str, index: usize) -> Result<Value> {
    if index >= tokens.len() {
        return Err(ContentStreamError::InvalidOperandCount {
            operator: operator.into(),
            expected: "at least 2".into(),
            actual: tokens.len(),
        });
    }
    token_to_value(&tokens[index])
}
