//! Content stream parser.
//!
//! Converts a byte stream into a sequence of [`Op`] values, handling inline
//! images specially and preserving unmodeled operators for roundtrip.

use super::{InlineImage, Op, TextElement};
use crate::core::reader::text::tokenizer::{CsToken, tokenize_content_stream};
use crate::core::types::{Array, Dict, Value};

/// Errors that can occur during content stream parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentStreamError {
    /// An operator received an unexpected number of operands.
    InvalidOperandCount {
        operator: String,
        expected: String,
        actual: usize,
    },
    /// An operand has an invalid value (e.g., out of range).
    InvalidOperandValue {
        operator: String,
        operand_index: usize,
        message: String,
    },
    /// Inline image structure is malformed (missing ID or EI).
    MalformedInlineImage,
}

impl std::fmt::Display for ContentStreamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidOperandCount {
                operator,
                expected,
                actual,
            } => write!(
                f,
                "operator '{}' expected {} operands, got {}",
                operator, expected, actual
            ),
            Self::InvalidOperandValue {
                operator,
                operand_index,
                message,
            } => write!(
                f,
                "operator '{}' operand {} is invalid: {}",
                operator, operand_index, message
            ),
            Self::MalformedInlineImage => write!(f, "malformed inline image (missing ID or EI)"),
        }
    }
}

impl std::error::Error for ContentStreamError {}

type Result<T> = std::result::Result<T, ContentStreamError>;

/// Parse a content stream into a sequence of operators.
pub fn parse_content_stream(input: &[u8]) -> Result<Vec<Op>> {
    let tokens = tokenize_content_stream(input);
    let mut ops = Vec::new();
    let mut pos = 0;

    while pos < tokens.len() {
        // Inline image (BI ... ID <data> EI); the tokenizer already split the
        // dictionary from the binary data and consumed the closing EI.
        if let Some(CsToken::InlineImage {
            dict,
            data,
            terminated,
        }) = tokens.get(pos)
        {
            if !terminated {
                return Err(ContentStreamError::MalformedInlineImage);
            }
            ops.push(Op::InlineImage(InlineImage {
                dict: inline_image_dict_entries(dict),
                data: data.clone(),
            }));
            pos += 1;
            continue;
        }

        // Collect operands until we hit an operator
        let mut operands = Vec::new();
        while pos < tokens.len() {
            if let CsToken::Keyword(kw) = &tokens[pos] {
                // This is the operator
                let op = parse_operator(kw, &operands)?;
                ops.push(op);
                pos += 1;
                break;
            } else {
                operands.push(tokens[pos].clone());
                pos += 1;
            }
        }
    }

    Ok(ops)
}

/// Parse the raw inline image dictionary bytes into ordered key/value pairs.
///
/// Inline image dictionaries use abbreviated keys (`/W`, `/H`, `/BPC`, `/CS`,
/// ...) and may contain arrays (`/D`, `/F`, `/DP`) and nested dictionaries
/// (`/DP`); [`parse_dictionary_inner`] already handles both.
fn inline_image_dict_entries(dict: &[u8]) -> Vec<(String, Value)> {
    parse_dictionary_inner(dict).0.into_iter().collect()
}

