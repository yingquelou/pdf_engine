use super::dict::parse_dictionary_inner;
use super::error::{ContentStreamError, Result};
use crate::core::reader::text::tokenizer::CsToken;
use crate::core::types::{Array, Value};

/// Convert a run of operand tokens into PDF values, coalescing structure.
///
/// Used for unmodeled operators so their operands survive a
/// parse → serialize → parse roundtrip: `[ ... ]` spans (with nesting)
/// become [`Value::Array`], `num gen R` triplets become [`Value::Ref`],
/// and `true`/`false`/`null` keywords become their typed values.
pub(super) fn tokens_to_values(tokens: &[CsToken]) -> Result<Vec<Value>> {
    let mut values = Vec::new();
    let mut pos = 0;
    while pos < tokens.len() {
        if let (
            Some(CsToken::Number(num)),
            Some(CsToken::Number(generation)),
            Some(CsToken::Keyword(kw)),
        ) = (tokens.get(pos), tokens.get(pos + 1), tokens.get(pos + 2))
            && kw == "R"
        {
            values.push(Value::Ref(*num as i64, *generation as i64));
            pos += 3;
            continue;
        }
        if matches!(tokens[pos], CsToken::ArrayOpen) {
            let end = array_close_index(tokens, pos).unwrap_or(tokens.len());
            values.push(Value::Array(Array(tokens_to_values(
                &tokens[pos + 1..end.min(tokens.len())],
            )?)));
            pos = end + 1;
            continue;
        }
        values.push(token_to_value(&tokens[pos])?);
        pos += 1;
    }
    Ok(values)
}

/// Index of the `ArrayClose` matching the `ArrayOpen` at `open`.
fn array_close_index(tokens: &[CsToken], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut pos = open;
    while pos < tokens.len() {
        match &tokens[pos] {
            CsToken::ArrayOpen => depth += 1,
            CsToken::ArrayClose => {
                depth -= 1;
                if depth == 0 {
                    return Some(pos);
                }
            }
            _ => {}
        }
        pos += 1;
    }
    None
}

pub(super) fn token_to_value(token: &CsToken) -> Result<Value> {
    Ok(match token {
        CsToken::Number(n) => Value::Float(*n),
        CsToken::LiteralString(s) => Value::String(s.iter().map(|&b| b as char).collect()),
        CsToken::HexString(s) => Value::HexString(hex_upper(s)),
        CsToken::Name(n) => Value::Name(n.clone()),
        CsToken::Keyword(k) => match k.as_str() {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            "null" => Value::Null,
            _ => Value::Name(k.clone()),
        },
        CsToken::ArrayOpen => Value::Array(Default::default()),
        CsToken::ArrayClose => Value::Array(Default::default()),
        CsToken::Dictionary(inner) => Value::Dict(parse_dictionary_inner(inner)),
        // Inline images are consumed as a whole by `parse_content_stream` and
        // cannot appear as an operand of another operator.
        CsToken::InlineImage { .. } => return Err(ContentStreamError::MalformedInlineImage),
    })
}

/// Render bytes as uppercase hex pairs, matching the document parser.
pub(crate) fn hex_upper(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(hex, "{byte:02X}");
    }
    hex
}
