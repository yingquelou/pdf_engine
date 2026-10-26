//! Page content parsing tests.

use super::*;
use crate::core::types::{Array, Dict, Object, StreamData, Trailer, Value};

fn make_simple_doc() -> Document {
    let mut doc = Document::new();
    doc.set_object(Object::new(
        1,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Catalog".into()))
                .insert("Pages", Value::Ref(2, 0)),
        ),
    ));
    doc.set_object(Object::new(
        2,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Pages".into()))
                .insert("Kids", Value::Array(Array::new().push(Value::Ref(3, 0))))
                .insert("Count", Value::Integer(1)),
        ),
    ));
    doc.set_object(Object::new(
        3,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Page".into()))
                .insert("Parent", Value::Ref(2, 0))
                .insert("Resources", Value::Dict(Dict::new()))
                .insert("Contents", Value::Ref(4, 0)),
        ),
    ));
    doc.set_object(Object::new(
        4,
        0,
        Value::Stream(
            Dict::new(),
            StreamData::Owned(b"q 1 0 0 1 100 200 cm Q".to_vec()),
        ),
    ));
    doc.trailer = Trailer {
        dict: Dict::new()
            .insert("Root", Value::Ref(1, 0))
            .insert("Size", Value::Integer(5)),
    };
    doc
}

#[test]
fn page_content_ops_parses_simple_page() {
    let doc = make_simple_doc();
    let ops = doc.page_content_ops(0).unwrap();
    assert_eq!(ops.len(), 3);
    assert!(matches!(ops[0], Op::SaveState));
    assert!(matches!(
        ops[1],
        Op::ConcatMatrix([1.0, 0.0, 0.0, 1.0, 100.0, 200.0])
    ));
    assert!(matches!(ops[2], Op::RestoreState));
}

#[test]
fn page_content_ops_returns_error_for_invalid_page_index() {
    let doc = make_simple_doc();
    let result = doc.page_content_ops(999);
    assert!(matches!(result, Err(PageContentError::PageNotFound)));
}

#[test]
fn page_resource_resolves_font_name() {
    let mut doc = Document::new();
    doc.set_object(Object::new(
        1,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Catalog".into()))
                .insert("Pages", Value::Ref(2, 0)),
        ),
    ));
    doc.set_object(Object::new(
        2,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Pages".into()))
                .insert("Kids", Value::Array(Array::new().push(Value::Ref(3, 0))))
                .insert("Count", Value::Integer(1)),
        ),
    ));
    let resources = Dict::new().insert(
        "Font",
        Value::Dict(Dict::new().insert("F1", Value::Name("TestFont".into()))),
    );
    doc.set_object(Object::new(
        3,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Page".into()))
                .insert("Parent", Value::Ref(2, 0))
                .insert("Resources", Value::Dict(resources))
                .insert(
                    "Contents",
                    Value::Stream(Dict::new(), StreamData::Owned(b"BT ET".to_vec())),
                ),
        ),
    ));
    doc.trailer = Trailer {
        dict: Dict::new()
            .insert("Root", Value::Ref(1, 0))
            .insert("Size", Value::Integer(4)),
    };

    let font_value = doc.page_resource(0, "Font", "F1").unwrap();
    assert!(font_value.is_some());
}

#[test]
fn page_resource_inherits_from_parent() {
    let mut doc = Document::new();
    doc.set_object(Object::new(
        1,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Catalog".into()))
                .insert("Pages", Value::Ref(2, 0)),
        ),
    ));
    // Resources live only on the Pages node; the page has no /Resources entry.
    let resources = Dict::new().insert(
        "Font",
        Value::Dict(Dict::new().insert("F1", Value::Name("InheritedFont".into()))),
    );
    doc.set_object(Object::new(
        2,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Pages".into()))
                .insert("Kids", Value::Array(Array::new().push(Value::Ref(3, 0))))
                .insert("Count", Value::Integer(1))
                .insert("Resources", Value::Dict(resources)),
        ),
    ));
    doc.set_object(Object::new(
        3,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Page".into()))
                .insert("Parent", Value::Ref(2, 0))
                .insert(
                    "Contents",
                    Value::Stream(Dict::new(), StreamData::Owned(b"BT ET".to_vec())),
                ),
        ),
    ));
    doc.trailer = Trailer {
        dict: Dict::new()
            .insert("Root", Value::Ref(1, 0))
            .insert("Size", Value::Integer(4)),
    };

    let font_value = doc.page_resource(0, "Font", "F1").unwrap();
    assert_eq!(
        font_value.and_then(|v| v.as_name().map(String::from)),
        Some("InheritedFont".to_string())
    );
}