fn parse_operator(keyword: &str, operands: &[CsToken]) -> Result<Op> {
    match keyword {
        // Graphics state
        "q" => Ok(Op::SaveState),
        "Q" => Ok(Op::RestoreState),
        "cm" => {
            let nums = expect_numbers_exact(operands, "cm", 6)?;
            Ok(Op::ConcatMatrix([
                nums[0], nums[1], nums[2], nums[3], nums[4], nums[5],
            ]))
        }
        "w" => Ok(Op::SetLineWidth(expect_number(&operands[0], "w", 0)?)),
        "J" => Ok(Op::SetLineCap(expect_integer(&operands[0], "J", 0)?)),
        "j" => Ok(Op::SetLineJoin(expect_integer(&operands[0], "j", 0)?)),
        "M" => Ok(Op::SetMiterLimit(expect_number(&operands[0], "M", 0)?)),
        "d" => {
            let (dash_array, next) = expect_number_array(operands, "d", 0)?;
            if operands.len() != next + 1 {
                return Err(ContentStreamError::InvalidOperandCount {
                    operator: "d".into(),
                    expected: "2".into(),
                    actual: operands.len(),
                });
            }
            let phase = expect_number(&operands[next], "d", next)?;
            Ok(Op::SetDashPattern(dash_array, phase))
        }
        "ri" => Ok(Op::SetRenderingIntent(expect_name(&operands[0], "ri", 0)?)),
        "i" => Ok(Op::SetFlatness(expect_number(&operands[0], "i", 0)?)),
        "gs" => Ok(Op::SetExtGState(expect_name(&operands[0], "gs", 0)?)),

        // Path construction
        "m" => {
            let nums = expect_numbers_exact(operands, "m", 2)?;
            Ok(Op::MoveTo(nums[0], nums[1]))
        }
        "l" => {
            let nums = expect_numbers_exact(operands, "l", 2)?;
            Ok(Op::LineTo(nums[0], nums[1]))
        }
        "c" => {
            let nums = expect_numbers_exact(operands, "c", 6)?;
            Ok(Op::CurveTo(
                nums[0], nums[1], nums[2], nums[3], nums[4], nums[5],
            ))
        }
        "v" => {
            let nums = expect_numbers_exact(operands, "v", 4)?;
            Ok(Op::CurveToReplicateInitial(
                nums[0], nums[1], nums[2], nums[3],
            ))
        }
        "y" => {
            let nums = expect_numbers_exact(operands, "y", 4)?;
            Ok(Op::CurveToReplicateFinal(
                nums[0], nums[1], nums[2], nums[3],
            ))
        }
        "h" => Ok(Op::CloseSubpath),
        "re" => {
            let nums = expect_numbers_exact(operands, "re", 4)?;
            Ok(Op::Rectangle(nums[0], nums[1], nums[2], nums[3]))
        }

        // Path painting
        "S" => Ok(Op::Stroke),
        "s" => Ok(Op::CloseAndStroke),
        "f" => Ok(Op::Fill),
        "F" => Ok(Op::FillObsolete),
        "f*" => Ok(Op::FillEvenOdd),
        "B" => Ok(Op::FillAndStroke),
        "B*" => Ok(Op::FillAndStrokeEvenOdd),
        "b" => Ok(Op::CloseFillAndStroke),
        "b*" => Ok(Op::CloseFillAndStrokeEvenOdd),
        "n" => Ok(Op::EndPath),

        // Clipping
        "W" => Ok(Op::Clip),
        "W*" => Ok(Op::ClipEvenOdd),

        // Text object
        "BT" => Ok(Op::BeginText),
        "ET" => Ok(Op::EndText),

        // Text state
        "Tc" => Ok(Op::SetCharSpacing(expect_number(&operands[0], "Tc", 0)?)),
        "Tw" => Ok(Op::SetWordSpacing(expect_number(&operands[0], "Tw", 0)?)),
        "Tz" => Ok(Op::SetHorizontalScaling(expect_number(
            &operands[0],
            "Tz",
            0,
        )?)),
        "TL" => Ok(Op::SetTextLeading(expect_number(&operands[0], "TL", 0)?)),
        "Tf" => {
            let name = expect_name(&operands[0], "Tf", 0)?;
            let size = expect_number(&operands[1], "Tf", 1)?;
            Ok(Op::SetFont(name, size))
        }
        "Tr" => Ok(Op::SetTextRenderingMode(expect_integer(
            &operands[0],
            "Tr",
            0,
        )?)),
        "Ts" => Ok(Op::SetTextRise(expect_number(&operands[0], "Ts", 0)?)),

        // Text positioning
        "Td" => {
            let nums = expect_numbers_exact(operands, "Td", 2)?;
            Ok(Op::MoveTextPosition(nums[0], nums[1]))
        }
        "TD" => {
            let nums = expect_numbers_exact(operands, "TD", 2)?;
            Ok(Op::MoveTextPositionAndSetLeading(nums[0], nums[1]))
        }
        "Tm" => {
            let nums = expect_numbers_exact(operands, "Tm", 6)?;
            Ok(Op::SetTextMatrix([
                nums[0], nums[1], nums[2], nums[3], nums[4], nums[5],
            ]))
        }
        "T*" => Ok(Op::NextLine),

        // Text showing
        "Tj" => Ok(Op::ShowText(expect_string(&operands[0], "Tj", 0)?)),
        "'" => Ok(Op::NextLineAndShowText(expect_string(
            &operands[0],
            "'",
            0,
        )?)),
        "\"" => {
            let aw = expect_number(&operands[0], "\"", 0)?;
            let ac = expect_number(&operands[1], "\"", 1)?;
            let text = expect_string(&operands[2], "\"", 2)?;
            Ok(Op::SetSpacingAndShowText(aw, ac, text))
        }
        "TJ" => Ok(Op::ShowTextWithPositioning(expect_text_array(
            operands, "TJ", 0,
        )?)),

        // Type 3 fonts
        "d0" => {
            let nums = expect_numbers_exact(operands, "d0", 2)?;
            Ok(Op::SetGlyphWidth(nums[0], nums[1]))
        }
        "d1" => {
            let nums = expect_numbers_exact(operands, "d1", 6)?;
            Ok(Op::SetGlyphWidthAndBBox(
                nums[0], nums[1], nums[2], nums[3], nums[4], nums[5],
            ))
        }

        // Color
        "CS" => Ok(Op::SetStrokeColorSpace(expect_name(&operands[0], "CS", 0)?)),
        "cs" => Ok(Op::SetFillColorSpace(expect_name(&operands[0], "cs", 0)?)),
        "SC" => Ok(Op::SetStrokeColor(expect_numbers_variadic(operands, "SC")?)),
        "SCN" => {
            if let Some(CsToken::Name(pattern)) = operands.last() {
                let components = expect_numbers_variadic(&operands[..operands.len() - 1], "SCN")?;
                if components.is_empty() {
                    Ok(Op::SetStrokeColorPattern(pattern.clone()))
                } else {
                    Ok(Op::SetStrokeColorPatternWithComponents(
                        components,
                        pattern.clone(),
                    ))
                }
            } else {
                Ok(Op::SetStrokeColorN(expect_numbers_variadic(
                    operands, "SCN",
                )?))
            }
        }
        "sc" => Ok(Op::SetFillColor(expect_numbers_variadic(operands, "sc")?)),
        "scn" => {
            if let Some(CsToken::Name(pattern)) = operands.last() {
                let components = expect_numbers_variadic(&operands[..operands.len() - 1], "scn")?;
                if components.is_empty() {
                    Ok(Op::SetFillColorPattern(pattern.clone()))
                } else {
                    Ok(Op::SetFillColorPatternWithComponents(
                        components,
                        pattern.clone(),
                    ))
                }
            } else {
                Ok(Op::SetFillColorN(expect_numbers_variadic(operands, "scn")?))
            }
        }
        "G" => Ok(Op::SetStrokeGray(expect_number(&operands[0], "G", 0)?)),
        "g" => Ok(Op::SetFillGray(expect_number(&operands[0], "g", 0)?)),
        "RG" => {
            let nums = expect_numbers_exact(operands, "RG", 3)?;
            Ok(Op::SetStrokeRgb(nums[0], nums[1], nums[2]))
        }
        "rg" => {
            let nums = expect_numbers_exact(operands, "rg", 3)?;
            Ok(Op::SetFillRgb(nums[0], nums[1], nums[2]))
        }
        "K" => {
            let nums = expect_numbers_exact(operands, "K", 4)?;
            Ok(Op::SetStrokeCmyk(nums[0], nums[1], nums[2], nums[3]))
        }
        "k" => {
            let nums = expect_numbers_exact(operands, "k", 4)?;
            Ok(Op::SetFillCmyk(nums[0], nums[1], nums[2], nums[3]))
        }

        // Shading
        "sh" => Ok(Op::PaintShading(expect_name(&operands[0], "sh", 0)?)),

        // XObject
        "Do" => Ok(Op::PaintXObject(expect_name(&operands[0], "Do", 0)?)),

        // Marked content
        "MP" => Ok(Op::MarkedContentPoint(expect_name(&operands[0], "MP", 0)?)),
        "DP" => {
            let tag = expect_name(&operands[0], "DP", 0)?;
            let props = expect_properties(operands, "DP", 1)?;
            Ok(Op::MarkedContentPointWithProperties(tag, props))
        }
        "BMC" => Ok(Op::BeginMarkedContent(expect_name(&operands[0], "BMC", 0)?)),
        "BDC" => {
            let tag = expect_name(&operands[0], "BDC", 0)?;
            let props = expect_properties(operands, "BDC", 1)?;
            Ok(Op::BeginMarkedContentWithProperties(tag, props))
        }
        "EMC" => Ok(Op::EndMarkedContent),

        // Compatibility
        "BX" => Ok(Op::BeginCompatibility),
        "EX" => Ok(Op::EndCompatibility),

        // Unmodeled operator
        _ => Ok(Op::Unmodeled {
            operator: keyword.into(),
            operands: tokens_to_values(operands)?,
        }),
    }
}

