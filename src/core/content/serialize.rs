//! 操作符到内容流字节的序列化。

use crate::core::types::Value;
use crate::core::writer::format_float;

use super::stream::ContentStream;
use super::types::{MarkedContentProperties, Op, TextArrayItem};

/// 内容流序列化失败。
///
/// 目前只有一种失败：标记内容属性表仍是「注册句柄」的未定型占位，却没有被
/// 装配层替换成实际对象号引用（即占位形态绕过了 `assemble` 直达序列化）。
/// 本库选择**类型化拒绝**而不是输出 `<句柄> -1 R` 这种假引用，让未接线的通路
/// 必然可见。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentWriteError {
    /// 属性表句柄未解析（`generation` 仍是
    /// [`MarkedContentProperties::PENDING_GENERATION`]）。
    UnresolvedPropertiesHandle {
        /// 注册时返回、但从未被替换成对象号的句柄。
        handle: usize,
    },
}

impl std::fmt::Display for ContentWriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnresolvedPropertiesHandle { handle } => write!(
                f,
                "marked-content property list handle {handle} was never resolved to an object number"
            ),
        }
    }
}

impl std::error::Error for ContentWriteError {}

/// 操作符携带的属性表若为未定型占位，返回其注册句柄；否则 `None`。
///
/// 只检查 `BDC`/`DP` 两个带属性表的操作符；供 [`ContentStream::to_bytes`]
/// （跳过该操作符）与 [`ContentStream::try_to_bytes`]（类型化报错）在写出
/// **整条操作符之前**拦截，避免留下 `/Tag ` 这类半截操作数。
pub(super) fn pending_properties_handle(op: &Op) -> Option<usize> {
    let properties = match op {
        Op::BeginMarkedContentProperties { properties, .. }
        | Op::MarkedContentPointProperties { properties, .. } => properties,
        _ => return None,
    };
    properties.as_pending_handle()
}

