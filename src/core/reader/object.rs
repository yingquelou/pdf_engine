//! PDF 对象、间接对象、trailer 与 xref 语法解析。

use crate::core::types::{
    Array, Dict, Object, StartXref, SubXref, Trailer, Value, Xref, XrefEntry, XrefKind,
};

use super::lexer::{Lexer, Token};
use super::{Result, err};

// ── 自由解析函数（接收 `&mut Lexer<'_>`） ──────────────────────────

/// 解析任意 PDF 对象（不含间接对象头）。
///
/// 间接引用识别规则：`int int R`。这里采用「先读两个数字，
/// 看下一 token 是否 `R`」的试探。
///
/// **迭代实现**：数组/字典容器用显式栈 [`Frame`] 记录，深度优先
/// 展开而非递归调用，避免深嵌套输入导致栈溢出。
pub(crate) fn parse_object(lex: &mut Lexer<'_>) -> Result<Value> {
    let mut stack: Vec<Frame> = Vec::new();
    loop {
        // 字典帧尚未读到键时：先读键（Name）或结束符 `>>`。
        if matches!(stack.last(), Some(Frame::Dict(_, None))) {
            lex.skip_ws();
            // 提前判断 `>>` 以避免把 `>>` 误解析为 Name
            if lex.peek() == Some(b'>') && lex.peek_at(1) == Some(b'>') {
                lex.pos += 2;
                if let Some(v) = pop_frame_value(&mut stack, lex.pos())? {
                    return Ok(v);
                }
                continue;
            }
            match lex.next_token()? {
                Token::Name(s) => {
                    if let Some(Frame::Dict(_, key)) = stack.last_mut() {
                        *key = Some(s);
                    }
                }
                Token::DictClose => {
                    if let Some(v) = pop_frame_value(&mut stack, lex.pos())? {
                        return Ok(v);
                    }
                    continue;
                }
                other => {
                    return Err(err(
                        lex.pos(),
                        format!("dict key must be Name, got {:?}", other),
                    ));
                }
            }
        } else if matches!(stack.last(), Some(Frame::Array(_))) {
            lex.skip_ws();
            if lex.peek() == Some(b']') {
                lex.pos += 1;
                if let Some(v) = pop_frame_value(&mut stack, lex.pos())? {
                    return Ok(v);
                }
                continue;
            }
        }

        let tok = lex.next_token()?;
        let value = match tok {
            Token::Number(s) => {
                // 浮点数不可能是间接引用的一部分，直接返回。
                if s.contains('.') {
                    let v = s
                        .parse::<f64>()
                        .map_err(|e| err(lex.pos(), format!("bad float {s}: {e}")))?;
                    Value::Float(v)
                } else {
                    let n = s
                        .parse::<i64>()
                        .map_err(|e| err(lex.pos(), format!("bad integer {s}: {e}")))?;

                    // 尝试 `n g R`：保存位置，失败则回退。
                    let saved = lex.pos;
                    let mut resolved = None;
                    match try_peek_is_number(lex) {
                        Ok(Token::Number(s2)) if s2.parse::<i64>().is_ok() => {
                            let g: i64 = s2.parse().unwrap();
                            match lex.next_token() {
                                Ok(Token::Keyword(k)) if k == "R" => {
                                    resolved = Some(Value::Ref(n, g));
                                }
                                _ => {
                                    // 不是 R，回退，把 n 当作整数。
                                    lex.pos = saved;
                                }
                            }
                        }
                        _ => {
                            // 第二个 token 不是「可作 generation 的整数」，
                            // 词法器已消费了它，需要回退到 saved。
                            lex.pos = saved;
                        }
                    }
                    resolved.unwrap_or(Value::Integer(n))
                }
            }
            Token::Name(s) => Value::Name(s),
            Token::Literal(s) => Value::String(s),
            Token::Hex(s) => Value::HexString(s),
            // 非 ISO 32000-1 的 `<~ ~>` ASCII85 字符串扩展形态：payload
            // 逐字保留（与写端包裹方式对称），不在此处做 base-85 解码。
            Token::Ascii85(s) => Value::Ascii85String(s),
            Token::Keyword(k) => match k.as_str() {
                "true" => Value::Bool(true),
                "false" => Value::Bool(false),
                "null" => Value::Null,
                _ => return Err(err(lex.pos(), format!("unexpected keyword '{k}'"))),
            },
            Token::DictOpen => {
                push_container(&stack, lex.pos())?;
                stack.push(Frame::Dict(Dict::new(), None));
                continue;
            }
            Token::ArrayOpen => {
                push_container(&stack, lex.pos())?;
                stack.push(Frame::Array(Array::new()));
                continue;
            }
            other => return Err(err(lex.pos(), format!("unexpected token {:?}", other))),
        };

        if let Some(v) = attach(&mut stack, value, lex.pos())? {
            return Ok(v);
        }
    }
}