/// Convert a run of operand tokens into PDF values, coalescing structure.
///
/// Used for unmodeled operators so their operands survive a
/// parse → serialize → parse roundtrip: `[ ... ]` spans (with nesting)
/// become [`Value::Array`], `num gen R` triplets become [`Value::Ref`],
/// and `true`/`false`/`null` keywords become their typed values.
fn tokens_to_values(tokens: &[CsToken]) -> Result<Vec<Value>> {
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

fn token_to_value(token: &CsToken) -> Result<Value> {
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

fn expect_number(token: &CsToken, operator: &str, index: usize) -> Result<f64> {
    match token {
        CsToken::Number(n) => Ok(*n),
        _ => Err(ContentStreamError::InvalidOperandValue {
            operator: operator.into(),
            operand_index: index,
            message: "expected number".into(),
        }),
    }
}

fn expect_integer(token: &CsToken, operator: &str, index: usize) -> Result<i64> {
    match token {
        CsToken::Number(n) => Ok(*n as i64),
        _ => Err(ContentStreamError::InvalidOperandValue {
            operator: operator.into(),
            operand_index: index,
            message: "expected integer".into(),
        }),
    }
}

fn expect_name(token: &CsToken, operator: &str, index: usize) -> Result<String> {
    match token {
        CsToken::Name(n) => Ok(n.clone()),
        _ => Err(ContentStreamError::InvalidOperandValue {
            operator: operator.into(),
            operand_index: index,
            message: "expected name".into(),
        }),
    }
}

fn expect_string(token: &CsToken, operator: &str, index: usize) -> Result<Vec<u8>> {
    match token {
        CsToken::LiteralString(s) | CsToken::HexString(s) => Ok(s.clone()),
        _ => Err(ContentStreamError::InvalidOperandValue {
            operator: operator.into(),
            operand_index: index,
            message: "expected string".into(),
        }),
    }
}

fn expect_numbers_exact(tokens: &[CsToken], operator: &str, count: usize) -> Result<Vec<f64>> {
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
        .map(|(i, t)| expect_number(t, operator, i))
        .collect()
}

fn expect_numbers_variadic(tokens: &[CsToken], operator: &str) -> Result<Vec<f64>> {
    tokens
        .iter()
        .enumerate()
        .map(|(i, t)| expect_number(t, operator, i))
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
fn expect_number_array(
    tokens: &[CsToken],
    operator: &str,
    index: usize,
) -> Result<(Vec<f64>, usize)> {
    let (inner, next) = take_array(tokens, operator, index)?;
    let values = inner
        .iter()
        .enumerate()
        .map(|(i, t)| expect_number(t, operator, i))
        .collect::<Result<Vec<_>>>()?;
    Ok((values, next))
}

fn expect_text_array(tokens: &[CsToken], operator: &str, index: usize) -> Result<Vec<TextElement>> {
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

fn expect_properties(tokens: &[CsToken], operator: &str, index: usize) -> Result<Value> {
    if index >= tokens.len() {
        return Err(ContentStreamError::InvalidOperandCount {
            operator: operator.into(),
            expected: "at least 2".into(),
            actual: tokens.len(),
        });
    }
    token_to_value(&tokens[index])
}