impl ContentStream {
    pub(super) fn serialize_one(op: &Op, out: &mut Vec<u8>) {
        let mut w = Writer(out);
        match op {
            Op::BeginText => w.emit_op(b"BT"),
            Op::EndText => w.emit_op(b"ET"),

            Op::SetFont { name, size } => {
                w.emit_byte(b'/');
                w.emit(name.as_bytes());
                w.emit_byte(b' ');
                w.emit_float(*size);
                w.emit_op_space(b"Tf");
            }
            Op::ShowText(s) => {
                w.emit_literal(s);
                w.emit_op_space(b"Tj");
            }
            Op::ShowTextHex(bytes) => {
                w.emit_byte(b'<');
                w.emit_hex(bytes);
                w.emit_byte(b'>');
                w.emit_op_space(b"Tj");
            }
            Op::ShowTextArray(items) => {
                w.emit_byte(b'[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        w.emit_byte(b' ');
                    }
                    match item {
                        TextArrayItem::String(s) => w.emit_literal(s),
                        TextArrayItem::MoveBy(n) => w.emit_float(*n),
                    }
                }
                w.emit_byte(b' ');
                w.emit_byte(b']');
                w.emit_op_space(b"TJ");
            }
            Op::ShowTextNextLine(text) => {
                w.emit_literal(text);
                w.emit_op_space(b"'");
            }
            Op::SetSpacingShowTextNextLine {
                word_spacing,
                char_spacing,
                text,
            } => {
                w.emit_float(*word_spacing);
                w.emit_byte(b' ');
                w.emit_float(*char_spacing);
                w.emit_byte(b' ');
                w.emit_literal(text);
                w.emit_op_space(b"\"");
            }
            Op::MoveNextLine { tx, ty } => {
                w.emit_float(*tx);
                w.emit_byte(b' ');
                w.emit_float(*ty);
                w.emit_op_space(b"Td");
            }
            Op::MoveNextLineSetLeading { leading } => {
                w.emit_float(*leading);
                w.emit_op_space(b"TD");
            }
            Op::NextLine => w.emit_op(b"T*"),
            Op::SetTextMatrix(m) => {
                for (i, v) in m.iter().enumerate() {
                    if i > 0 {
                        w.emit_byte(b' ');
                    }
                    w.emit_float(*v);
                }
                w.emit_op_space(b"Tm");
            }
            Op::SetCharSpacing(v) => {
                w.emit_float(*v);
                w.emit_op_space(b"Tc");
            }
            Op::SetWordSpacing(v) => {
                w.emit_float(*v);
                w.emit_op_space(b"Tw");
            }
            Op::SetTextScale(v) => {
                w.emit_float(*v);
                w.emit_op_space(b"Tz");
            }
            Op::SetTextLeading(v) => {
                w.emit_float(*v);
                w.emit_op_space(b"TL");
            }
            Op::SetTextRender(v) => {
                w.emit_int(*v as i64);
                w.emit_op_space(b"Tr");
            }
            Op::SetTextRise(v) => {
                w.emit_float(*v);
                w.emit_op_space(b"Ts");
            }

            Op::SaveState => w.emit_op(b"q"),
            Op::RestoreState => w.emit_op(b"Q"),
            Op::ConcatMatrix(m) => {
                for (i, v) in m.iter().enumerate() {
                    if i > 0 {
                        w.emit_byte(b' ');
                    }
                    w.emit_float(*v);
                }
                w.emit_op_space(b"cm");
            }
            Op::SetLineWidth(v) => {
                w.emit_float(*v);
                w.emit_op_space(b"w");
            }
            Op::SetLineCap(v) => {
                w.emit_int(v.value() as i64);
                w.emit_op_space(b"J");
            }
            Op::SetLineJoin(v) => {
                w.emit_int(v.value() as i64);
                w.emit_op_space(b"j");
            }
            Op::SetMiterLimit(v) => {
                w.emit_float(*v);
                w.emit_op_space(b"M");
            }
            Op::SetDashPattern { array, phase } => {
                w.emit_byte(b'[');
                for (i, v) in array.iter().enumerate() {
                    if i > 0 {
                        w.emit_byte(b' ');
                    }
                    w.emit_float(*v);
                }
                w.emit(b"] ");
                w.emit_float(*phase);
                w.emit_op_space(b"d");
            }
            Op::SetRenderingIntent(name) => {
                w.emit_byte(b'/');
                w.emit(name.as_bytes());
                w.emit_op_space(b"ri");
            }
            Op::SetExtGState(name) => {
                w.emit_byte(b'/');
                w.emit(name.as_bytes());
                w.emit_op_space(b"gs");
            }
            Op::SetFlatness(v) => {
                w.emit_float(*v);
                w.emit_op_space(b"i");
            }

            Op::MoveTo { x, y } => {
                w.emit_float(*x);
                w.emit_byte(b' ');
                w.emit_float(*y);
                w.emit_op_space(b"m");
            }
            Op::LineTo { x, y } => {
                w.emit_float(*x);
                w.emit_byte(b' ');
                w.emit_float(*y);
                w.emit_op_space(b"l");
            }
            Op::CurveTo {
                x1,
                y1,
                x2,
                y2,
                x3,
                y3,
            } => {
                w.emit_float(*x1);
                w.emit_byte(b' ');
                w.emit_float(*y1);
                w.emit_byte(b' ');
                w.emit_float(*x2);
                w.emit_byte(b' ');
                w.emit_float(*y2);
                w.emit_byte(b' ');
                w.emit_float(*x3);
                w.emit_byte(b' ');
                w.emit_float(*y3);
                w.emit_op_space(b"c");
            }
            Op::CurveToInitial { x2, y2, x3, y3 } => {
                w.emit_float(*x2);
                w.emit_byte(b' ');
                w.emit_float(*y2);
                w.emit_byte(b' ');
                w.emit_float(*x3);
                w.emit_byte(b' ');
                w.emit_float(*y3);
                w.emit_op_space(b"v");
            }
            Op::CurveToFinal { x1, y1, x3, y3 } => {
                w.emit_float(*x1);
                w.emit_byte(b' ');
                w.emit_float(*y1);
                w.emit_byte(b' ');
                w.emit_float(*x3);
                w.emit_byte(b' ');
                w.emit_float(*y3);
                w.emit_op_space(b"y");
            }
            Op::ClosePath => w.emit_op(b"h"),
            Op::Rectangle { x, y, w: rw, h } => {
                w.emit_float(*x);
                w.emit_byte(b' ');
                w.emit_float(*y);
                w.emit_byte(b' ');
                w.emit_float(*rw);
                w.emit_byte(b' ');
                w.emit_float(*h);
                w.emit_op_space(b"re");
            }

            Op::Stroke => w.emit_op(b"S"),
            Op::ClosePathStroke => w.emit_op(b"s"),
            Op::Fill => w.emit_op(b"f"),
            Op::FillRule => w.emit_op(b"F"),
            Op::FillEvenOdd => w.emit_op(b"f*"),
            Op::FillStroke => w.emit_op(b"B"),
            Op::FillStrokeEvenOdd => w.emit_op(b"B*"),
            Op::ClosePathFillStroke => w.emit_op(b"b"),
            Op::ClosePathFillStrokeEvenOdd => w.emit_op(b"b*"),
            Op::EndPath => w.emit_op(b"n"),

            Op::Clip => w.emit_op_space(b"W"),
            Op::ClipEvenOdd => w.emit_op_space(b"W*"),

            Op::SetStrokeGray { gray } => {
                w.emit_float(*gray);
                w.emit_op_space(b"G");
            }
            Op::SetFillGray { gray } => {
                w.emit_float(*gray);
                w.emit_op_space(b"g");
            }
            Op::SetStrokeRGB { r, g, b } => {
                w.emit_float(*r);
                w.emit_byte(b' ');
                w.emit_float(*g);
                w.emit_byte(b' ');
                w.emit_float(*b);
                w.emit_op_space(b"RG");
            }
            Op::SetFillRGB { r, g, b } => {
                w.emit_float(*r);
                w.emit_byte(b' ');
                w.emit_float(*g);
                w.emit_byte(b' ');
                w.emit_float(*b);
                w.emit_op_space(b"rg");
            }
            Op::SetStrokeCMYK { c, m, y: cy, k } => {
                w.emit_float(*c);
                w.emit_byte(b' ');
                w.emit_float(*m);
                w.emit_byte(b' ');
                w.emit_float(*cy);
                w.emit_byte(b' ');
                w.emit_float(*k);
                w.emit_op_space(b"K");
            }
            Op::SetFillCMYK { c, m, y: cy, k } => {
                w.emit_float(*c);
                w.emit_byte(b' ');
                w.emit_float(*m);
                w.emit_byte(b' ');
                w.emit_float(*cy);
                w.emit_byte(b' ');
                w.emit_float(*k);
                w.emit_op_space(b"k");
            }
            Op::SetStrokeColorSpace(name) => {
                w.emit_byte(b'/');
                w.emit(name.as_bytes());
                w.emit_op_space(b"CS");
            }
            Op::SetFillColorSpace(name) => {
                w.emit_byte(b'/');
                w.emit(name.as_bytes());
                w.emit_op_space(b"cs");
            }
            Op::SetStrokeColor(values) => {
                for (i, v) in values.iter().enumerate() {
                    if i > 0 {
                        w.emit_byte(b' ');
                    }
                    w.emit_float(*v);
                }
                w.emit_op_space(b"SC");
            }
            Op::SetFillColor(values) => {
                for (i, v) in values.iter().enumerate() {
                    if i > 0 {
                        w.emit_byte(b' ');
                    }
                    w.emit_float(*v);
                }
                w.emit_op_space(b"sc");
            }
            Op::SetStrokeColorN { values } => {
                for (i, v) in values.iter().enumerate() {
                    if i > 0 {
                        w.emit_byte(b' ');
                    }
                    w.emit_float(*v);
                }
                w.emit_op_space(b"SCN");
            }
            Op::SetFillColorN { values } => {
                for (i, v) in values.iter().enumerate() {
                    if i > 0 {
                        w.emit_byte(b' ');
                    }
                    w.emit_float(*v);
                }
                w.emit_op_space(b"scn");
            }
            Op::SetStrokeColorPattern(name) => {
                w.emit_byte(b'/');
                w.emit(name.as_bytes());
                w.emit_op_space(b"SCN");
            }
            Op::SetFillColorPattern(name) => {
                w.emit_byte(b'/');
                w.emit(name.as_bytes());
                w.emit_op_space(b"scn");
            }
            Op::SetStrokeColorPatternWithComponents { values, name } => {
                for (i, v) in values.iter().enumerate() {
                    if i > 0 {
                        w.emit_byte(b' ');
                    }
                    w.emit_float(*v);
                }
                if !values.is_empty() {
                    w.emit_byte(b' ');
                }
                w.emit_byte(b'/');
                w.emit(name.as_bytes());
                w.emit_op_space(b"SCN");
            }
            Op::SetFillColorPatternWithComponents { values, name } => {
                for (i, v) in values.iter().enumerate() {
                    if i > 0 {
                        w.emit_byte(b' ');
                    }
                    w.emit_float(*v);
                }
                if !values.is_empty() {
                    w.emit_byte(b' ');
                }
                w.emit_byte(b'/');
                w.emit(name.as_bytes());
                w.emit_op_space(b"scn");
            }

            Op::BeginMarkedContent { tag } => {
                w.emit_byte(b'/');
                w.emit(tag.as_bytes());
                w.emit_op_space(b"BMC");
            }
            Op::BeginMarkedContentProperties { tag, properties } => {
                w.emit_byte(b'/');
                w.emit(tag.as_bytes());
                w.emit_byte(b' ');
                w.emit_properties(properties);
                w.emit_op_space(b"BDC");
            }
            Op::MarkedContentPoint { tag } => {
                w.emit_byte(b'/');
                w.emit(tag.as_bytes());
                w.emit_op_space(b"MP");
            }
            Op::MarkedContentPointProperties { tag, properties } => {
                w.emit_byte(b'/');
                w.emit(tag.as_bytes());
                w.emit_byte(b' ');
                w.emit_properties(properties);
                w.emit_op_space(b"DP");
            }
            Op::EndMarkedContent => w.emit_op(b"EMC"),

            Op::DrawXObject(name) => {
                w.emit_byte(b'/');
                w.emit(name.as_bytes());
                w.emit_op_space(b"Do");
            }
            Op::PaintShading(name) => {
                w.emit_byte(b'/');
                w.emit(name.as_bytes());
                w.emit_op_space(b"sh");
            }
        }
    }
}

