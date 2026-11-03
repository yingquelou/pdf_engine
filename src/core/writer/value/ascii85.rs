//! ASCII85 串（`<~ ... ~>`）body 的写出前门禁（ISO 32000-1 口径）。
//!
//! 本模块只做「能不能原样写出」的判定，产出 crate 既有的类型化写错误
//! [`WriteError::InvalidObject`](crate::core::writer::WriteError::InvalidObject)；
//! 序列化本身在父模块 `value`。
//!
//! 规范依据（本地副本 `docs/PDF32000_2008.pdf`，行号为
//! `pdftotext -raw docs/PDF32000_2008.pdf -` 的输出行；多列表格必须用
//! `-raw`，`-layout` 会错位）：
//!
//! - **§7.3.5 不是字符串**：raw L911 起是 `7.3.5 Name Objects`。字符串在
//!   **§7.3.4 String Objects**（raw L815-822）里只有两种形态——`( )`
//!   字面串与 `< >` 十六进制串。`<~ ~>` **不在 ISO 32000-1 语法内**，
//!   它是生产者把 ASCII85 过滤器数据直接当字符串字面量写的扩展形态，
//!   读端 `reader/lexer.rs::read_ascii85` 按「非 ISO 扩展」逐字接收。
//! - **字符口径唯一来源是 §7.4.3 ASCII85Decode Filter**（raw L1311-1313，
//!   PDF 页 33）：「The ASCII base-85 encoding shall use the ASCII
//!   characters ! through u ((21h) - (75h)) and the character z (7Ah),
//!   with the 2-character sequence ~> (7Eh)(3Eh) as its EOD marker.
//!   The ASCII85Decode filter shall ignore all white-space characters
//!   (see 7.2, "Lexical Conventions"). Any other characters, and any
//!   character sequences that represent impossible combinations in the
//!   ASCII base-85 encoding shall cause an error.」
//! - **白空格集合**：§7.2.2 Table 1（raw L738-746，PDF 页 13）=
//!   NUL(00h) HT(09h) LF(0Ah) FF(0Ch) CR(0Dh) SP(20h)，与本库读端
//!   `Lexer::is_whitespace`、过滤器 `filters/ascii.rs` 同口径。
//! - **`z` 短写**：§7.4.3 raw L1320-1322 只在「五字节全 0」时以 `z`(7Ah)
//!   代替五个 `!`；raw L1330-1332 列出「correctly encoded byte sequence」
//!   永不出现的情形，其一是「A z character occurs in the middle of a
//!   group」。本库 `filters/ascii.rs` 的解码同样以
//!   `'z' inside a partial group` 报错，故写端在同一口径上拒绝。
//!
//! **要存任意字节请改用 `Value::String` / `Value::HexString`**（§7.3.4.2
//! 的字面串转义、§7.3.4.3 的十六进制串都能承载任意字节序列），二进制负载
//! 走流对象或嵌入文件路径（12.7.4 嵌入文件流 / 7.4 过滤器）。本车道**不**
//! 提供 base-85 编码器：`Ascii85String` 存的是调用方给的编码文本，写端
//! 只判定「原样夹进 `<~ ~>` 之后还能不能被自己的读端取回同一个值」，
//! 判定不过就类型化拒绝，绝不转义或剥离——那会改变调用方要求的语义字节。

use crate::core::types::{Dict, Document, Value};
use crate::core::writer::WriteError;

/// §7.2.2 Table 1（raw L738-746）的白空格集合；不计入 base-85 组长度。
fn is_ascii85_whitespace(b: u8) -> bool {
    matches!(b, 0x00 | b'\t' | b'\n' | 0x0c | b'\r' | b' ')
}

