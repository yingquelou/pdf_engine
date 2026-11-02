use crate::core::types::{Array, Dict, Document, Object, StreamData, Value};

mod encoding;
mod font_dict;
mod glyph_metrics;
mod glyph_metrics_errors;
mod optional_entries;
mod resources;

fn char_proc() -> Value {
    Value::Stream(Dict::new(), StreamData::Owned(b"0 0 d0".to_vec()))
}

/// 以指定字节构造无过滤器的字形程序流。
fn char_proc_with(bytes: &[u8]) -> Value {
    Value::Stream(Dict::new(), StreamData::Owned(bytes.to_vec()))
}

/// 以指定的 `/CharProcs` 条目构造 Type 3 字体字典（其余条目沿用默认）。
fn type3_dict_with_char_procs(encoding: Value, entries: &[(&str, Value)]) -> Dict {
    let mut procs = Dict::new();
    for (name, value) in entries {
        procs = procs.insert(*name, value.clone());
    }
    type3_dict(encoding).insert("CharProcs", Value::Dict(procs))
}

/// 解析只含字形 A 的字体，返回字符码 65 的 `glyph_metrics` 错误。
fn glyph_metrics_error(proc_bytes: &[u8]) -> super::FontError {
    let document = Document::new();
    let font = Value::Dict(type3_dict_with_char_procs(
        differences_encoding(),
        &[("A", char_proc_with(proc_bytes))],
    ));
    let parsed = document.type3_font(&font).unwrap().unwrap();
    parsed.glyph_metrics(&document, 65).unwrap_err()
}

fn differences_encoding() -> Value {
    Value::Dict(
        Dict::new().insert(
            "Differences",
            Value::Array(
                Array::new()
                    .push(Value::Integer(65))
                    .push(Value::Name("A".into()))
                    .push(Value::Integer(66))
                    .push(Value::Name("B".into())),
            ),
        ),
    )
}

fn type3_dict(encoding: Value) -> Dict {
    Dict::new()
        .insert("Type", Value::Name("Font".into()))
        .insert("Subtype", Value::Name("Type3".into()))
        .insert(
            "FontBBox",
            Value::Array(
                Array::new()
                    .push(Value::Integer(0))
                    .push(Value::Integer(0))
                    .push(Value::Integer(1000))
                    .push(Value::Integer(1000)),
            ),
        )
        .insert(
            "FontMatrix",
            Value::Array(
                Array::new()
                    .push(Value::Float(0.001))
                    .push(Value::Integer(0))
                    .push(Value::Integer(0))
                    .push(Value::Float(0.001))
                    .push(Value::Integer(0))
                    .push(Value::Integer(0)),
            ),
        )
        .insert(
            "CharProcs",
            Value::Dict(
                Dict::new()
                    .insert("B", char_proc())
                    .insert("A", char_proc()),
            ),
        )
        .insert("FirstChar", Value::Integer(65))
        .insert("LastChar", Value::Integer(66))
        .insert(
            "Widths",
            Value::Array(
                Array::new()
                    .push(Value::Integer(500))
                    .push(Value::Integer(600)),
            ),
        )
        .insert("Encoding", encoding)
}

fn type3_font(encoding: Value) -> Value {
    Value::Dict(type3_dict(encoding))
}

fn resources_with_font(font: Value) -> Dict {
    Dict::new().insert("Font", Value::Dict(Dict::new().insert("F1", font)))
}

fn assert_contains(error: super::FontError, needle: &str) {
    let message = error.to_string();
    assert!(
        message.contains(needle),
        "expected error containing {needle:?}, got {message:?}"
    );
}

/// 以指定的 `/Resources` 值构造 Type 3 字体字典。
fn type3_dict_with_resources(resources: Value) -> Dict {
    type3_dict(differences_encoding()).insert("Resources", resources)
}

/// 字形程序可引用的示例资源字典。
fn sample_resources() -> Dict {
    Dict::new()
        .insert(
            "XObject",
            Value::Dict(Dict::new().insert("Im1", Value::Ref(30, 0))),
        )
        .insert(
            "ProcSet",
            Value::Array(Array::new().push(Value::Name("PDF".into()))),
        )
}
