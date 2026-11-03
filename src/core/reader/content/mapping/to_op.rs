use super::*;

/// 把读端 `Op` 映射为写端 `content::Op`。
pub fn to_content_op(op: &Op) -> Result<WriteOp, OpMappingError> {
    Ok(match op {
        Op::SaveState => WriteOp::SaveState,
        Op::RestoreState => WriteOp::RestoreState,
        Op::ConcatMatrix(m) => WriteOp::ConcatMatrix(*m),
        Op::SetLineWidth(v) => WriteOp::SetLineWidth(*v),
        Op::SetLineCap(n) => WriteOp::SetLineCap(line_cap(*n)?),
        Op::SetLineJoin(n) => WriteOp::SetLineJoin(line_join(*n)?),
        Op::SetMiterLimit(v) => WriteOp::SetMiterLimit(*v),
        Op::SetDashPattern(array, phase) => WriteOp::SetDashPattern {
            array: array.clone(),
            phase: *phase,
        },
        Op::SetRenderingIntent(name) => WriteOp::SetRenderingIntent(name.clone()),
        Op::SetFlatness(v) => WriteOp::SetFlatness(*v),
        Op::SetExtGState(name) => WriteOp::SetExtGState(name.clone()),

        Op::MoveTo(x, y) => WriteOp::MoveTo { x: *x, y: *y },
        Op::LineTo(x, y) => WriteOp::LineTo { x: *x, y: *y },
        Op::CurveTo(x1, y1, x2, y2, x3, y3) => WriteOp::CurveTo {
            x1: *x1,
            y1: *y1,
            x2: *x2,
            y2: *y2,
            x3: *x3,
            y3: *y3,
        },
        Op::CurveToReplicateInitial(x2, y2, x3, y3) => WriteOp::CurveToInitial {
            x2: *x2,
            y2: *y2,
            x3: *x3,
            y3: *y3,
        },
        Op::CurveToReplicateFinal(x1, y1, x3, y3) => WriteOp::CurveToFinal {
            x1: *x1,
            y1: *y1,
            x3: *x3,
            y3: *y3,
        },
        Op::CloseSubpath => WriteOp::ClosePath,
        Op::Rectangle(x, y, w, h) => WriteOp::Rectangle {
            x: *x,
            y: *y,
            w: *w,
            h: *h,
        },

        Op::Stroke => WriteOp::Stroke,
        Op::CloseAndStroke => WriteOp::ClosePathStroke,
        Op::Fill => WriteOp::Fill,
        Op::FillObsolete => WriteOp::FillRule,
        Op::FillEvenOdd => WriteOp::FillEvenOdd,
        Op::FillAndStroke => WriteOp::FillStroke,
        Op::FillAndStrokeEvenOdd => WriteOp::FillStrokeEvenOdd,
        Op::CloseFillAndStroke => WriteOp::ClosePathFillStroke,
        Op::CloseFillAndStrokeEvenOdd => WriteOp::ClosePathFillStrokeEvenOdd,
        Op::EndPath => WriteOp::EndPath,

        Op::Clip => WriteOp::Clip,
        Op::ClipEvenOdd => WriteOp::ClipEvenOdd,

        Op::BeginText => WriteOp::BeginText,
        Op::EndText => WriteOp::EndText,

        Op::SetCharSpacing(v) => WriteOp::SetCharSpacing(*v),
        Op::SetWordSpacing(v) => WriteOp::SetWordSpacing(*v),
        Op::SetHorizontalScaling(v) => WriteOp::SetTextScale(*v),
        Op::SetTextLeading(v) => WriteOp::SetTextLeading(*v),
        Op::SetFont(name, size) => WriteOp::SetFont {
            name: name.clone(),
            size: *size,
        },
        Op::SetTextRenderingMode(mode) => {
            if (0..=255).contains(mode) {
                WriteOp::SetTextRender(*mode as u8)
            } else {
                return Err(OpMappingError::OperandOutOfRange {
                    operator: "Tr".into(),
                    value: *mode,
                });
            }
        }
        Op::SetTextRise(v) => WriteOp::SetTextRise(*v),

        Op::MoveTextPosition(tx, ty) => WriteOp::MoveNextLine { tx: *tx, ty: *ty },
        Op::MoveTextPositionAndSetLeading(..) => {
            return Err(not_representable(
                "TD",
                "writer MoveNextLineSetLeading emits a single TD operand (spec-invalid); \
                 serialized raw instead",
            ));
        }
        Op::SetTextMatrix(m) => WriteOp::SetTextMatrix(*m),
        Op::NextLine => WriteOp::NextLine,

        Op::ShowText(bytes) => match std::str::from_utf8(bytes) {
            Ok(s) => WriteOp::ShowText(s.to_owned()),
            Err(_) => WriteOp::ShowTextHex(bytes.clone()),
        },
        Op::NextLineAndShowText(bytes) => WriteOp::ShowTextNextLine(utf8_text("'", bytes)?),
        Op::SetSpacingAndShowText(aw, ac, bytes) => WriteOp::SetSpacingShowTextNextLine {
            word_spacing: *aw,
            char_spacing: *ac,
            text: utf8_text("\"", bytes)?,
        },
        Op::ShowTextWithPositioning(elements) => {
            let mut items = Vec::with_capacity(elements.len());
            for element in elements {
                items.push(match element {
                    TextElement::String(bytes) => TextArrayItem::String(utf8_text("TJ", bytes)?),
                    TextElement::Adjustment(v) => TextArrayItem::MoveBy(*v),
                });
            }
            WriteOp::ShowTextArray(items)
        }

        Op::SetGlyphWidth(..) | Op::SetGlyphWidthAndBBox(..) => {
            return Err(not_representable(
                "d0/d1",
                "writer Op models no Type 3 glyph-width operators",
            ));
        }

        Op::SetStrokeColorSpace(name) => WriteOp::SetStrokeColorSpace(name.clone()),
        Op::SetFillColorSpace(name) => WriteOp::SetFillColorSpace(name.clone()),
        Op::SetStrokeColor(values) => WriteOp::SetStrokeColor(values.clone()),
        Op::SetStrokeColorN(values) => WriteOp::SetStrokeColorN {
            values: values.clone(),
        },
        Op::SetFillColor(values) => WriteOp::SetFillColor(values.clone()),
        Op::SetFillColorN(values) => WriteOp::SetFillColorN {
            values: values.clone(),
        },
        Op::SetStrokeColorPattern(name) => WriteOp::SetStrokeColorPattern(name.clone()),
        Op::SetStrokeColorPatternWithComponents(values, name) => {
            WriteOp::SetStrokeColorPatternWithComponents {
                values: values.clone(),
                name: name.clone(),
            }
        }
        Op::SetFillColorPattern(name) => WriteOp::SetFillColorPattern(name.clone()),
        Op::SetFillColorPatternWithComponents(values, name) => {
            WriteOp::SetFillColorPatternWithComponents {
                values: values.clone(),
                name: name.clone(),
            }
        }
        Op::SetStrokeGray(v) => WriteOp::SetStrokeGray { gray: *v },
        Op::SetFillGray(v) => WriteOp::SetFillGray { gray: *v },
        Op::SetStrokeRgb(r, g, b) => WriteOp::SetStrokeRGB {
            r: *r,
            g: *g,
            b: *b,
        },
        Op::SetFillRgb(r, g, b) => WriteOp::SetFillRGB {
            r: *r,
            g: *g,
            b: *b,
        },
        Op::SetStrokeCmyk(c, m, y, k) => WriteOp::SetStrokeCMYK {
            c: *c,
            m: *m,
            y: *y,
            k: *k,
        },
        Op::SetFillCmyk(c, m, y, k) => WriteOp::SetFillCMYK {
            c: *c,
            m: *m,
            y: *y,
            k: *k,
        },

        Op::PaintShading(name) => WriteOp::PaintShading(name.clone()),

        Op::InlineImage(_) => {
            return Err(not_representable(
                "BI",
                "writer Op models no inline images; the tokenizer also skips BI..EI, \
                 so a parsed stream never yields this variant",
            ));
        }

        Op::PaintXObject(name) => WriteOp::DrawXObject(name.clone()),

        Op::MarkedContentPoint(tag) => WriteOp::MarkedContentPoint { tag: tag.clone() },
        Op::MarkedContentPointWithProperties(tag, props) => WriteOp::MarkedContentPointProperties {
            tag: tag.clone(),
            properties: to_properties("DP", props)?,
        },
        Op::BeginMarkedContent(tag) => WriteOp::BeginMarkedContent { tag: tag.clone() },
        Op::BeginMarkedContentWithProperties(tag, props) => WriteOp::BeginMarkedContentProperties {
            tag: tag.clone(),
            properties: to_properties("BDC", props)?,
        },
        Op::EndMarkedContent => WriteOp::EndMarkedContent,

        Op::BeginCompatibility | Op::EndCompatibility => {
            return Err(not_representable(
                "BX/EX",
                "writer Op models no compatibility operators",
            ));
        }

        Op::Unmodeled { operator, .. } => {
            return Err(not_representable(
                operator,
                "operator is not modeled by writer Op; serialized raw for roundtrip",
            ));
        }
    })
}

