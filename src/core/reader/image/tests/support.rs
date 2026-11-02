//! 测试夹具助手：构造最小文档、流与图像字典。

use crate::core::types::{Array, Dict, Document, Object, StreamData, Trailer, Value};

pub(super) fn dict(entries: Vec<(&str, Value)>) -> Dict {
    let mut dict = Dict::new();
    for (key, value) in entries {
        dict = dict.insert(key, value);
    }
    dict
}

pub(super) fn stream(dict: Dict, data: impl Into<Vec<u8>>) -> Value {
    Value::Stream(dict, StreamData::Owned(data.into()))
}

pub(super) fn doc_with(objects: Vec<(i64, Value)>) -> Document {
    let doc = Document::new();
    for (num, value) in objects {
        doc.set_object(Object::new(num, 0, value));
    }
    doc
}

pub(super) fn image_dict(width: i64, height: i64, space: &str, bits: i64) -> Dict {
    dict(vec![
        ("Type", Value::Name("XObject".into())),
        ("Subtype", Value::Name("Image".into())),
        ("Width", Value::Integer(width)),
        ("Height", Value::Integer(height)),
        ("ColorSpace", Value::Name(space.into())),
        ("BitsPerComponent", Value::Integer(bits)),
    ])
}

/// Catalog 1 → Pages 2 → Page 3；页面可选带 `/Resources`。
pub(super) fn document_with_page(resources: Option<Value>, objects: Vec<(i64, Value)>) -> Document {
    let mut page = dict(vec![
        ("Type", Value::Name("Page".into())),
        ("Parent", Value::Ref(2, 0)),
    ]);
    if let Some(resources) = resources {
        page = page.insert("Resources", resources);
    }
    let doc = doc_with(objects);
    doc.set_object(Object::new(
        1,
        0,
        Value::Dict(dict(vec![
            ("Type", Value::Name("Catalog".into())),
            ("Pages", Value::Ref(2, 0)),
        ])),
    ));
    doc.set_object(Object::new(
        2,
        0,
        Value::Dict(dict(vec![
            ("Type", Value::Name("Pages".into())),
            ("Kids", Value::Array(Array(vec![Value::Ref(3, 0)]))),
            ("Count", Value::Integer(1)),
        ])),
    ));
    doc.set_object(Object::new(3, 0, Value::Dict(page)));
    let mut doc = doc;
    doc.trailer = Trailer {
        dict: Dict::new()
            .insert("Root", Value::Ref(1, 0))
            .insert("Size", Value::Integer(4)),
    };
    doc
}
