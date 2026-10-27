//! 读端内容操作符与写端 [`content::Op`] 的双向映射及无损序列化。
//!
//! 三件套：
//! - [`to_content_op`]：读端 `Op` → 写端 `Op`，写端模型无法表达的
//!   操作符（`TD` 双操作数、`d0`/`d1`、`BX`/`EX`、内嵌图像、未建模
//!   透传、越界的 `J`/`j`/`Tr`、非 UTF-8 的 `'`/`"`/`TJ` 串）返回
//!   [`OpMappingError`]；
//! - [`from_content_op`]：写端 `Op` → 读端 `Op`，总是成功；
//!   `MoveNextLineSetLeading`（单操作数 `TD`）按有损方式映射；
//! - [`serialize_ops`]：把读端 `Op` 序列化为可被
//!   [`parse_content_stream`](super::parse_content_stream) 重新解析的
//!   字节——可映射的操作符走写端序列化器，其余走原始回退，保证
//!   `parse → serialize → parse` 模型级往返。

use crate::core::content::{
    ContentStream, LineCap, LineJoin, MarkedContentProperties, Op as WriteOp, TextArrayItem,
};
use crate::core::types::Value;
use crate::core::writer::{format_float, write_value};

use super::parser::hex_upper;
use super::{InlineImage, Op, TextElement};

/// 读端操作符无法由写端 `content::Op` 模型表达时的错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OpMappingError {
    /// 写端模型没有对应变体（或该变体的字节形式不合规范）。
    NotRepresentableByWriter { operator: String, reason: String },
    /// 数值操作数超出写端变体的可表示范围（`J`/`j` 非 0–2、`Tr` 非 0–255）。
    OperandOutOfRange { operator: String, value: i64 },
}

impl std::fmt::Display for OpMappingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotRepresentableByWriter { operator, reason } => {
                write!(
                    f,
                    "operator '{operator}' is not representable by writer Op: {reason}"
                )
            }
            Self::OperandOutOfRange { operator, value } => {
                write!(
                    f,
                    "operator '{operator}' operand {value} is out of writer range"
                )
            }
        }
    }
}

impl std::error::Error for OpMappingError {}

fn not_representable(operator: &str, reason: impl Into<String>) -> OpMappingError {
    OpMappingError::NotRepresentableByWriter {
        operator: operator.into(),
        reason: reason.into(),
    }
}

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

/// 把读端 `Op` 序列化为可重新解析的 PDF 内容流字节。
///
/// 可映射到写端模型的操作符复用 `ContentStream` 序列化器；其余
/// （TD/d0/d1/BX/EX/未建模/内嵌图像/越界值/非 UTF-8 串）走原始回退，
/// 保证 `parse_content_stream(&serialize_ops(ops))` 与输入模型相等。
/// `InlineImage` 以 `BI … ID <data> EI` 语法回写，数据字节原样保留。
pub fn serialize_ops(ops: &[Op]) -> Vec<u8> {
    let mut out = Vec::new();
    for op in ops {
        serialize_op(op, &mut out);
    }
    out
}

fn serialize_op(op: &Op, out: &mut Vec<u8>) {
    match to_content_op(op) {
        Ok(mapped) => {
            let mut stream = ContentStream::new();
            stream.push(mapped);
            out.extend(stream.to_bytes());
        }
        Err(_) => serialize_raw(op, out),
    }
}