#[test]
fn page_resource_returns_none_for_missing_resource() {
    let doc = make_simple_doc();
    let result = doc.page_resource(0, "Font", "NonExistent").unwrap();
    assert!(result.is_none());
}

#[test]
fn page_resource_decodes_name_escapes() {
    let mut doc = Document::new();
    doc.set_object(Object::new(
        1,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Catalog".into()))
                .insert("Pages", Value::Ref(2, 0)),
        ),
    ));
    doc.set_object(Object::new(
        2,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Pages".into()))
                .insert("Kids", Value::Array(Array::new().push(Value::Ref(3, 0))))
                .insert("Count", Value::Integer(1)),
        ),
    ));
    // Resource name with #20 (space) escape
    let resources = Dict::new().insert(
        "Font",
        Value::Dict(Dict::new().insert("F#201", Value::Name("TestFont".into()))),
    );
    doc.set_object(Object::new(
        3,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Page".into()))
                .insert("Parent", Value::Ref(2, 0))
                .insert("Resources", Value::Dict(resources))
                .insert(
                    "Contents",
                    Value::Stream(Dict::new(), StreamData::Owned(b"BT ET".to_vec())),
                ),
        ),
    ));
    doc.trailer = Trailer {
        dict: Dict::new()
            .insert("Root", Value::Ref(1, 0))
            .insert("Size", Value::Integer(4)),
    };

    let font = doc.page_resource(0, "Font", "F 1").unwrap();
    assert!(font.is_some());
}

#[test]
fn page_content_ops_handles_contents_array() {
    let mut doc = Document::new();
    doc.set_object(Object::new(
        1,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Catalog".into()))
                .insert("Pages", Value::Ref(2, 0)),
        ),
    ));
    doc.set_object(Object::new(
        2,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Pages".into()))
                .insert("Kids", Value::Array(Array::new().push(Value::Ref(3, 0))))
                .insert("Count", Value::Integer(1)),
        ),
    ));
    doc.set_object(Object::new(
        3,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Page".into()))
                .insert("Parent", Value::Ref(2, 0))
                .insert("Resources", Value::Dict(Dict::new()))
                .insert(
                    "Contents",
                    Value::Array(Array::new().push(Value::Ref(4, 0)).push(Value::Ref(5, 0))),
                ),
        ),
    ));
    doc.set_object(Object::new(
        4,
        0,
        Value::Stream(Dict::new(), StreamData::Owned(b"q".to_vec())),
    ));
    doc.set_object(Object::new(
        5,
        0,
        Value::Stream(Dict::new(), StreamData::Owned(b"Q".to_vec())),
    ));
    doc.trailer = Trailer {
        dict: Dict::new()
            .insert("Root", Value::Ref(1, 0))
            .insert("Size", Value::Integer(6)),
    };

    let ops = doc.page_content_ops(0).unwrap();
    assert_eq!(ops.len(), 2);
    assert!(matches!(ops[0], Op::SaveState));
    assert!(matches!(ops[1], Op::RestoreState));
}

#[test]
fn decode_name_escapes_handles_various_cases() {
    assert_eq!(decode_name_escapes("F#201"), "F 1");
    assert_eq!(decode_name_escapes("Normal"), "Normal");
    assert_eq!(decode_name_escapes("A#42B"), "ABB");
    assert_eq!(decode_name_escapes("#"), "#");
    assert_eq!(decode_name_escapes("#2"), "#2");
}
