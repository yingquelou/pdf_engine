//! E4 读端表单测试。

mod actions;
mod alternate_name;
mod choice;
mod errors;
mod flags;
mod language_escape;
mod mapping_name_escape;
mod merged;
mod options;
mod resources;
mod tree_structure;

use super::*;
use crate::core::reader::color_space::ColorSpaceModel;
use crate::core::reader::graphics_state::BlendMode;
use crate::core::types::{Array, Object, PdfTextString, Trailer};

const PAGES_OBJECT: i64 = 2;

fn name(value: &str) -> Value {
    Value::Name(value.into())
}

fn text(value: &str) -> Value {
    Value::HexString(
        PdfTextString::from(value)
            .encode()
            .iter()
            .map(|byte| format!("{byte:02X}"))
            .collect(),
    )
}

fn arr(values: Vec<Value>) -> Value {
    Value::Array(Array(values))
}

fn insert(doc: &Document, num: i64, value: Value) {
    doc.set_object(Object::new(num, 0, value));
}

fn doc_with_catalog(catalog: Dict) -> Document {
    let mut doc = Document::new();
    insert(
        &doc,
        1,
        Value::Dict(
            catalog
                .insert("Type", name("Catalog"))
                .insert("Pages", Value::Ref(PAGES_OBJECT, 0)),
        ),
    );
    insert(
        &doc,
        PAGES_OBJECT,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Pages"))
                .insert("Kids", arr(Vec::new()))
                .insert("Count", Value::Integer(0)),
        ),
    );
    doc.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(1, 0)),
    };
    doc
}

fn doc_with_acro_form(acro: Value) -> Document {
    doc_with_catalog(Dict::new().insert("AcroForm", acro))
}

fn form_error(acro: Value) -> FormError {
    let doc = doc_with_acro_form(acro);
    doc.acro_form().expect_err("form should be rejected")
}

fn widget(rect: Value) -> Dict {
    Dict::new()
        .insert("Type", name("Annot"))
        .insert("Subtype", name("Widget"))
        .insert("Rect", rect)
}

fn javascript(script: &str) -> Value {
    Value::Dict(
        Dict::new()
            .insert("S", name("JavaScript"))
            .insert("JS", text(script)),
    )
}
