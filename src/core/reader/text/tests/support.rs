//! Test fixtures shared across the theme files.

use crate::core::types::{
    Array, Dict, Document, Object, StartXref, StreamData, Trailer, Value, Xref,
};

pub(super) fn winansi_type1_font() -> Value {
    Value::Dict(
        Dict::new()
            .insert("Type", Value::Name("Font".into()))
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("Helvetica".into()))
            .insert("Encoding", Value::Name("WinAnsiEncoding".into())),
    )
}

pub(super) fn resources_with_font(name: &str, font: Value) -> Dict {
    Dict::new().insert("Font", Value::Dict(Dict::new().insert(name, font)))
}

pub(super) fn differences_type1_font() -> Value {
    let differences = Array::new()
        .push(Value::Integer(65))
        .push(Value::Name("copyright".into()));
    Value::Dict(
        Dict::new()
            .insert("Type", Value::Name("Font".into()))
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("Helvetica".into()))
            .insert(
                "Encoding",
                Value::Dict(Dict::new().insert("Differences", Value::Array(differences))),
            ),
    )
}

pub(super) fn document_from_objects(objects: Vec<Object>) -> Document {
    let mut trailer = Trailer::new();
    trailer.dict = Dict::new().insert("Root", Value::Ref(1, 0));
    Document::from_objects(objects, Xref::new(), trailer, StartXref(0))
}

pub(super) fn catalog_with_pages(pages: Value) -> Value {
    Value::Dict(
        Dict::new()
            .insert("Type", Value::Name("Catalog".into()))
            .insert("Pages", pages),
    )
}

pub(super) fn pages_node(kids: Array) -> Value {
    let count = kids.len() as i64;
    Value::Dict(
        Dict::new()
            .insert("Type", Value::Name("Pages".into()))
            .insert("Kids", Value::Array(kids))
            .insert("Count", Value::Integer(count)),
    )
}

pub(super) fn page_with(contents: Value, resources: Value) -> Value {
    Value::Dict(
        Dict::new()
            .insert("Type", Value::Name("Page".into()))
            .insert("Parent", Value::Ref(2, 0))
            .insert("Contents", contents)
            .insert("Resources", resources),
    )
}

pub(super) fn contents_stream(content: &[u8]) -> Value {
    Value::Stream(Dict::new(), StreamData::Owned(content.to_vec()))
}

/// Catalog object 1 → `/Pages` object 2 → single page object 3, plus any
/// additional objects supplied by the test.
pub(super) fn single_page_document(
    contents: Value,
    resources: Value,
    extra: Vec<Object>,
) -> Document {
    let mut objects = vec![
        Object::new(1, 0, catalog_with_pages(Value::Ref(2, 0))),
        Object::new(2, 0, pages_node(Array::new().push(Value::Ref(3, 0)))),
        Object::new(3, 0, page_with(contents, resources)),
    ];
    objects.extend(extra);
    document_from_objects(objects)
}

pub(super) fn resources_with_font_and_xobjects(font: Value, xobjects: &[(&str, i64)]) -> Dict {
    let mut map = Dict::new();
    for (name, num) in xobjects {
        map = map.insert(*name, Value::Ref(*num, 0));
    }
    resources_with_font("F1", font).insert("XObject", Value::Dict(map))
}

pub(super) fn form_xobject(content: &[u8], resources: Option<Dict>) -> Value {
    let mut dict = Dict::new()
        .insert("Type", Value::Name("XObject".into()))
        .insert("Subtype", Value::Name("Form".into()))
        .insert("FormType", Value::Integer(1))
        .insert(
            "BBox",
            Value::Array(
                Array::new()
                    .push(Value::Integer(0))
                    .push(Value::Integer(0))
                    .push(Value::Integer(100))
                    .push(Value::Integer(100)),
            ),
        );
    if let Some(resources) = resources {
        dict = dict.insert("Resources", Value::Dict(resources));
    }
    Value::Stream(dict, StreamData::Owned(content.to_vec()))
}

pub(super) fn image_xobject() -> Value {
    Value::Stream(
        Dict::new()
            .insert("Type", Value::Name("XObject".into()))
            .insert("Subtype", Value::Name("Image".into()))
            .insert("Width", Value::Integer(1))
            .insert("Height", Value::Integer(1))
            .insert("ColorSpace", Value::Name("DeviceGray".into()))
            .insert("BitsPerComponent", Value::Integer(8)),
        StreamData::Owned(vec![0]),
    )
}

pub(super) fn oc_properties_document(on: &[i64], off: &[i64]) -> Document {
    oc_properties_document_with(on, off, Vec::new())
}

pub(super) fn oc_properties_document_with(on: &[i64], off: &[i64], extra: Vec<Object>) -> Document {
    let mut on_array = Array::new();
    for object in on {
        on_array = on_array.push(Value::Ref(*object, 0));
    }
    let mut off_array = Array::new();
    for object in off {
        off_array = off_array.push(Value::Ref(*object, 0));
    }
    let groups: Vec<i64> = on.iter().chain(off).copied().collect();
    let mut ocgs = Array::new();
    let mut objects = Vec::new();
    for object in &groups {
        ocgs = ocgs.push(Value::Ref(*object, 0));
        objects.push(Object::new(
            *object,
            0,
            Value::Dict(
                Dict::new()
                    .insert("Type", Value::Name("OCG".into()))
                    .insert("Name", Value::String(format!("Group {object}"))),
            ),
        ));
    }
    let catalog = Value::Dict(
        Dict::new()
            .insert("Type", Value::Name("Catalog".into()))
            .insert(
                "OCProperties",
                Value::Dict(
                    Dict::new().insert("OCGs", Value::Array(ocgs)).insert(
                        "D",
                        Value::Dict(
                            Dict::new()
                                .insert("BaseState", Value::Name("ON".into()))
                                .insert("ON", Value::Array(on_array))
                                .insert("OFF", Value::Array(off_array)),
                        ),
                    ),
                ),
            ),
    );
    objects.extend(extra);
    objects.insert(0, Object::new(1, 0, catalog));
    document_from_objects(objects)
}

pub(super) fn marked_resources(properties: &[(&str, Value)]) -> Dict {
    let mut map = Dict::new();
    for (name, value) in properties {
        map = map.insert(*name, value.clone());
    }
    resources_with_font("F1", winansi_type1_font()).insert("Properties", Value::Dict(map))
}
