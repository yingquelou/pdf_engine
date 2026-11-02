use crate::core::reader::text::tokenizer::{CsToken, tokenize_content_stream};
use crate::core::types::{Array, Dict, Value};

/// Build a [`Dict`] from the raw inner bytes of a `<< ... >>` literal.
///
/// Used for inline property lists (BDC/DP, ISO 32000-1 14.6.2). Parsing is
/// lenient: a key without a parsable value is skipped, and unknown keywords
/// become names. Literal strings keep their raw bytes one-per-char so
/// `PdfTextString` can decode PDFDocEncoding or UTF-16BE later; hex strings
/// keep uppercase hex characters, matching the document parser's convention.
pub(crate) fn parse_dictionary_inner(inner: &[u8]) -> Dict {
    let tokens = tokenize_content_stream(inner);
    let mut dict = Dict::new();
    let mut pos = 0;
    while pos < tokens.len() {
        let CsToken::Name(key) = &tokens[pos] else {
            pos += 1;
            continue;
        };
        let key = key.clone();
        pos += 1;
        let Some(value) = take_dictionary_value(&tokens, &mut pos) else {
            continue;
        };
        dict = dict.insert(key, value);
    }
    dict
}

/// Read one value starting at `*pos`, advancing past it.
fn take_dictionary_value(tokens: &[CsToken], pos: &mut usize) -> Option<Value> {
    // `num gen R` indirect reference form.
    if let (
        Some(CsToken::Number(num)),
        Some(CsToken::Number(generation)),
        Some(CsToken::Keyword(k)),
    ) = (tokens.get(*pos), tokens.get(*pos + 1), tokens.get(*pos + 2))
        && k == "R"
    {
        *pos += 3;
        return Some(Value::Ref(*num as i64, *generation as i64));
    }
    let token = tokens.get(*pos)?;
    let value = match token {
        CsToken::Name(name) => Value::Name(name.clone()),
        CsToken::Keyword(k) => match k.as_str() {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            _ => return None,
        },
        CsToken::Number(n) => Value::Float(*n),
        CsToken::LiteralString(bytes) => Value::String(bytes.iter().map(|&b| b as char).collect()),
        CsToken::HexString(bytes) => {
            let mut hex = String::with_capacity(bytes.len() * 2);
            for byte in bytes {
                use std::fmt::Write as _;
                let _ = write!(hex, "{byte:02X}");
            }
            Value::HexString(hex)
        }
        CsToken::ArrayOpen => {
            *pos += 1;
            let mut items = Vec::new();
            while let Some(t) = tokens.get(*pos) {
                if matches!(t, CsToken::ArrayClose) {
                    *pos += 1;
                    break;
                }
                if let (
                    Some(CsToken::Number(object)),
                    Some(CsToken::Number(generation)),
                    Some(CsToken::Keyword(keyword)),
                ) = (tokens.get(*pos), tokens.get(*pos + 1), tokens.get(*pos + 2))
                    && keyword == "R"
                {
                    items.push(Value::Ref(*object as i64, *generation as i64));
                    *pos += 3;
                    continue;
                }
                if let Some(v) = take_array_element(t) {
                    items.push(v);
                }
                *pos += 1;
            }
            return Some(Value::Array(Array(items)));
        }
        CsToken::Dictionary(inner) => {
            *pos += 1;
            return Some(Value::Dict(parse_dictionary_inner(inner)));
        }
        CsToken::ArrayClose | CsToken::InlineImage { .. } => return None,
    };
    *pos += 1;
    Some(value)
}

fn take_array_element(token: &CsToken) -> Option<Value> {
    match token {
        CsToken::Number(n) => Some(Value::Float(*n)),
        CsToken::LiteralString(bytes) => {
            Some(Value::String(bytes.iter().map(|&b| b as char).collect()))
        }
        CsToken::HexString(bytes) => {
            let mut hex = String::with_capacity(bytes.len() * 2);
            for byte in bytes {
                use std::fmt::Write as _;
                let _ = write!(hex, "{byte:02X}");
            }
            Some(Value::HexString(hex))
        }
        CsToken::Name(n) => Some(Value::Name(n.clone())),
        CsToken::Keyword(k) if k == "true" => Some(Value::Bool(true)),
        CsToken::Keyword(k) if k == "false" => Some(Value::Bool(false)),
        CsToken::Dictionary(inner) => Some(Value::Dict(parse_dictionary_inner(inner))),
        _ => None,
    }
}
