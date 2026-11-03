//! 标准结构属性读端测试（ISO 32000-1 14.8.5.2、14.8.5.5–14.8.5.7）。

use crate::core::reader::structure::{
    FormatSpecificOwner, ListNumbering, PrintFieldChecked, PrintFieldRole, StandardAttributeOwner,
    StructureError, TableScope,
};
use crate::core::types::{Array, Dict, Document, Object, Trailer, Value};

const CATALOG: i64 = 1;
const ROOT: i64 = 2;

pub(super) fn arr(values: Vec<Value>) -> Value {
    Value::Array(Array(values))
}

pub(super) fn name(value: &str) -> Value {
    Value::Name(value.to_owned())
}

pub(super) fn insert(doc: &Document, num: i64, value: Value) {
    doc.set_object(Object::new(num, 0, value));
}

pub(super) fn element(structure_type: &str) -> Dict {
    Dict::new()
        .insert("Type", name("StructElem"))
        .insert("S", name(structure_type))
        .insert("P", Value::Ref(ROOT, 0))
}

pub(super) fn struct_root(entries: Dict) -> Dict {
    entries.insert("Type", name("StructTreeRoot"))
}

/// Catalog 1 → StructTreeRoot 2，顶层元素按给定对象号引用。
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

/// 若干顶层元素：每个 (对象号, 结构元素字典)。
pub(super) fn document_with_elements(elements: Vec<(i64, Dict)>) -> Document {
    let kids = arr(elements
        .iter()
        .map(|(number, _)| Value::Ref(*number, 0))
        .collect());
    let doc = document_with_root(struct_root(Dict::new()).insert("K", kids));
    for (number, element) in elements {
        insert(&doc, number, Value::Dict(element));
    }
    doc
}

/// 单个顶层元素，`/A` 为给定属性对象字典。
pub(super) fn attribute_document(attribute: Dict) -> Document {
    document_with_elements(vec![(5, element("P").insert("A", Value::Dict(attribute)))])
}

pub(super) fn assert_malformed(doc: &Document, needle: &str) {
    match doc
        .structure_tree()
        .expect("structure tree")
        .expect("tree present")
        .standard_attributes(0)
    {
        Err(StructureError::Malformed(message)) => {
            assert!(
                message.contains(needle),
                "expected {needle:?}, got {message}"
            );
        }
        other => panic!("expected malformed error containing {needle:?}, got {other:?}"),
    }
}

mod format_specific;
mod list;
mod owners;
mod print_field;
mod table;
