//! Page-tree traversal, `/Parent` inheritance and the content budget.

use crate::core::types::{Array, Dict, Object, StreamData, Value};

use super::super::{
    MAX_CONTENT_BYTES, MAX_CONTENT_STREAMS, MAX_PAGE_TREE_DEPTH, TextExtractionError,
};
use super::support::{
    catalog_with_pages, contents_stream, document_from_objects, form_xobject, page_with,
    pages_node, resources_with_font, resources_with_font_and_xobjects, single_page_document,
    winansi_type1_font,
};

#[test]
fn page_tree_cycle_does_not_hang_and_finds_reachable_page() {
    let objects = vec![
        Object::new(1, 0, catalog_with_pages(Value::Ref(2, 0))),
        Object::new(
            2,
            0,
            pages_node(Array::new().push(Value::Ref(2, 0)).push(Value::Ref(3, 0))),
        ),
        Object::new(
            3,
            0,
            page_with(
                contents_stream(b"BT /F1 12 Tf (Reachable) Tj ET"),
                Value::Dict(resources_with_font("F1", winansi_type1_font())),
            ),
        ),
    ];
    let document = document_from_objects(objects);

    assert_eq!(document.extract_page_text(0).as_deref(), Some("Reachable"));
    assert!(document.extract_page_text(1).is_none());
    assert_eq!(
        document.try_extract_page_text(1),
        Err(TextExtractionError::PageNotFound)
    );
}

#[test]
fn page_tree_deeper_than_limit_returns_not_found() {
    let layers = MAX_PAGE_TREE_DEPTH + 2;
    let mut objects = vec![Object::new(1, 0, catalog_with_pages(Value::Ref(2, 0)))];
    for layer in 0..layers {
        let num = 2 + layer as i64;
        objects.push(Object::new(
            num,
            0,
            pages_node(Array::new().push(Value::Ref(num + 1, 0))),
        ));
    }
    let page_num = 2 + layers as i64;
    objects.push(Object::new(
        page_num,
        0,
        page_with(
            contents_stream(b"BT /F1 12 Tf (TooDeep) Tj ET"),
            Value::Dict(resources_with_font("F1", winansi_type1_font())),
        ),
    ));
    let document = document_from_objects(objects);

    assert!(document.extract_page_text(0).is_none());
    assert_eq!(
        document.try_extract_page_text(0),
        Err(TextExtractionError::PageNotFound)
    );
}

#[test]
fn parent_chain_cycle_terminates_and_own_resources_still_apply() {
    let font_resources = Value::Dict(resources_with_font("F1", winansi_type1_font()));
    let own_page = Value::Dict(
        Dict::new()
            .insert("Type", Value::Name("Page".into()))
            .insert("Parent", Value::Ref(5, 0))
            .insert("Contents", contents_stream(b"BT /F1 12 Tf (Own) Tj ET"))
            .insert("Resources", font_resources),
    );
    // Page 4 has no /Resources; its /Parent points at object 5 whose /Parent
    // points back at page 4, forming a cycle.
    let page_without_resources = Value::Dict(
        Dict::new()
            .insert("Type", Value::Name("Page".into()))
            .insert("Parent", Value::Ref(5, 0))
            .insert("Contents", contents_stream(b"BT /F1 12 Tf (Missing) Tj ET")),
    );
    let cyclic_parent = Value::Dict(
        Dict::new()
            .insert("Type", Value::Name("Pages".into()))
            .insert("Parent", Value::Ref(4, 0)),
    );
    let objects = vec![
        Object::new(1, 0, catalog_with_pages(Value::Ref(2, 0))),
        Object::new(
            2,
            0,
            pages_node(Array::new().push(Value::Ref(3, 0)).push(Value::Ref(4, 0))),
        ),
        Object::new(3, 0, own_page),
        Object::new(4, 0, page_without_resources),
        Object::new(5, 0, cyclic_parent),
    ];
    let document = document_from_objects(objects);

    assert_eq!(document.extract_page_text(0).as_deref(), Some("Own"));
    assert!(document.extract_page_text(1).is_none());
    assert_eq!(
        document.try_extract_page_text(1),
        Err(TextExtractionError::MalformedContent)
    );
}

#[test]
fn page_contents_stream_count_over_limit_returns_content_too_large() {
    let mut contents = Array::new();
    for _ in 0..=MAX_CONTENT_STREAMS {
        contents = contents.push(contents_stream(b""));
    }
    let document =
        single_page_document(Value::Array(contents), Value::Dict(Dict::new()), Vec::new());

    assert_eq!(
        document.try_extract_page_text(0),
        Err(TextExtractionError::ContentTooLarge)
    );
    assert!(document.extract_page_text(0).is_none());
}

#[test]
fn page_contents_byte_size_over_limit_returns_content_too_large() {
    let oversized = Value::Stream(
        Dict::new(),
        StreamData::Owned(vec![b' '; MAX_CONTENT_BYTES + 1]),
    );
    let document = single_page_document(oversized, Value::Dict(Dict::new()), Vec::new());

    assert_eq!(
        document.try_extract_page_text(0),
        Err(TextExtractionError::ContentTooLarge)
    );
    assert!(document.extract_page_text(0).is_none());
}

#[test]
fn form_stream_budget_is_shared_with_page_content_streams() {
    let mut contents = Array::new();
    for _ in 0..MAX_CONTENT_STREAMS - 1 {
        contents = contents.push(contents_stream(b""));
    }
    contents = contents.push(contents_stream(b"/Fm Do"));
    let resources = resources_with_font_and_xobjects(winansi_type1_font(), &[("Fm", 10)]);
    let form = form_xobject(b"BT /F1 12 Tf (Form) Tj ET", None);
    let document = single_page_document(
        Value::Array(contents),
        Value::Dict(resources),
        vec![Object::new(10, 0, form)],
    );

    assert_eq!(
        document.try_extract_page_text(0),
        Err(TextExtractionError::ContentTooLarge)
    );
    assert!(document.extract_page_text(0).is_none());
}
