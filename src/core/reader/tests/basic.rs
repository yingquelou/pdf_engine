//! 基础解析器测试：词法 token、对象语法、xref/trailer、注释与空白。

use super::*;
use crate::core::types::{StreamData, Value};

#[test]
fn parse_int_and_float() {
    let mut lex = Lexer::new(b"42 3.5 -7 +1");
    assert_eq!(parse_object(&mut lex).unwrap(), Value::Integer(42));
    assert_eq!(parse_object(&mut lex).unwrap(), Value::Float(3.5));
    assert_eq!(parse_object(&mut lex).unwrap(), Value::Integer(-7));
    assert_eq!(parse_object(&mut lex).unwrap(), Value::Integer(1));
}

#[test]
fn parse_name_and_string() {
    let mut lex = Lexer::new(b"/Type (hello\\nworld) <48656C6C6F>");
    let v = parse_object(&mut lex).unwrap();
    assert_eq!(v, Value::Name("Type".to_string()));
    let v = parse_object(&mut lex).unwrap();
    assert_eq!(v, Value::String("hello\nworld".to_string()));
    let v = parse_object(&mut lex).unwrap();
    assert_eq!(v, Value::HexString("48656C6C6F".to_string()));
}

#[test]
fn parse_bool_null_ref() {
    let mut lex = Lexer::new(b"true false null 12 0 R");
    assert_eq!(parse_object(&mut lex).unwrap(), Value::Bool(true));
    assert_eq!(parse_object(&mut lex).unwrap(), Value::Bool(false));
    assert_eq!(parse_object(&mut lex).unwrap(), Value::Null);
    assert_eq!(parse_object(&mut lex).unwrap(), Value::Ref(12, 0));
}

#[test]
fn parse_array_and_dict() {
    let mut lex = Lexer::new(b"[1 2 [3 4]] << /A 1 /B /C >>");
    let v = parse_object(&mut lex).unwrap();
    let a = v.as_array().unwrap();
    assert_eq!(a.len(), 3);
    assert_eq!(a.iter().nth(2).unwrap().as_array().unwrap().len(), 2);
    let v = parse_object(&mut lex).unwrap();
    let d = v.as_dict().unwrap();
    assert_eq!(d.get("A").and_then(|v| v.as_integer()), Some(1));
    assert_eq!(d.get("B").and_then(|v| v.as_name()), Some("C"));
}

#[test]
fn parse_indirect_object_simple() {
    let mut lex = Lexer::new(b"1 0 obj 42 endobj");
    let o = parse_indirect_object(&mut lex).unwrap();
    assert_eq!(o.num, 1);
    assert_eq!(o.generation, 0);
    assert_eq!(o.value, Value::Integer(42));
}

/// 深嵌套数组触发深度上限：不得栈溢出，应返回 `Err`；
/// 合法深度（32 层）仍正常解析。
#[test]
fn deep_nesting_hits_limit_gracefully() {
    let deep = format!("{}{}", "[".repeat(10_000), "]".repeat(10_000));
    let mut lex = Lexer::new(deep.as_bytes());
    assert!(parse_object(&mut lex).is_err(), "超限嵌套应报错而非栈溢出");

    let ok = format!("{}{}", "[".repeat(32), "]".repeat(32));
    let mut lex = Lexer::new(ok.as_bytes());
    assert!(parse_object(&mut lex).is_ok(), "限内嵌套应正常解析");
}

#[test]
fn parse_indirect_object_stream_ref() {
    // 流对象：compute_stream_range 只算范围，不读字节。
    let raw = b"2 0 obj\n<< /Len 5 >>\nstream\nhello\nendstream\nendobj";
    let mut lex = Lexer::new(raw);
    let o = parse_indirect_object(&mut lex).unwrap();
    match o.value {
        Value::Stream(_, StreamData::Ref { offset, len }) => {
            // offset 指向 'h' 的绝对偏移（hello 的起始）
            assert_eq!(&raw[offset as usize..(offset + len) as usize], b"hello");
            assert_eq!(len, 5);
        }
        other => panic!("expected stream ref, got {:?}", other),
    }
}

#[test]
fn stream_length_prevents_embedded_endstream_truncation() {
    let payload = b"before endstream after";
    let raw = [
        format!("2 0 obj\n<< /Length {} >>\nstream\n", payload.len()).as_bytes(),
        payload,
        b"\nendstream\nendobj",
    ]
    .concat();
    let mut lex = Lexer::new(&raw);
    let object = parse_indirect_object(&mut lex).unwrap();
    match object.value {
        Value::Stream(_, StreamData::Ref { offset, len }) => {
            assert_eq!(len as usize, payload.len());
            assert_eq!(&raw[offset as usize..(offset + len) as usize], payload);
        }
        other => panic!("expected stream ref, got {other:?}"),
    }
}

#[test]
fn parse_xref_and_trailer() {
    let raw =
        b"xref\n0 2\n0000000000 65535 f \n0000000009 00000 n \ntrailer\n<< /Size 2 /Root 1 0 R >>";
    let mut lex = Lexer::new(raw);
    let xref = parse_xref(&mut lex).unwrap();
    assert_eq!(xref.0.len(), 1);
    assert_eq!(xref.0[0].start, 0);
    assert_eq!(xref.0[0].entries.len(), 2);

    let t = parse_trailer(&mut lex).unwrap();
    assert_eq!(t.size(), Some(2));
    assert_eq!(t.root(), Some((1, 0)));
}

#[test]
fn skip_comments_and_whitespace() {
    let mut lex = Lexer::new(b"% header comment\n  42   % trailing\n");
    assert_eq!(parse_object(&mut lex).unwrap(), Value::Integer(42));
}

#[test]
fn nul_byte_treated_as_whitespace() {
    // ISO 32000-1 §7.2：0x00 NUL 属于空白字符，token 间的 NUL 应被跳过。
    let mut lex = Lexer::new(b"\x00\x00 true \x00false");
    assert_eq!(parse_object(&mut lex).unwrap(), Value::Bool(true));
    assert_eq!(parse_object(&mut lex).unwrap(), Value::Bool(false));
    // Name 以 NUL 终止
    let mut lex = Lexer::new(b"/Na\x00me");
    assert_eq!(
        parse_object(&mut lex).unwrap(),
        Value::Name("Na".to_string())
    );
}