/// 数组/字典嵌套深度上限：显式栈本身不占调用栈，但深嵌套值在
/// 释放（`Drop`）时仍会递归，故保留该上限作为资源与安全护栏。
/// 正常 PDF 远达不到该深度。
const MAX_NESTING_DEPTH: usize = 64;

/// 显式解析栈的容器帧。
enum Frame {
    /// 正在收集元素的数组。
    Array(Array),
    /// 正在收集键值的字典；`Option<String>` 为待配对的键。
    Dict(Dict, Option<String>),
}

/// 打开新容器前的深度检查。
fn push_container(_stack: &[Frame], pos: u64) -> Result<()> {
    if _stack.len() >= MAX_NESTING_DEPTH {
        return Err(err(
            pos,
            format!("nesting too deep (limit {MAX_NESTING_DEPTH})"),
        ));
    }
    Ok(())
}

/// 把已完成的子值挂到当前容器；若无外层容器则返回该值为最终结果。
fn attach(stack: &mut Vec<Frame>, value: Value, pos: u64) -> Result<Option<Value>> {
    match stack.last_mut() {
        None => Ok(Some(value)),
        Some(Frame::Array(arr)) => {
            arr.0.push(value);
            Ok(None)
        }
        Some(Frame::Dict(dict, key)) => match key.take() {
            Some(k) => {
                dict.0.insert(k, value);
                Ok(None)
            }
            None => Err(err(pos, "dict value without preceding key")),
        },
    }
}

/// 弹出当前最内层容器帧并挂到其父容器（或作为最终结果返回）。
fn pop_frame_value(stack: &mut Vec<Frame>, pos: u64) -> Result<Option<Value>> {
    let frame = stack.pop().expect("container frame must exist");
    let value = match frame {
        Frame::Array(a) => Value::Array(a),
        Frame::Dict(d, _) => Value::Dict(d),
    };
    attach(stack, value, pos)
}

/// 试图预读一个数字 token；若非数字则回退到调用前位置。
fn try_peek_is_number(lex: &mut Lexer<'_>) -> Result<Token> {
    let saved = lex.pos;
    lex.skip_ws();
    let is_num =
        matches!(lex.peek(), Some(b) if b.is_ascii_digit() || b == b'.' || b == b'+' || b == b'-');
    if !is_num {
        lex.pos = saved;
        return Err(err(lex.pos(), "not a number"));
    }
    match lex.next_token() {
        Ok(t @ Token::Number(_)) => Ok(t),
        Ok(other) => {
            lex.pos = saved;
            Err(err(lex.pos(), format!("expected number, got {:?}", other)))
        }
        Err(e) => {
            lex.pos = saved;
            Err(e)
        }
    }
}

/// 解析 `num gen obj <value> endobj`。
///
/// 流处理：当 `<value>` 是 dict 且后跟 `stream` 关键字时，消费 `stream`
/// 与一个 EOL，调用 [`Lexer::compute_stream_range`] 得到流字节在文件中的
/// **绝对偏移 + 长度**，构造 `Value::Stream(d, StreamData::Ref{offset,len})`。
/// 流字节不读入内存。
pub(crate) fn parse_indirect_object(lex: &mut Lexer<'_>) -> Result<Object> {
    parse_indirect_object_with_length_resolver(lex, |_, _| Ok(None))
}

pub(crate) fn parse_indirect_object_with_length_resolver(
    lex: &mut Lexer<'_>,
    mut resolve_length: impl FnMut(i64, i64) -> Result<Option<u64>>,
) -> Result<Object> {
    let num = parse_int_token(lex, "object number")?;
    let generation = parse_int_token(lex, "generation")?;
    let kw = lex.next_token()?;
    if !matches!(kw, Token::Keyword(ref k) if k == "obj") {
        return Err(err(lex.pos(), format!("expected 'obj', got {:?}", kw)));
    }
    let mut value = parse_object(lex)?;

    // 处理 stream
    if let Value::Dict(d) = &value
        && lex.peek_is_stream_keyword()
    {
        // 消费 "stream"
        let _ = lex.next_token()?;
        // 规范要求 stream 后跟 CRLF 或 LF；我们容错到任意一个换行。
        if lex.peek() == Some(b'\r') {
            lex.pos += 1;
        }
        if lex.peek() == Some(b'\n') {
            lex.pos += 1;
        }
        let declared_length = match d.get("Length") {
            Some(Value::Integer(length)) => Some(
                u64::try_from(*length)
                    .map_err(|_| err(lex.pos(), "invalid negative stream /Length"))?,
            ),
            Some(Value::Ref(length_num, length_generation)) => {
                resolve_length(*length_num, *length_generation)?
            }
            Some(_) => return Err(err(lex.pos(), "invalid stream /Length")),
            None => None,
        };
        let (offset, len) = lex.compute_stream_range(declared_length)?;
        // 消费 endstream
        lex.skip_ws();
        let end = lex.next_token()?;
        if !matches!(end, Token::Keyword(ref k) if k == "endstream") {
            return Err(err(
                lex.pos(),
                format!("expected 'endstream', got {:?}", end),
            ));
        }
        value = Value::Stream(
            d.clone(),
            crate::core::types::StreamData::Ref { offset, len },
        );
    }

    // endobj（部分文件省略，故宽容）
    let saved = lex.pos;
    lex.skip_ws();
    match lex.next_token() {
        Ok(Token::Keyword(k)) if k == "endobj" => {}
        _ => lex.pos = saved, // 回退
    }

    Ok(Object::new(num, generation, value))
}

