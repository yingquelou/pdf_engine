//! Content stream parser.
//!
//! Converts a byte stream into a sequence of [`Op`] values, handling inline
//! images specially and preserving unmodeled operators for roundtrip.

use super::{InlineImage, Op, TextElement};
use crate::core::reader::text::tokenizer::{CsToken, tokenize_content_stream};
use crate::core::types::Value;

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
        // Check for inline image (BI ... ID <data> EI)
        if let Some(CsToken::Keyword(kw)) = tokens.get(pos) {
            if kw == "BI" {
                let (image, next_pos) = parse_inline_image(&tokens, pos, input)?;
                ops.push(Op::InlineImage(image));
                pos = next_pos;
                continue;
            }
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

fn parse_inline_image(
    tokens: &[CsToken],
    start: usize,
    input: &[u8],
) -> Result<(InlineImage, usize)> {
    // Skip BI
    let mut pos = start + 1;

    // Parse dictionary entries until ID
    let mut dict = Vec::new();
    while pos < tokens.len() {
        match &tokens[pos] {
            CsToken::Keyword(kw) if kw == "ID" => {
                pos += 1;
                break;
            }
            CsToken::Name(key) => {
                let key = key.clone();
                pos += 1;
                if pos >= tokens.len() {
                    return Err(ContentStreamError::MalformedInlineImage);
                }
                let value = token_to_value(&tokens[pos])?;
                dict.push((key, value));
                pos += 1;
            }
            _ => {
                return Err(ContentStreamError::MalformedInlineImage);
            }
        }
    }

    // Find the raw data between ID and EI in the original input
    // We need to locate the ID keyword in the input, then find EI
    let id_pos = find_keyword_in_input(input, b"ID", start);
    let ei_pos = find_keyword_in_input(input, b"EI", pos);

    let data = if let (Some(id), Some(ei)) = (id_pos, ei_pos) {
        // Data starts after "ID " (3 bytes) and ends before " EI"
        let data_start = id + 3;
        let data_end = ei;
        if data_start <= data_end && data_end <= input.len() {
            input[data_start..data_end].to_vec()
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    // Skip past EI
    pos += 1;

    Ok((InlineImage { dict, data }, pos))
}

fn find_keyword_in_input(input: &[u8], keyword: &[u8], _after_token: usize) -> Option<usize> {
    // Simple search for the keyword in the input
    // This is a best-effort approach; inline images are tricky because the data
    // can contain arbitrary bytes
    let mut pos = 0;
    while pos + keyword.len() < input.len() {
        if &input[pos..pos + keyword.len()] == keyword {
            // Check that it's surrounded by whitespace or delimiters
            let before_ok = pos == 0 || is_ws_or_delimiter(input[pos - 1]);
            let after_ok = pos + keyword.len() >= input.len()
                || is_ws_or_delimiter(input[pos + keyword.len()]);
            if before_ok && after_ok {
                return Some(pos);
            }
        }
        pos += 1;
    }
    None
}

fn is_ws_or_delimiter(b: u8) -> bool {
    matches!(
        b,
        b' ' | b'\n' | b'\r' | b'\t' | b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'/' | b'%'
    )
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
            if operands.len() != 2 {
                return Err(ContentStreamError::InvalidOperandCount {
                    operator: "d".into(),
                    expected: "2".into(),
                    actual: operands.len(),
                });
            }
            let dash_array = expect_number_array(&operands[0], "d", 0)?;
            let phase = expect_number(&operands[1], "d", 1)?;
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
        "Th" => Ok(Op::SetHorizontalScaling(expect_number(
            &operands[0],
            "Th",
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
        "SCN" => Ok(Op::SetStrokeColorN(expect_numbers_variadic(
            operands, "SCN",
        )?)),
        "sc" => Ok(Op::SetFillColor(expect_numbers_variadic(operands, "sc")?)),
        "scn" => Ok(Op::SetFillColorN(expect_numbers_variadic(operands, "scn")?)),
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
            operands: operands
                .iter()
                .map(token_to_value)
                .collect::<Result<Vec<_>>>()?,
        }),
    }
}

fn token_to_value(token: &CsToken) -> Result<Value> {
    match token {
        CsToken::Number(n) => Ok(Value::Float(*n)),
        CsToken::LiteralString(s) => Ok(Value::String(String::from_utf8_lossy(s).into_owned())),
        CsToken::HexString(s) => Ok(Value::HexString(String::from_utf8_lossy(s).into_owned())),
        CsToken::Name(n) => Ok(Value::Name(n.clone())),
        CsToken::Keyword(k) => Ok(Value::Name(k.clone())),
        CsToken::ArrayOpen => Ok(Value::Array(Default::default())),
        CsToken::ArrayClose => Ok(Value::Array(Default::default())),
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

fn expect_number_array(_token: &CsToken, operator: &str, index: usize) -> Result<Vec<f64>> {
    // For now, we don't support arrays in the tokenizer output
    // This would need to be extended to parse array tokens
    Err(ContentStreamError::InvalidOperandValue {
        operator: operator.into(),
        operand_index: index,
        message: "array operands not yet supported".into(),
    })
}

fn expect_text_array(
    _tokens: &[CsToken],
    operator: &str,
    index: usize,
) -> Result<Vec<TextElement>> {
    // Similar to above, arrays need special handling
    Err(ContentStreamError::InvalidOperandValue {
        operator: operator.into(),
        operand_index: index,
        message: "array operands not yet supported".into(),
    })
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