fn utf8_text(operator: &str, bytes: &[u8]) -> Result<String, OpMappingError> {
    std::str::from_utf8(bytes).map(Into::into).map_err(|_| {
        not_representable(
            operator,
            "string operand is not valid UTF-8 and the writer has no hex form for it",
        )
    })
}

fn line_cap(n: i64) -> Result<LineCap, OpMappingError> {
    match n {
        0 => Ok(LineCap::Butt),
        1 => Ok(LineCap::Round),
        2 => Ok(LineCap::Square),
        other => Err(OpMappingError::OperandOutOfRange {
            operator: "J".into(),
            value: other,
        }),
    }
}

fn line_join(n: i64) -> Result<LineJoin, OpMappingError> {
    match n {
        0 => Ok(LineJoin::Miter),
        1 => Ok(LineJoin::Round),
        2 => Ok(LineJoin::Bevel),
        other => Err(OpMappingError::OperandOutOfRange {
            operator: "j".into(),
            value: other,
        }),
    }
}

fn to_properties(operator: &str, value: &Value) -> Result<MarkedContentProperties, OpMappingError> {
    match value {
        Value::Name(name) => Ok(MarkedContentProperties::Name(name.clone())),
        Value::Dict(dict) => Ok(MarkedContentProperties::Dict(dict.clone())),
        // 间接引用属性表（`/Tag 5 0 R BDC`）：写端模型可表达，故映射后仍能
        // 走写端序列化器回写 `obj gen R`，不必退回原始字节。
        Value::Ref(object, generation) => Ok(MarkedContentProperties::Reference {
            object: *object,
            generation: *generation,
        }),
        other => Err(not_representable(
            operator,
            format!(
                "property operand must be a name, inline dict or indirect reference, got {other:?}"
            ),
        )),
    }
}
