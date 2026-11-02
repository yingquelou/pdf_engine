//! `Do` + form XObject execution: resources, nesting, cycles and state
//! restore semantics.

use crate::core::types::{Dict, Object, Value};

use super::support::{
    contents_stream, differences_type1_font, form_xobject, image_xobject, resources_with_font,
    resources_with_font_and_xobjects, single_page_document, winansi_type1_font,
};

#[test]
fn form_xobject_with_own_resources_is_extracted() {
    let resources = resources_with_font_and_xobjects(winansi_type1_font(), &[("Fm", 10)]);
    let form = form_xobject(
        b"BT /F2 12 Tf (Form) Tj ET",
        Some(resources_with_font("F2", winansi_type1_font())),
    );
    let document = single_page_document(
        contents_stream(b"BT /F1 12 Tf (Page) Tj ET /Fm Do"),
        Value::Dict(resources),
        vec![Object::new(10, 0, form)],
    );

    assert_eq!(document.extract_page_text(0).as_deref(), Some("PageForm"));
}

#[test]
fn form_without_resources_inherits_caller_resources() {
    let resources = resources_with_font_and_xobjects(winansi_type1_font(), &[("Fm", 10)]);
    let form = form_xobject(b"BT /F1 12 Tf (Inherited) Tj ET", None);
    let document = single_page_document(
        contents_stream(b"/Fm Do"),
        Value::Dict(resources),
        vec![Object::new(10, 0, form)],
    );

    assert_eq!(document.extract_page_text(0).as_deref(), Some("Inherited"));
}

#[test]
fn nested_form_xobject_text_is_extracted_in_order() {
    let resources = resources_with_font_and_xobjects(winansi_type1_font(), &[("A", 10), ("B", 11)]);
    let form_a = form_xobject(b"BT /F1 12 Tf (A) Tj ET /B Do BT /F1 12 Tf (C) Tj ET", None);
    let form_b = form_xobject(b"BT /F1 12 Tf (B) Tj ET", None);
    let document = single_page_document(
        contents_stream(b"/A Do"),
        Value::Dict(resources),
        vec![Object::new(10, 0, form_a), Object::new(11, 0, form_b)],
    );

    assert_eq!(document.extract_page_text(0).as_deref(), Some("ABC"));
}

#[test]
fn cyclic_form_xobject_references_terminate() {
    let resources = resources_with_font_and_xobjects(winansi_type1_font(), &[("A", 10), ("B", 11)]);
    let form_a = form_xobject(b"BT /F1 12 Tf (A) Tj ET /B Do", None);
    let form_b = form_xobject(b"BT /F1 12 Tf (B) Tj ET /A Do", None);
    let document = single_page_document(
        contents_stream(b"/A Do"),
        Value::Dict(resources),
        vec![Object::new(10, 0, form_a), Object::new(11, 0, form_b)],
    );

    assert_eq!(document.extract_page_text(0).as_deref(), Some("AB"));
}

#[test]
fn repeated_form_xobject_is_extracted_each_time() {
    let resources = resources_with_font_and_xobjects(winansi_type1_font(), &[("Fm", 10)]);
    let form = form_xobject(b"BT /F1 12 Tf (X) Tj ET", None);
    let document = single_page_document(
        contents_stream(b"/Fm Do /Fm Do"),
        Value::Dict(resources),
        vec![Object::new(10, 0, form)],
    );

    assert_eq!(document.extract_page_text(0).as_deref(), Some("XX"));
}

#[test]
fn form_font_selection_does_not_leak_into_caller() {
    let page_resources = resources_with_font_and_xobjects(winansi_type1_font(), &[("Fm", 10)]);
    let form = form_xobject(
        b"BT /F1 12 Tf (A) Tj ET",
        Some(resources_with_font("F1", differences_type1_font())),
    );
    let document = single_page_document(
        contents_stream(b"BT /F1 12 Tf (A) Tj ET /Fm Do BT (A) Tj ET"),
        Value::Dict(page_resources),
        vec![Object::new(10, 0, form)],
    );

    assert_eq!(document.extract_page_text(0).as_deref(), Some("A©A"));
}

#[test]
fn q_and_q_restore_selected_font() {
    let resources = Dict::new().insert(
        "Font",
        Value::Dict(
            Dict::new()
                .insert("F1", winansi_type1_font())
                .insert("F2", differences_type1_font()),
        ),
    );
    let document = single_page_document(
        contents_stream(b"BT /F1 12 Tf (A) Tj ET q BT /F2 12 Tf (A) Tj ET Q BT (A) Tj ET"),
        Value::Dict(resources),
        Vec::new(),
    );

    assert_eq!(document.extract_page_text(0).as_deref(), Some("A©A"));
}

#[test]
fn image_xobject_is_skipped_without_losing_text() {
    let resources = resources_with_font_and_xobjects(winansi_type1_font(), &[("Im1", 10)]);
    let document = single_page_document(
        contents_stream(b"BT /F1 12 Tf (A) Tj ET /Im1 Do /Missing Do BT /F1 12 Tf (B) Tj ET"),
        Value::Dict(resources),
        vec![Object::new(10, 0, image_xobject())],
    );

    assert_eq!(document.extract_page_text(0).as_deref(), Some("AB"));
}
