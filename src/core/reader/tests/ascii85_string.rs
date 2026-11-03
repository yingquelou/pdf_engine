//! 7.3.4 扩展语法：`<~ ~>` ASCII85 字符串字面量的**读端**解析路径。
//!
//! 口径要点（与写端 `writer/value.rs` 的 `Value::Ascii85String` 分支核对过）：
//! 写端把 payload **原样**夹在 `<~` 与 `~>` 之间、不做任何转义，所以读端
//! 也逐字收集其间字符、只剥两个定界符；payload 是 ASCII85 *编码文本*，
//! 不是解码后的字节。`<~ ~>` 是**非 ISO 32000-1 语法**（标准只定义
//! ASCII85 过滤器 7.4.3），故这里只做生产者实态的互操作读取，
//! 不进 `PdfFeature`、不做版本门禁。

use super::*;
use crate::core::filters;
use crate::core::reader::lexer::Token;
use crate::core::types::{Dict, Value};

/// 既有的公共解码入口（本车道不改 `filters/**`）：把 ASCII85 编码文本
/// 解成字节，与 `reader/signature/bytes.rs` 的 `pdf_string_bytes` 同一路径。
fn decode_ascii85_payload(payload: &str) -> Vec<u8> {
    let dict = Dict::new().insert("Filter", Value::Name("ASCII85Decode".into()));
    filters::decode_stream(&dict, payload.as_bytes()).expect("ASCII85Decode should decode payload")
}

/// 读端此前完全没有 `<~ ~>` 的 Token/解析路径（类型为死代码）：
/// 现在必须产出 `Value::Ascii85String`，且 payload 逐字等于定界符之间的原文。
#[test]
fn lexer_tokenizes_ascii85_string_and_maps_it_to_the_value_variant() {
    let mut lex = Lexer::new(b"<~9jqo^~>");
    assert_eq!(
        lex.next_token().unwrap(),
        Token::Ascii85("9jqo^".to_string())
    );

    let mut lex = Lexer::new(b"<~9jqo^~>");
    assert_eq!(
        parse_object(&mut lex).unwrap(),
        Value::Ascii85String("9jqo^".to_string())
    );
}

/// 边界：空串 `<~~>` 与纯空白串都合法，payload 为定界符之间的原文。
#[test]
fn ascii85_string_empty_delimiters_parse_to_empty_payload() {
    let mut lex = Lexer::new(b"<~~>");
    assert_eq!(
        parse_object(&mut lex).unwrap(),
        Value::Ascii85String(String::new())
    );

    let mut lex = Lexer::new(b"<~ ~>");
    assert_eq!(
        parse_object(&mut lex).unwrap(),
        Value::Ascii85String(" ".to_string())
    );
}

/// `~` 本身是 ASCII85 的「数据结束」标记，故 `~>` 前允许杂散 `~`
/// （`~~>` 形态）：解析须停在这里，不吃掉后续 token，也不 panic。
#[test]
fn ascii85_string_terminator_after_stray_tilde_stops_without_eating_next_token() {
    let mut lex = Lexer::new(b"<~9jqo^~~> /Next");
    assert_eq!(
        parse_object(&mut lex).unwrap(),
        Value::Ascii85String("9jqo^~".to_string())
    );
    // 关键证据：终止符后的 `/Next` 仍是独立 token，未被串吞掉。
    assert_eq!(lex.next_token().unwrap(), Token::Name("Next".to_string()));
    // 杂散 `~` 留在 payload 内不影响解码：base-85 解码器把 `~` 当数据结束。
    assert_eq!(decode_ascii85_payload("9jqo^~"), b"Man ");
}

/// 尾部/内部混入空白与换行时按原样保留，保证「写出→读回」字节对称。
#[test]
fn ascii85_string_keeps_whitespace_and_newlines_verbatim_in_payload() {
    let mut lex = Lexer::new(b"<~9jqo^\n ~>\n42");
    assert_eq!(
        parse_object(&mut lex).unwrap(),
        Value::Ascii85String("9jqo^\n ".to_string())
    );
    assert_eq!(parse_object(&mut lex).unwrap(), Value::Integer(42));
    assert_eq!(decode_ascii85_payload("9jqo^\n "), b"Man ");
}