/// 判定 `Value::Ascii85String` 的 body（`<~` 与 `~>` 之间的原文）能否
/// 原样写出；返回 `Some((字节偏移, 违规字节, 理由))` 表示必须拒绝。
///
/// 单趟扫描、零分配（违规消息由调用方 `format!`）：
/// 1. `~>`：ASCII85 的 EOD 标记。body 里出现它时，`<~ body ~>` 会被本库
///    读端在**第一个** `~>` 处收尾，产出比写入值更短的串——静默损坏。
/// 2. 单独的 `~`：不在 `!`–`u`/`z` 内（7Eh），且本库 `ASCII85Decode`
///    把它当 EOD 提前终止；写出的串虽能字节对称地读回，解码出的字节却
///    被截短，故一并拒绝。
/// 3. `z` 只允许落在组边界（此前已消费的 body 数据字符数是 5 的倍数）。
/// 4. 其余字节必须落在 `!`(21h)–`u`(75h)；Table 1 的白空格任意位置合法。
///
/// 不校验组取值（> 2^32-1）、末组长度等 base-85「impossible
/// combinations」：那是 `filters::decode_stream` 解码期的职责，字面量层
/// 的往返安全只依赖定界符与字符集。
pub(crate) fn ascii85_body_rejection(body: &str) -> Option<(usize, u8, &'static str)> {
    let bytes = body.as_bytes();
    let mut group_len = 0usize;
    for (offset, &b) in bytes.iter().enumerate() {
        if is_ascii85_whitespace(b) {
            continue;
        }
        match b {
            b'~' => {
                let reason = if bytes.get(offset + 1) == Some(&b'>') {
                    "contains the ASCII85 end-of-data marker `~>`; the emitted `<~ ... ~>` re-parses as a truncated string"
                } else {
                    "contains TILDE (~, 7Eh), outside the ASCII85 alphabet and an early end-of-data for ASCII85Decode"
                };
                return Some((offset, b, reason));
            }
            b'z' => {
                if group_len != 0 {
                    return Some((
                        offset,
                        b,
                        "uses the `z` shorthand inside a partial group; only a whole group of five zero values may be written as `z`",
                    ));
                }
            }
            b'!'..=b'u' => group_len = (group_len + 1) % 5,
            _ => {
                return Some((
                    offset,
                    b,
                    "is outside the ASCII85 alphabet (`!` through `u` plus `z`, with Table 1 white-space allowed anywhere)",
                ));
            }
        }
    }
    None
}

/// 值树（含字典/数组/流字典的嵌套）里首个不可写出的 ASCII85 body 消息。
///
/// 显式栈遍历，与 `types::features::scan_value_features` 同风格，避免深
/// 嵌套递归爆栈；`Value::Stream` 只查字典（流字节是二进制，与本判定无关）。
pub(super) fn first_ascii85_body_error(value: &Value) -> Option<String> {
    let mut stack: Vec<&Value> = vec![value];
    while let Some(v) = stack.pop() {
        match v {
            Value::Ascii85String(body) => {
                if let Some((offset, byte, reason)) = ascii85_body_rejection(body) {
                    return Some(format!(
                        "ASCII85 string body byte offset {offset} is 0x{byte:02x}: {reason} \
                         (ISO 32000-1 7.4.3, raw L1311-1313 of docs/PDF32000_2008.pdf)"
                    ));
                }
            }
            Value::Dict(d) | Value::Stream(d, _) => stack.extend(d.0.values()),
            Value::Array(a) => stack.extend(a.iter()),
            _ => {}
        }
    }
    None
}

/// trailer 字典里首个不可写出的 ASCII85 body，连同其键名。
pub(super) fn first_ascii85_trailer_error(dict: &Dict) -> Option<(String, String)> {
    for (key, value) in &dict.0 {
        if let Some(reason) = first_ascii85_body_error(value) {
            return Some((key.clone(), reason));
        }
    }
    None
}

/// 值是否对象流（`/Type /ObjStm`）容器。
pub(super) fn is_object_stream_container(value: &Value) -> bool {
    matches!(
        value,
        Value::Stream(dict, _) if dict.get("Type").and_then(|v| v.as_name()) == Some("ObjStm")
    )
}

/// 把不可写出的 body 组装成 crate 既有的类型化写错误。
///
/// 复用 [`WriteError::InvalidObject`]：本仓库已把它当作「给定取值无法写出」
/// 的通用类型化错误（`builder/assemble/*` 与 `builder/navigation/*` 的
/// `invalid()` 辅助函数都返回它），无需新增变体、也不破坏既有匹配。
pub(super) fn invalid_ascii85_error(context: &str, reason: String) -> WriteError {
    WriteError::InvalidObject(format!("{context}: {reason}"))
}

/// 文档级 ASCII85 body 预检：全部对象值树 + trailer，首个违规即返回
/// [`WriteError::InvalidObject`]。
///
/// 两个调用场景（同一实现、同一理由）：
/// - **装配期门禁**：`builder/output.rs` 在 `DocumentWriter::build` 之前调用，
///   此时对象号已分配、字节尚未写出，保持仓库的「非法取值零字节」契约；
/// - **直接 `Document` API 的自检**：调用方可在写出前主动过一遍。
///
/// 遍历用 `Document::objects_for_write()`（一次克隆整批对象）：只在容器
/// 与装配期各跑一次，不在逐对象热路径上。
pub(crate) fn validate_ascii85_payloads(doc: &Document) -> Result<(), WriteError> {
    for obj in doc.objects_for_write() {
        if let Some(reason) = first_ascii85_body_error(&obj.value) {
            return Err(invalid_ascii85_error(
                &format!("object {}", obj.num),
                reason,
            ));
        }
    }
    if let Some((key, reason)) = first_ascii85_trailer_error(&doc.trailer.dict) {
        return Err(invalid_ascii85_error(&format!("trailer /{key}"), reason));
    }
    Ok(())
}
