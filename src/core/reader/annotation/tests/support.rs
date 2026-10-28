//! 注读端测试共享构造辅助：最小目录/页面树与注解字典。

use crate::core::reader::annotation::Document;
use crate::core::types::{Array, Dict, Object, Trailer, Value, Xref};

pub(super) fn catalog_object() -> Object {
    Object::new(
        1,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Catalog".into()))
                .insert("Pages", Value::Ref(2, 0)),
        ),
    )
}

pub(super) fn pages_object(kids: Vec<Value>) -> Object {
    Object::new(
        2,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Pages".into()))
                .insert("Kids", Value::Array(Array(kids))),
        ),
    )
}

pub(super) fn page_object(annots: Option<Vec<Value>>) -> Object {
    let mut dict = Dict::new()
        .insert("Type", Value::Name("Page".into()))
        .insert("Parent", Value::Ref(2, 0));
    if let Some(annots) = annots {
        dict = dict.insert("Annots", Value::Array(Array(annots)));
    }
    Object::new(3, 0, Value::Dict(dict))
}

pub(super) fn doc(objects: Vec<Object>) -> Document {
    let mut trailer = Trailer::new();
    trailer.dict = trailer.dict.clone().insert("Root", Value::Ref(1, 0));
    Document::from_objects(objects, Xref::new(), trailer, Default::default())
}

pub(super) fn doc_with_annots(annots: Vec<Value>, extra: Vec<Object>) -> Document {
    let mut objects = vec![
        catalog_object(),
        pages_object(vec![Value::Ref(3, 0)]),
        page_object(Some(annots)),
    ];
    objects.extend(extra);
    doc(objects)
}

pub(super) fn base_annotation(subtype: &str) -> Dict {
    Dict::new()
        .insert("Type", Value::Name("Annot".into()))
        .insert("Subtype", Value::Name(subtype.into()))
        .insert(
            "Rect",
            Value::Array(Array(vec![
                Value::Integer(10),
                Value::Float(20.5),
                Value::Integer(30),
                Value::Float(40.0),
            ])),
        )
}
