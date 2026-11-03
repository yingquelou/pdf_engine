//! 拒绝路径：`/Type`、数值范围、枚举码、值类型、`/D` `/BM` `/Font`
//! 畸形结构，全部经由 `assert_invalid` 断言错误消息。

use super::*;

#[test]
fn rejects_invalid_type_entry() {
    let document = Document::new();
    assert_invalid(
        &document,
        vec![("Type", Value::Name("Font".into()))],
        "/Type",
    );
}

#[test]
fn rejects_out_of_range_numbers() {
    let document = Document::new();
    for (key, value, needle) in [
        ("LW", Value::Integer(-1), "/LW"),
        ("ML", Value::Float(0.5), "/ML"),
        ("FL", Value::Integer(101), "/FL"),
        ("SM", Value::Float(-0.1), "/SM"),
        ("CA", Value::Float(1.5), "/CA"),
        ("ca", Value::Float(-0.1), "/ca"),
    ] {
        assert_invalid(&document, vec![(key, value)], needle);
    }
}

#[test]
fn rejects_out_of_range_codes() {
    let document = Document::new();
    for (key, value) in [
        ("LC", Value::Integer(3)),
        ("LJ", Value::Integer(-1)),
        ("OPM", Value::Integer(2)),
        ("LC", Value::Float(1.5)),
    ] {
        assert_invalid(&document, vec![(key, value)], &format!("/{key}"));
    }
}

#[test]
fn rejects_mismatched_value_types() {
    let document = Document::new();
    for (key, value) in [
        ("LW", Value::Name("thin".into())),
        ("OP", Value::Integer(1)),
        ("RI", Value::String("AbsoluteColorimetric".into())),
        ("BM", Value::Integer(1)),
        ("Font", Value::Dict(Dict::new())),
    ] {
        assert_invalid(&document, vec![(key, value)], &format!("/{key}"));
    }
}

#[test]
fn rejects_malformed_dash_patterns() {
    let document = Document::new();
    for dash in [
        Value::Integer(1),
        Value::Array(Array(vec![Value::Array(Array::new())])),
        Value::Array(Array(vec![
            Value::Array(Array(vec![Value::Integer(-1)])),
            Value::Integer(0),
        ])),
        Value::Array(Array(vec![
            Value::Array(Array(vec![Value::Integer(1)])),
            Value::Integer(-1),
        ])),
        Value::Array(Array(vec![
            Value::Array(Array(vec![Value::Name("dash".into())])),
            Value::Integer(0),
        ])),
    ] {
        assert_invalid(&document, vec![("D", dash)], "/D");
    }
}

#[test]
fn rejects_malformed_blend_mode_arrays() {
    let document = Document::new();
    for blend_mode in [
        Value::Array(Array::new()),
        Value::Array(Array(vec![Value::Integer(1)])),
    ] {
        assert_invalid(&document, vec![("BM", blend_mode)], "/BM");
    }
}

#[test]
fn rejects_malformed_font_entries() {
    let document = document_with_objects(vec![(9, Value::Integer(1))]);
    for font in [
        Value::Array(Array(vec![Value::Ref(9, 0), Value::Integer(12)])),
        Value::Array(Array(vec![Value::Dict(Dict::new()), Value::Integer(0)])),
        Value::Array(Array(vec![Value::Dict(Dict::new())])),
        Value::Array(Array(vec![
            Value::Dict(Dict::new()),
            Value::Name("large".into()),
        ])),
    ] {
        assert_invalid(&document, vec![("Font", font)], "/Font");
    }
}
