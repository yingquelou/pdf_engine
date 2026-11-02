//! 逻辑结构树读端测试（ISO 32000-1 14.7.1–14.7.4.4）。

use crate::core::reader::structure::StructureError;
use crate::core::types::{Array, Dict, Document, Object, Trailer, Value};

const CATALOG: i64 = 1;
const ROOT: i64 = 2;
const PAGE: i64 = 10;

pub(super) fn arr(values: Vec<Value>) -> Value {
    Value::Array(Array(values))
}

pub(super) fn name(value: &str) -> Value {
    Value::Name(value.to_owned())
}

pub(super) fn insert(doc: &Document, num: i64, value: Value) {
    doc.set_object(Object::new(num, 0, value));
}

pub(super) fn element(structure_type: &str, parent: i64) -> Dict {
    Dict::new()
        .insert("Type", name("StructElem"))
        .insert("S", name(structure_type))
        .insert("P", Value::Ref(parent, 0))
}

pub(super) fn struct_root(entries: Dict) -> Dict {
    entries.insert("Type", name("StructTreeRoot"))
}

/// Catalog 1 → StructTreeRoot 2。
pub(super) fn document_with_root(root: Dict) -> Document {
    let mut doc = Document::new();
    insert(&doc, ROOT, Value::Dict(root));
    insert(
        &doc,
        CATALOG,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Catalog"))
                .insert("StructTreeRoot", Value::Ref(ROOT, 0)),
        ),
    );
    doc.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(CATALOG, 0)),
    };
    doc
}

fn assert_malformed(doc: &Document, needle: &str) {
    match doc.structure_tree() {
        Err(StructureError::Malformed(message)) => {
            assert!(
                message.contains(needle),
                "expected {needle:?}, got {message}"
            );
        }
        other => panic!("expected malformed error containing {needle:?}, got {other:?}"),
    }
}

fn malformed_document(kid: Value) -> Document {
    let doc = document_with_root(struct_root(
        Dict::new().insert("K", arr(vec![Value::Ref(5, 0)])),
    ));
    insert(&doc, 5, kid);
    doc
}

/// 单个顶层结构元素，`/A` 为给定属性对象字典。
fn attribute_document(attribute: Dict) -> Document {
    malformed_document(Value::Dict(
        element("P", ROOT).insert("A", Value::Dict(attribute)),
    ))
}

/// 用户属性属性对象（Table 328）。
fn user_properties(properties: Value) -> Dict {
    Dict::new()
        .insert("O", name("UserProperties"))
        .insert("P", properties)
}

/// 用户属性字典（Table 329）。
fn user_property(property_name: &str, value: Value) -> Dict {
    Dict::new()
        .insert("N", Value::String(property_name.to_owned()))
        .insert("V", value)
}

/// 最小 Layout 属性对象。
fn layout_attribute(space_before: i64) -> Dict {
    Dict::new()
        .insert("O", name("Layout"))
        .insert("SpaceBefore", Value::Integer(space_before))
}

mod attributes;
mod element_tree;
mod malformed;
mod mark_info;
mod parent_tree;
mod user_properties;