/// 未闭合（窗口末尾没有 `~>`）：类型化词法错误，与既有
/// `unterminated literal string` / `unterminated hex string` 同风格。
#[test]
fn ascii85_string_unterminated_at_window_end_is_a_typed_lexical_error() {
    let mut lex = Lexer::new(b"<~9jqo^");
    let e = parse_object(&mut lex).unwrap_err();
    assert_eq!(e.message, "unterminated ascii85 string");

    // 未闭合的串后头还跟着别的对象文本：不得静默成功、不得把后续对象
    // 当合法值返回。
    let mut lex = Lexer::new(b"<~9jqo^ 4 0 obj /Type /Catalog endobj");
    let e = parse_object(&mut lex).unwrap_err();
    assert_eq!(e.message, "unterminated ascii85 string");
}

/// 末尾只有一个 `~`（没有 `>`）也不得 panic / 无限循环。
#[test]
fn ascii85_string_unterminated_never_panics_or_loops_on_trailing_tilde() {
    for body in ["<~", "<~a", "<~abc~", "<~abc~~", "<~~"] {
        let mut lex = Lexer::new(body.as_bytes());
        let e = parse_object(&mut lex)
            .expect_err("unterminated ascii85 string must error")
            .clone();
        assert_eq!(e.message, "unterminated ascii85 string", "body {body:?}");
    }
}

/// 容器内形态：数组/字典/间接对象里都能读回该变体（值工厂映射完整）。
#[test]
fn ascii85_string_parses_inside_array_dict_and_indirect_object() {
    let mut lex = Lexer::new(b"[<~9jqo^~> <~z~>]");
    let v = parse_object(&mut lex).unwrap();
    let arr = v.as_array().unwrap();
    assert_eq!(
        arr.iter().cloned().collect::<Vec<_>>(),
        vec![
            Value::Ascii85String("9jqo^".into()),
            Value::Ascii85String("z".into())
        ]
    );

    let mut lex = Lexer::new(b"<< /S <~9jqo^~> >>");
    let d = parse_object(&mut lex).unwrap();
    assert_eq!(
        d.as_dict().unwrap().get("S"),
        Some(&Value::Ascii85String("9jqo^".into()))
    );

    let mut lex = Lexer::new(b"7 0 obj\n<< /Contents <~9jqo^~> >>\nendobj");
    let o = parse_indirect_object(&mut lex).unwrap();
    assert_eq!(o.num, 7);
    assert_eq!(
        o.value.as_dict().unwrap().get("Contents"),
        Some(&Value::Ascii85String("9jqo^".into()))
    );
}

/// 新增 `<~` 判别不得改变既有 `<<`（字典开）与 `<hex>`（十六进制串）形态。
#[test]
fn lexer_keeps_dict_and_hex_forms_after_adding_the_ascii85_form() {
    let mut lex = Lexer::new(b"<< <48656C6C6F> >>");
    assert_eq!(lex.next_token().unwrap(), Token::DictOpen);
    assert_eq!(lex.next_token().unwrap(), Token::Hex("48656C6C6F".into()));
    assert_eq!(lex.next_token().unwrap(), Token::DictClose);
}

/// payload 的语义口径证据：存的是 ASCII85 **编码文本**，经既有过滤器
/// 公共入口解码（`z` 短路、末组补齐都要能用）。
#[test]
fn ascii85_string_payload_decodes_through_the_existing_ascii85_filter_entry() {
    let mut lex = Lexer::new(b"<~z~>");
    let v = parse_object(&mut lex).unwrap();
    let Value::Ascii85String(payload) = &v else {
        panic!("expected ascii85-string, got {}", v.type_name());
    };
    assert_eq!(decode_ascii85_payload(payload), vec![0, 0, 0, 0]);
    assert_eq!(v.type_name(), "ascii85-string");
}