/// 原始回退：直接按 PDF 语法写出读端模型无法映射的操作符。
fn serialize_raw(op: &Op, out: &mut Vec<u8>) {
    match op {
        Op::MoveTextPositionAndSetLeading(tx, ty) => {
            push_num(out, *tx);
            push_num(out, *ty);
            out.extend(b"TD\n");
        }
        Op::SetLineCap(n) => {
            push_int(out, *n);
            out.extend(b" J\n");
        }
        Op::SetLineJoin(n) => {
            push_int(out, *n);
            out.extend(b" j\n");
        }
        Op::SetTextRenderingMode(mode) => {
            push_int(out, *mode);
            out.extend(b" Tr\n");
        }
        Op::NextLineAndShowText(bytes) => {
            push_string_operand(out, bytes);
            out.extend(b" '\n");
        }
        Op::SetSpacingAndShowText(aw, ac, bytes) => {
            push_num(out, *aw);
            push_num(out, *ac);
            push_string_operand(out, bytes);
            out.extend(b" \"\n");
        }
        Op::ShowTextWithPositioning(elements) => {
            out.push(b'[');
            for element in elements {
                out.push(b' ');
                match element {
                    TextElement::String(bytes) => push_string_operand(out, bytes),
                    TextElement::Adjustment(v) => push_num(out, *v),
                }
            }
            out.extend(b" ] TJ\n");
        }
        Op::SetGlyphWidth(v, l) => {
            push_num(out, *v);
            push_num(out, *l);
            out.extend(b"d0\n");
        }
        Op::SetGlyphWidthAndBBox(v, l, ll, lb, ur, ub) => {
            for n in [*v, *l, *ll, *lb, *ur, *ub] {
                push_num(out, n);
            }
            out.extend(b"d1\n");
        }
        Op::BeginCompatibility => out.extend(b"BX\n"),
        Op::EndCompatibility => out.extend(b"EX\n"),
        Op::MarkedContentPointWithProperties(tag, props) => {
            push_marked_properties(out, tag, props, b"DP");
        }
        Op::BeginMarkedContentWithProperties(tag, props) => {
            push_marked_properties(out, tag, props, b"BDC");
        }
        Op::Unmodeled { operator, operands } => {
            let mut text = String::new();
            for value in operands {
                text.push(' ');
                write_value(value, &mut text);
            }
            text.push(' ');
            text.push_str(operator);
            text.push('\n');
            out.extend(text.as_bytes());
        }
        Op::InlineImage(image) => serialize_inline_image(image, out),
        // 其余变体都可映射走写端序列化器；这里兜底以防调用顺序变化。
        other => serialize_op(other, out),
    }
}

fn push_marked_properties(out: &mut Vec<u8>, tag: &str, props: &Value, keyword: &[u8]) {
    let mut text = String::new();
    write_value(&Value::Name(tag.to_owned()), &mut text);
    text.push(' ');
    write_value(props, &mut text);
    text.push(' ');
    text.push_str(std::str::from_utf8(keyword).unwrap_or("BDC"));
    text.push('\n');
    out.extend(text.as_bytes());
}

/// 内嵌图像输出（`BI ... ID <data> EI`）。
///
/// 字典按模型顺序写出，数据字节原样跟随；分词器读取时会剥掉 `ID`
/// 后的一个分隔空白与 `EI` 前的一个分隔空白，因此这两处由本函数补齐。
fn serialize_inline_image(image: &InlineImage, out: &mut Vec<u8>) {
    out.extend(b"BI");
    for (key, value) in &image.dict {
        out.push(b' ');
        let mut text = String::new();
        write_value(&Value::Name(key.clone()), &mut text);
        text.push(' ');
        write_value(value, &mut text);
        out.extend(text.as_bytes());
    }
    out.extend(b" ID\n");
    out.extend(&image.data);
    out.extend(b"\nEI\n");
}

fn push_num(out: &mut Vec<u8>, v: f64) {
    out.extend(format_float(v).as_bytes());
    out.push(b' ');
}

fn push_int(out: &mut Vec<u8>, v: i64) {
    out.extend(v.to_string().as_bytes());
    out.push(b' ');
}

/// 写出字符串操作数：合法 UTF-8 用字面量形式（`( )` 转义），
/// 否则用十六进制形式，两者都能被分词器还原为原始字节。
fn push_string_operand(out: &mut Vec<u8>, bytes: &[u8]) {
    if std::str::from_utf8(bytes).is_ok() {
        out.push(b'(');
        for &b in bytes {
            match b {
                b'(' | b')' | b'\\' => {
                    out.push(b'\\');
                    out.push(b);
                }
                b'\n' => out.extend(b"\\n"),
                b'\r' => out.extend(b"\\r"),
                b'\t' => out.extend(b"\\t"),
                _ => out.push(b),
            }
        }
        out.push(b')');
    } else {
        out.push(b'<');
        out.extend(hex_upper(bytes).as_bytes());
        out.push(b'>');
    }
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
        other => Err(not_representable(
            operator,
            format!("property operand must be a name or inline dict, got {other:?}"),
        )),
    }
}

fn from_properties(properties: &MarkedContentProperties) -> Value {
    match properties {
        MarkedContentProperties::Name(name) => Value::Name(name.clone()),
        MarkedContentProperties::Dict(dict) => Value::Dict(dict.clone()),
    }
}