// ── 内部辅助：Writer + 字面量转义 ─────────────────────

struct Writer<'a>(&'a mut Vec<u8>);

impl<'a> Writer<'a> {
    fn emit(&mut self, data: &[u8]) {
        self.0.extend_from_slice(data);
    }
    fn emit_byte(&mut self, b: u8) {
        self.0.push(b);
    }
    fn emit_int(&mut self, v: i64) {
        self.0.extend_from_slice(v.to_string().as_bytes());
    }
    fn emit_float(&mut self, v: f64) {
        self.0.extend_from_slice(format_float(v).as_bytes());
    }
    fn emit_hex(&mut self, bytes: &[u8]) {
        const HEX: &[u8; 16] = b"0123456789ABCDEF";
        for &byte in bytes {
            self.0.push(HEX[usize::from(byte >> 4)]);
            self.0.push(HEX[usize::from(byte & 0x0F)]);
        }
    }
    fn emit_op(&mut self, kw: &[u8]) {
        self.0.extend_from_slice(kw);
        self.0.push(b'\n');
    }
    /// 带前置空格的操作符输出（有参数操作符：参数与关键字之间需要空格）。
    fn emit_op_space(&mut self, kw: &[u8]) {
        self.0.push(b' ');
        self.0.extend_from_slice(kw);
        self.0.push(b'\n');
    }
    /// 输出标记内容属性表：资源名 `/name`、内联字典 `<< ... >>`，或间接引用
    /// 操作数 `obj gen R`（后一形态非 ISO 32000-1 授权，是互操作容错/本库自有
    /// 写法，见 [`MarkedContentProperties`] 文档；只写操作数，不登记被引用对象）。
    fn emit_properties(&mut self, properties: &MarkedContentProperties) {
        // 未定型占位（句柄尚未被装配层替换）兜底：调用方 `to_bytes` /
        // `try_to_bytes` 已在写出整条操作符前拦截，这里宁可不写，也绝不输出
        // `<句柄> -1 R` 这种看起来像引用的假操作数。
        if properties.is_pending_handle() {
            return;
        }
        match properties {
            MarkedContentProperties::Name(name) => {
                self.0.push(b'/');
                self.0.extend_from_slice(name.as_bytes());
            }
            MarkedContentProperties::Dict(dict) => {
                let mut out = String::new();
                crate::core::writer::write_value(&Value::Dict(dict.clone()), &mut out);
                self.0.extend_from_slice(out.as_bytes());
            }
            MarkedContentProperties::Reference { object, generation } => {
                self.emit_int(*object);
                self.emit_byte(b' ');
                self.emit_int(*generation);
                self.emit_byte(b' ');
                self.emit(b"R");
            }
        }
    }
    /// 输出转义后的文本字面量 `(s)`，内部处理 `( ) \` 等转义。
    fn emit_literal(&mut self, s: &str) {
        self.0.push(b'(');
        for c in s.chars() {
            match c {
                '(' | ')' | '\\' => {
                    self.0.push(b'\\');
                    let mut buf = [0u8; 4];
                    let encoded = c.encode_utf8(&mut buf);
                    self.0.extend_from_slice(encoded.as_bytes());
                }
                '\n' => self.0.extend_from_slice(b"\\n"),
                '\r' => self.0.extend_from_slice(b"\\r"),
                '\t' => self.0.extend_from_slice(b"\\t"),
                _ => {
                    let mut buf = [0u8; 4];
                    let encoded = c.encode_utf8(&mut buf);
                    self.0.extend_from_slice(encoded.as_bytes());
                }
            }
        }
        self.0.push(b')');
    }
}
