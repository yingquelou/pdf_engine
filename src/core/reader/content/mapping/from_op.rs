use super::*;

/// 把写端 `content::Op` 映射回读端 `Op`（总是成功）。
///
/// 唯一的有损点：[`WriteOp::MoveNextLineSetLeading`] 只携带一个操作数
/// （不合规范的 `TD`），这里把它当作 `ty` 分量、`tx` 取 0。
pub fn from_content_op(op: &WriteOp) -> Op {
    match op {
        WriteOp::BeginText => Op::BeginText,
        WriteOp::EndText => Op::EndText,
        WriteOp::SetFont { name, size } => Op::SetFont(name.clone(), *size),
        WriteOp::ShowText(s) => Op::ShowText(s.clone().into_bytes()),
        WriteOp::ShowTextHex(bytes) => Op::ShowText(bytes.clone()),
        WriteOp::ShowTextArray(items) => Op::ShowTextWithPositioning(
            items
                .iter()
                .map(|item| match item {
                    TextArrayItem::String(s) => TextElement::String(s.clone().into_bytes()),
                    TextArrayItem::MoveBy(v) => TextElement::Adjustment(*v),
                })
                .collect(),
        ),
        WriteOp::ShowTextNextLine(s) => Op::NextLineAndShowText(s.clone().into_bytes()),
        WriteOp::SetSpacingShowTextNextLine {
            word_spacing,
            char_spacing,
            text,
        } => Op::SetSpacingAndShowText(*word_spacing, *char_spacing, text.clone().into_bytes()),
        WriteOp::MoveNextLine { tx, ty } => Op::MoveTextPosition(*tx, *ty),
        WriteOp::MoveNextLineSetLeading { leading } => {
            Op::MoveTextPositionAndSetLeading(0.0, *leading)
        }
        WriteOp::NextLine => Op::NextLine,
        WriteOp::SetTextMatrix(m) => Op::SetTextMatrix(*m),
        WriteOp::SetCharSpacing(v) => Op::SetCharSpacing(*v),
        WriteOp::SetWordSpacing(v) => Op::SetWordSpacing(*v),
        WriteOp::SetTextScale(v) => Op::SetHorizontalScaling(*v),
        WriteOp::SetTextLeading(v) => Op::SetTextLeading(*v),
        WriteOp::SetTextRender(v) => Op::SetTextRenderingMode(i64::from(*v)),
        WriteOp::SetTextRise(v) => Op::SetTextRise(*v),

        WriteOp::SaveState => Op::SaveState,
        WriteOp::RestoreState => Op::RestoreState,
        WriteOp::ConcatMatrix(m) => Op::ConcatMatrix(*m),
        WriteOp::SetLineWidth(v) => Op::SetLineWidth(*v),
        WriteOp::SetLineCap(v) => Op::SetLineCap(i64::from(v.value())),
        WriteOp::SetLineJoin(v) => Op::SetLineJoin(i64::from(v.value())),
        WriteOp::SetMiterLimit(v) => Op::SetMiterLimit(*v),
        WriteOp::SetDashPattern { array, phase } => Op::SetDashPattern(array.clone(), *phase),
        WriteOp::SetRenderingIntent(name) => Op::SetRenderingIntent(name.clone()),
        WriteOp::SetExtGState(name) => Op::SetExtGState(name.clone()),
        WriteOp::SetFlatness(v) => Op::SetFlatness(*v),

        WriteOp::MoveTo { x, y } => Op::MoveTo(*x, *y),
        WriteOp::LineTo { x, y } => Op::LineTo(*x, *y),
        WriteOp::CurveTo {
            x1,
            y1,
            x2,
            y2,
            x3,
            y3,
        } => Op::CurveTo(*x1, *y1, *x2, *y2, *x3, *y3),
        WriteOp::CurveToInitial { x2, y2, x3, y3 } => {
            Op::CurveToReplicateInitial(*x2, *y2, *x3, *y3)
        }
        WriteOp::CurveToFinal { x1, y1, x3, y3 } => Op::CurveToReplicateFinal(*x1, *y1, *x3, *y3),
        WriteOp::ClosePath => Op::CloseSubpath,
        WriteOp::Rectangle { x, y, w, h } => Op::Rectangle(*x, *y, *w, *h),

        WriteOp::Stroke => Op::Stroke,
        WriteOp::ClosePathStroke => Op::CloseAndStroke,
        WriteOp::Fill => Op::Fill,
        WriteOp::FillRule => Op::FillObsolete,
        WriteOp::FillEvenOdd => Op::FillEvenOdd,
        WriteOp::FillStroke => Op::FillAndStroke,
        WriteOp::FillStrokeEvenOdd => Op::FillAndStrokeEvenOdd,
        WriteOp::ClosePathFillStroke => Op::CloseFillAndStroke,
        WriteOp::ClosePathFillStrokeEvenOdd => Op::CloseFillAndStrokeEvenOdd,
        WriteOp::EndPath => Op::EndPath,

        WriteOp::Clip => Op::Clip,
        WriteOp::ClipEvenOdd => Op::ClipEvenOdd,

        WriteOp::SetStrokeGray { gray } => Op::SetStrokeGray(*gray),
        WriteOp::SetFillGray { gray } => Op::SetFillGray(*gray),
        WriteOp::SetStrokeRGB { r, g, b } => Op::SetStrokeRgb(*r, *g, *b),
        WriteOp::SetFillRGB { r, g, b } => Op::SetFillRgb(*r, *g, *b),
        WriteOp::SetStrokeCMYK { c, m, y, k } => Op::SetStrokeCmyk(*c, *m, *y, *k),
        WriteOp::SetFillCMYK { c, m, y, k } => Op::SetFillCmyk(*c, *m, *y, *k),
        WriteOp::SetStrokeColorSpace(name) => Op::SetStrokeColorSpace(name.clone()),
        WriteOp::SetFillColorSpace(name) => Op::SetFillColorSpace(name.clone()),
        WriteOp::SetStrokeColor(values) => Op::SetStrokeColor(values.clone()),
        WriteOp::SetFillColor(values) => Op::SetFillColor(values.clone()),
        WriteOp::SetStrokeColorN { values } => Op::SetStrokeColorN(values.clone()),
        WriteOp::SetFillColorN { values } => Op::SetFillColorN(values.clone()),
        WriteOp::SetStrokeColorPattern(name) => Op::SetStrokeColorPattern(name.clone()),
        WriteOp::SetFillColorPattern(name) => Op::SetFillColorPattern(name.clone()),
        WriteOp::SetStrokeColorPatternWithComponents { values, name } => {
            Op::SetStrokeColorPatternWithComponents(values.clone(), name.clone())
        }
        WriteOp::SetFillColorPatternWithComponents { values, name } => {
            Op::SetFillColorPatternWithComponents(values.clone(), name.clone())
        }

        WriteOp::BeginMarkedContent { tag } => Op::BeginMarkedContent(tag.clone()),
        WriteOp::BeginMarkedContentProperties { tag, properties } => {
            Op::BeginMarkedContentWithProperties(tag.clone(), from_properties(properties))
        }
        WriteOp::MarkedContentPoint { tag } => Op::MarkedContentPoint(tag.clone()),
        WriteOp::MarkedContentPointProperties { tag, properties } => {
            Op::MarkedContentPointWithProperties(tag.clone(), from_properties(properties))
        }
        WriteOp::EndMarkedContent => Op::EndMarkedContent,

        WriteOp::DrawXObject(name) => Op::PaintXObject(name.clone()),
        WriteOp::PaintShading(name) => Op::PaintShading(name.clone()),
    }
}

fn from_properties(properties: &MarkedContentProperties) -> Value {
    match properties {
        MarkedContentProperties::Name(name) => Value::Name(name.clone()),
        MarkedContentProperties::Dict(dict) => Value::Dict(dict.clone()),
        // 间接引用操作数保持 `Value::Ref`，与读端 `BDC`/`DP` 的引用形态对齐。
        MarkedContentProperties::Reference { object, generation } => {
            Value::Ref(*object, *generation)
        }
    }
}