pub(crate) fn parse_int_token(lex: &mut Lexer<'_>, what: &str) -> Result<i64> {
    let t = lex.next_token()?;
    match t {
        Token::Number(s) => s
            .parse::<i64>()
            .map_err(|e| err(lex.pos(), format!("bad {what} '{s}': {e}"))),
        other => Err(err(lex.pos(), format!("expected {what}, got {:?}", other))),
    }
}

/// 解析 `trailer << ... >>`。
pub(crate) fn parse_trailer(lex: &mut Lexer<'_>) -> Result<Trailer> {
    let kw = lex.next_token()?;
    if !matches!(kw, Token::Keyword(ref k) if k == "trailer") {
        return Err(err(lex.pos(), format!("expected 'trailer', got {:?}", kw)));
    }
    let v = parse_object(lex)?;
    match v {
        Value::Dict(d) => Ok(Trailer { dict: d }),
        other => Err(err(
            lex.pos(),
            format!("trailer must be a dict, got {}", other.type_name()),
        )),
    }
}

/// 解析经典交叉引用表。
/// 语法：
/// ```text
/// xref
/// <start> <count>
/// <offset10> <gen5> <n|f>
/// ...
/// ```
pub(crate) fn parse_xref(lex: &mut Lexer<'_>) -> Result<Xref> {
    let kw = lex.next_token()?;
    if !matches!(kw, Token::Keyword(ref k) if k == "xref") {
        return Err(err(lex.pos(), format!("expected 'xref', got {:?}", kw)));
    }
    let mut subs = Vec::new();
    loop {
        lex.skip_ws();
        // trailer 关键字意味着 xref 段结束
        if lex.starts_with(b"trailer") {
            break;
        }
        // EOF 兜底
        if lex.peek().is_none() {
            break;
        }
        let start = parse_int_token(lex, "xref sub start")?;
        let count = parse_int_token(lex, "xref sub count")?;
        let mut entries = Vec::with_capacity(count.max(0) as usize);
        for _ in 0..count {
            let off = parse_int_token(lex, "xref offset")?;
            let generation = parse_int_token(lex, "xref generation")?;
            let kind = lex.next_token()?;
            let in_use = match kind {
                Token::Keyword(k) if k == "n" => true,
                Token::Keyword(k) if k == "f" => false,
                other => {
                    return Err(err(
                        lex.pos(),
                        format!("expected 'n' or 'f', got {:?}", other),
                    ));
                }
            };
            entries.push(XrefEntry {
                offset: off,
                generation,
                kind: if in_use {
                    XrefKind::Normal
                } else {
                    XrefKind::Free
                },
            });
        }
        subs.push(SubXref::new(start, entries));
    }
    Ok(Xref(subs))
}

/// 解析 `startxref <offset> %%EOF` 中的数字部分。
pub(crate) fn parse_startxref(lex: &mut Lexer<'_>) -> Result<StartXref> {
    let kw = lex.next_token()?;
    if !matches!(kw, Token::Keyword(ref k) if k == "startxref") {
        return Err(err(
            lex.pos(),
            format!("expected 'startxref', got {:?}", kw),
        ));
    }
    let n = parse_int_token(lex, "startxref value")?;
    Ok(StartXref(n))
}

/// 从字节缓冲读取定宽字段（大端），返回 u64。
pub(crate) fn read_field(bytes: &[u8], pos: usize, width: usize) -> u64 {
    let mut v: u64 = 0;
    for i in 0..width {
        if let Some(&b) = bytes.get(pos + i) {
            v = (v << 8) | b as u64;
        }
    }
    v
}
