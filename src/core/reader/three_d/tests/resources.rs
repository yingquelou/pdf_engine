//! 3D 流 `/Resources` 名称树（13.6.3.1 Table 300）读端测试：文本串键
//! 解码、`/Kids` 嵌套、值原样保留与畸形树拒绝。

use std::collections::BTreeMap;

use crate::core::types::{Array, Dict, Document, Object, Trailer, Value, Xref};

use super::super::ThreeDError;
use super::super::stream::parse_stream;
use super::{document, stream};

fn parse_resources(
    document: &Document,
    value: Value,
) -> Result<Option<BTreeMap<String, Value>>, ThreeDError> {
    parse_stream(
        document,
        &stream("U3D").insert("Resources", value),
        "3D stream",
    )
    .map(|parsed| parsed.stream.resources)
}

#[test]
fn stream_resources_read_text_string_name_tree() {
    let kid = Dict::new().insert(
        "Names",
        Value::Array(Array(vec![
            Value::String("alpha".into()),
            Value::Integer(1),
            Value::HexString("FEFF00E9".into()),
            Value::Ref(7, 0),
        ])),
    );
    let sibling = Dict::new().insert(
        "Names",
        Value::Array(Array(vec![
            Value::String("beta".into()),
            Value::Dict(Dict::new().insert("Tool", Value::Name("Orbit".into()))),
        ])),
    );
    let root = Dict::new().insert(
        "Kids",
        Value::Array(Array(vec![Value::Ref(10, 0), Value::Dict(sibling)])),
    );
    let document = Document::from_objects(
        vec![Object::new(10, 0, Value::Dict(kid))],
        Xref::new(),
        Trailer::new(),
        Default::default(),
    );

    let resources = parse_resources(&document, Value::Dict(root))
        .expect("name tree")
        .expect("/Resources");
    assert_eq!(
        resources.keys().map(String::as_str).collect::<Vec<_>>(),
        ["alpha", "beta", "é"],
        "键按解码后的文本串排序"
    );
    assert_eq!(resources["alpha"], Value::Integer(1));
    assert_eq!(
        resources["é"],
        Value::Ref(7, 0),
        "名称树值原样保留，不解析间接引用"
    );
    assert_eq!(
        resources["beta"],
        Value::Dict(Dict::new().insert("Tool", Value::Name("Orbit".into())))
    );
}

#[test]
fn stream_resources_reject_malformed_trees() {
    let document = document();
    let names = |entries: Vec<Value>| {
        Value::Dict(Dict::new().insert("Names", Value::Array(Array(entries))))
    };
    for (value, fragment) in [
        (Value::Integer(1), "is not a name tree dictionary"),
        (names(vec![Value::String("a".into())]), "length is not even"),
        (
            names(vec![Value::Integer(1), Value::Integer(2)]),
            "key is not a string",
        ),
        (
            names(vec![Value::HexString("7F".into()), Value::Integer(1)]),
            "name is not a valid text string",
        ),
        (
            names(vec![
                Value::String("A".into()),
                Value::Integer(1),
                Value::HexString("FEFF0041".into()),
                Value::Integer(2),
            ]),
            "duplicate name",
        ),
        (Value::Dict(Dict::new()), "neither /Kids nor /Names"),
    ] {
        let error = parse_resources(&document, value).expect_err("malformed name tree");
        assert!(
            matches!(error, ThreeDError::Malformed(_)),
            "expected Malformed, got {error:?}"
        );
        assert!(error.to_string().contains(fragment), "{error}");
    }
}

#[test]
fn stream_resources_absent_is_none() {
    let parsed = parse_stream(&document(), &stream("U3D"), "3D stream").expect("stream");
    assert_eq!(parsed.stream.resources, None);
}
