use super::*;

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
