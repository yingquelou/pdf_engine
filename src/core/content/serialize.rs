//! 操作符到内容流字节的序列化。

use crate::core::writer::format_float;

use super::stream::ContentStream;
use super::types::{Op, TextArrayItem};

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
            Op::CurveTo { x1, y1, x2, y2, x3, y3 } => {
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

            Op::DrawXObject(name) => {
                w.emit_byte(b'/');
                w.emit(name.as_bytes());
                w.emit_op_space(b"Do");
            }

            Op::SetFillAlpha(v) => {
                w.emit_float(*v);
                w.emit_op_space(b"ca");
            }
            Op::SetStrokeAlpha(v) => {
                w.emit_float(*v);
                w.emit_op_space(b"CA");
            }
            Op::SetBlendMode(mode) => {
                w.emit_byte(b'/');
                w.emit(mode.as_bytes());
                w.emit_op_space(b"BM");
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
