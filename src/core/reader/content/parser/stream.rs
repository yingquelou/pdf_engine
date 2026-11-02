use super::dict::parse_dictionary_inner;
use super::error::{ContentStreamError, Result};
use super::operand::{
    expect_integer, expect_name, expect_number, expect_number_array, expect_numbers_exact,
    expect_numbers_variadic, expect_properties, expect_string, expect_text_array,
};
use super::value::tokens_to_values;
use crate::core::reader::content::{InlineImage, Op};
use crate::core::reader::text::tokenizer::{CsToken, tokenize_content_stream};
use crate::core::types::Value;

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
