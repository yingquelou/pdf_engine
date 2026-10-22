//! PDF 值、对象与流的文本序列化。

use crate::pdf::types::{Array, Dict, Document, Object, StreamData, Value};
use std::fmt::Write as _;

use super::WriteError;

/// 序列化工作项：显式栈的元素。
///
/// 用 `Vec<Emit>` 逆序压入待输出项，把嵌套 Dict/Array 的深度优先遍历
/// 从递归改为迭代，避免深嵌套值导致栈溢出。
enum Emit<'a> {
    /// 待序列化的值（标量直接写出；容器展开为子项重新入栈）。
    Value(&'a Value),
    /// 流对象的字典部分（`<< ... >>\nstream\n`），第二项为流字节长度。
    StreamDict(&'a Dict, usize),
    /// 已转义的 Name（补 `/` 并做 `#xx`）。
    Name(&'a str),
    /// 固定文本。
    Str(&'static str),
    /// 运行时拼接的文本（如 `/Length N`）。
    Owned(String),
}

/// 把单个 [`Value`] 序列化为字符串形式。
///
/// 返回 `String` 是因为 PDF 对象的文本表示总是合法 UTF-8；
/// 流的二进制内容由上层 `write_document` 单独处理。
pub fn write_value(value: &Value, out: &mut String) {
    emit_all(vec![Emit::Value(value)], out);
}

/// 驱动显式栈，直到所有工作项输出完毕。
fn emit_all(mut stack: Vec<Emit<'_>>, out: &mut String) {
    while let Some(item) = stack.pop() {
        match item {
            Emit::Str(s) => out.push_str(s),
            Emit::Owned(s) => out.push_str(&s),
            Emit::Name(n) => write_name(out, n),
            Emit::StreamDict(dict, bytes_len) => {
                // 输出：`<<` + 各条目 + （缺省时补 `/Length`）+ ` >>\nstream\n`
                stack.push(Emit::Str(" >>\nstream\n"));
                if !dict.0.contains_key("Length") {
                    stack.push(Emit::Owned(format!(" /Length {bytes_len}")));
                }
                for (k, v) in dict.0.iter().rev() {
                    stack.push(Emit::Value(v));
                    stack.push(Emit::Str(" "));
                    stack.push(Emit::Name(k));
                    stack.push(Emit::Str(" "));
                }
                stack.push(Emit::Str("<<"));
            }
            Emit::Value(value) => match value {
                Value::Integer(n) => {
                    let _ = write!(out, "{n}");
                }
                Value::Float(f) => {
                    // 规范要求至少一位小数；这里用通用格式
                    let s = format_float(*f);
                    let _ = write!(out, "{s}");
                }
                Value::Bool(b) => {
                    out.push_str(if *b { "true" } else { "false" });
                }
                Value::Null => out.push_str("null"),
                Value::Name(s) => write_name(out, s),
                Value::String(s) => {
                    out.push('(');
                    for c in s.chars() {
                        match c {
                            '(' | ')' | '\\' => {
                                out.push('\\');
                                out.push(c);
                            }
                            '\n' => out.push_str("\\n"),
                            '\r' => out.push_str("\\r"),
                            '\t' => out.push_str("\\t"),
                            _ => out.push(c),
                        }
                    }
                    out.push(')');
                }
                Value::HexString(s) => {
                    out.push('<');
                    out.push_str(s);
                    out.push('>');
                }
                Value::Ascii85String(s) => {
                    out.push_str("<~");
                    out.push_str(s);
                    out.push_str("~>");
                }
                Value::Dict(d) => {
                    // 逆序压入：`<<` + 各 ` key value` + ` >>`
                    stack.push(Emit::Str(" >>"));
                    for (k, v) in d.0.iter().rev() {
                        stack.push(Emit::Value(v));
                        stack.push(Emit::Str(" "));
                        stack.push(Emit::Name(k));
                        stack.push(Emit::Str(" "));
                    }
                    stack.push(Emit::Str("<<"));
                }
                Value::Array(a) => {
                    // 逆序压入：`[` + 空格分隔的各值 + `]`
                    stack.push(Emit::Str("]"));
                    for (i, v) in a.0.iter().enumerate().rev() {
                        stack.push(Emit::Value(v));
                        if i > 0 {
                            stack.push(Emit::Str(" "));
                        }
                    }
                    stack.push(Emit::Str("["));
                }
                Value::Ref(n, g) => {
                    let _ = write!(out, "{n} {g} R");
                }
                Value::Stream(dict, data) => {
                    // write_value 无 Document 上下文，无法按需从文件读取流字节
                    // 长度：Owned 用实际长度，Ref 尝试从字典 /Length 取，否则 0。
                    let bytes_len = match data {
                        crate::pdf::types::StreamData::Owned(b) => b.len(),
                        crate::pdf::types::StreamData::Ref { .. }
                        | crate::pdf::types::StreamData::EncryptedRef { .. } => dict
                            .get("Length")
                            .and_then(|v| v.as_integer())
                            .map(|n| n as usize)
                            .unwrap_or(0),
                    };
                    // 流字典后接 `endstream`（二进制内容由 write_object 拼接）
                    stack.push(Emit::Str("endstream"));
                    stack.push(Emit::StreamDict(dict, bytes_len));
                }
            },
        }
    }
}

/// 序列化流对象的字典部分（`<< ... >>\nstream\n`），含 /Length 补全。
/// 流的二进制内容与 endstream/endobj 由调用方拼接。
fn write_stream_dict(dict: &Dict, bytes_len: usize, out: &mut String) {
    emit_all(vec![Emit::StreamDict(dict, bytes_len)], out);
}

/// 把一个 PDF Name（已去掉前导 `/`）写入 `out`，补回前导 `/` 并做
/// `#xx` 转义（读端 `Lexer::read_name` 的解码的逆运算）。
///
/// 需要转义的字节：空白与控制字符（`< 0x21`）、不可打印字节
/// （`> 0x7E`）、分隔符 `( ) < > [ ] { } / %`，以及转义引导符 `#`；
/// 其余可见 ASCII 原样输出。**所有** Name 输出点（字典键、Name 值）
/// 都经此函数，避免重复。
///
/// **取舍**：内部 `Name` 为 Rust `String`，而读端把 `#xx` 解码出的字节
/// 按 Latin-1 映射为 `char`。故 `char as u32 <= 0xFF` 的字符（含 Latin-1
/// 范围的非 ASCII，如 `é`）按单字节规则转义后**可精确回读**；对 `> 0xFF`
/// 的字符则按其 UTF-8 字节逐个 `#xx` 输出（保持为合法 PDF 名称），读端会
/// 把它们当作多个 Latin-1 字符，**无法完美回读**——这是字节导向的 PDF
/// Name 与 Rust `String` 之间的固有边界。
fn write_name(out: &mut String, name: &str) {
    out.push('/');
    for c in name.chars() {
        let cp = c as u32;
        if cp <= 0xFF {
            // 单字节（含 Latin-1 非 ASCII）：按字节规则转义。
            if needs_name_escape(cp as u8) {
                write_escaped_name_byte(out, cp as u8);
            } else {
                out.push(cp as u8 as char);
            }
        } else {
            // > 0xFF：按 UTF-8 字节逐个转义，保证输出仍是合法 Name。
            let mut buf = [0u8; 4];
            for &b in c.encode_utf8(&mut buf).as_bytes() {
                write_escaped_name_byte(out, b);
            }
        }
    }
}

/// 以 `#xx` 形式输出一个字节。
fn write_escaped_name_byte(out: &mut String, b: u8) {
    let _ = write!(out, "#{b:02X}");
}

/// 字节是否需在 Name 中转义：空白/控制（`< 0x21`）、不可打印（`> 0x7E`）、
/// 分隔符（`( ) < > [ ] { } / %`）与转义引导符 `#`。
fn needs_name_escape(b: u8) -> bool {
    b < 0x21
        || b > 0x7E
        || matches!(
            b,
            b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%' | b'#'
        )
}

/// 把浮点数格式化为 PDF 友好的字符串：保证至少一位小数，
/// 去掉尾随零。
pub(crate) fn format_float(f: f64) -> String {
    if f.is_nan() {
        return "0.0".to_string();
    }
    if f.is_infinite() {
        return if f > 0.0 {
            "1e999".to_string()
        } else {
            "-1e999".to_string()
        };
    }
    let s = format!("{f}");
    if !s.contains('.') && !s.contains('e') && !s.contains('E') {
        format!("{s}.0")
    } else {
        s
    }
}

/// 准备写出的流字典：若流字节已被读端解压（`StreamData::Owned`），则字典中
/// **由读端解压过的**过滤器（`FlateDecode` / `LZWDecode` / `CCITTFaxDecode` /
/// `ASCIIHexDecode` / `ASCII85Decode` / `RunLengthDecode` / `DCTDecode`，
/// 含其缩写）应被移除——字节已是明文，再声明过滤器会让读者二次解压。
/// **这是「宽进」（读端解压）与「严出」（按目标版本重新表达）之间的必要
/// 协调**。
///
/// - 写端构造的 Owned 流（本就无 Filter）：原样返回；
/// - 读端解压过的 Owned 流（dict 仍声明这些过滤器）：移除它们与
///   `/DecodeParms`，并把 `/Length` 更新为解压后长度；
/// - 读端**不解压**的过滤器（`BZip2Decode` / `JPXDecode` /
///   `JBIG2Decode` 等）：原样保留，否则会把压缩态字节当
///   明文写出；
/// - `StreamData::Ref`（未解压的原始字节）：字典保持不变。
fn prepare_stream_dict_for_write(dict: &Dict, owned_bytes: &[u8]) -> Dict {
    let Some(filter_val) = dict.get("Filter").or_else(|| dict.get("F")) else {
        // 无 Filter 声明 → 直接更新 /Length 就完事
        let mut d = dict.clone();
        d.0.insert("Length".into(), Value::Integer(owned_bytes.len() as i64));
        return d;
    };

    // 「读端已解压」的过滤器判定与 `filters.rs::canonical_filter` 保持单一
    // 事实源：只有读端确实解压过的过滤器，其 `/Filter` 才可以从字典里移除
    // （字节在内存中已是明文）。**BZip2 / JPX / JBIG2 等读端并不解压**，
    // 必须原样保留——否则会把仍处于压缩态的字节当作明文写出。
    let is_reader_decoded = |name: &str| {
        matches!(
            crate::pdf::filters::canonical_filter(name),
            "FlateDecode"
                | "LZWDecode"
                | "CCITTFaxDecode"
                | "DCTDecode"
                | "ASCIIHexDecode"
                | "ASCII85Decode"
                | "RunLengthDecode"
        )
    };

    // 收集需要保留的过滤器（读端不解压的过滤器原样保留）
    let mut remaining: Vec<Value> = Vec::new();
    match filter_val {
        Value::Name(n) => {
            if !is_reader_decoded(n) {
                remaining.push(Value::Name(n.clone()));
            }
        }
        Value::Array(a) => {
            for item in a.iter() {
                if let Value::Name(n) = item {
                    if !is_reader_decoded(n) {
                        remaining.push(Value::Name(n.clone()));
                    }
                } else {
                    // 非 Name 项保留（罕见但安全）
                    remaining.push(item.clone());
                }
            }
        }
        _ => {
            // 类型异常，保留原样
        }
    }

    let mut d = dict.clone();
    if remaining.is_empty() {
        // 全部是读端已解压的过滤器 → 移除 /Filter（含 /F 别名）与 /DecodeParms
        d.0.remove("Filter");
        d.0.remove("F");
        d.0.remove("DecodeParms");
        d.0.remove("DP");
    } else {
        // 仍有保留的过滤器 → 单个写成 Name，多个写成 Array；同时清掉 /F
        // 别名，避免 `/Filter` 与 `/F` 同时存在造成歧义。
        let kept = if remaining.len() == 1 {
            remaining.pop().unwrap()
        } else {
            Value::Array(Array(remaining))
        };
        d.0.remove("F");
        d.0.insert("Filter".into(), kept);
        if matches!(filter_val, Value::Array(_)) {
            // 数组情形下 /DecodeParms 与过滤器按下标配对，移除部分项后配对
            // 关系已错位，故一并移除；单个 Name 情形下 parms 仍属于该过滤器，
            // 予以保留。
            d.0.remove("DecodeParms");
            d.0.remove("DP");
        }
    }
    // 无论如何，/Length 必须对应 Owned 字节长度
    d.0.insert("Length".into(), Value::Integer(owned_bytes.len() as i64));
    d
}

/// 序列化单个间接对象到字节缓冲，返回该对象的字节。
///
/// 流字节从 `doc.stream_bytes` 取（读端按需从文件读，写端用 Owned）。
pub fn write_object(obj: &Object, doc: &Document) -> Result<Vec<u8>, WriteError> {
    if obj.num <= 0 {
        return Err(WriteError::InvalidObject(format!(
            "object number must be positive, got {}",
            obj.num
        )));
    }
    let mut header = String::new();
    let _ = writeln!(&mut header, "{} {} obj", obj.num, obj.generation);
    let mut out = Vec::with_capacity(header.len() + 16);
    out.extend_from_slice(header.as_bytes());

    match &obj.value {
        Value::Stream(dict, data) => {
            let bytes = doc.stream_bytes(&obj.value).ok_or_else(|| {
                WriteError::InvalidObject(format!(
                    "stream bytes unavailable for object {}",
                    obj.num
                ))
            })?;

            // 流字典准备：
            // - Owned 字节 + 原 dict 有 /Filter → 移除压缩过滤器 + 更新 Length（宽进严出协调）
            // - Ref 字节 + 原 dict → 原样（写端构造的 Ref 流不存在"解压"问题）
            let prepared_dict = match data {
                StreamData::Owned(_) => prepare_stream_dict_for_write(dict, &bytes),
                StreamData::Ref { .. } | StreamData::EncryptedRef { .. } => {
                    let mut d = dict.clone();
                    d.0.insert("Length".into(), Value::Integer(bytes.len() as i64));
                    d
                }
            };

            let mut s = String::new();
            write_stream_dict(&prepared_dict, bytes.len(), &mut s);
            out.extend_from_slice(s.as_bytes());
            out.extend_from_slice(bytes.as_slice());
            out.extend_from_slice(b"\nendstream\nendobj\n");
        }
        other => {
            let mut s = String::new();
            write_value(other, &mut s);
            s.push('\n');
            out.extend_from_slice(s.as_bytes());
            out.extend_from_slice(b"endobj\n");
        }
    }
    Ok(out)
}
