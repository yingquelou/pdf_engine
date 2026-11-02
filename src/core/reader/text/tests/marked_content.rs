//! Marked-content sections: `/ActualText`, `BDC` property names and
//! optional-content (OCG/OCMD) filtering.

use crate::core::types::{Array, Dict, Document, Object, Value};

use super::support::{
    catalog_with_pages, document_from_objects, form_xobject, marked_resources,
    oc_properties_document, oc_properties_document_with, resources_with_font, winansi_type1_font,
};

fn ocmd_object(object: i64, groups: &[i64], policy: &str) -> Object {
    let mut refs = Array::new();
    for group in groups {
        refs = refs.push(Value::Ref(*group, 0));
    }
    Object::new(
        object,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("OCMD".into()))
                .insert("OCGs", Value::Array(refs))
                .insert("P", Value::Name(policy.into())),
        ),
    )
}

#[test]
fn actual_text_replaces_all_shows_in_section() {
    let document = Document::new();
    let resources = resources_with_font("F1", winansi_type1_font());
    let content = b"BT /F1 12 Tf /Span <</ActualText (R)>> BDC (orig) Tj (more) Tj ET EMC \
                    BT /F1 12 Tf (tail) Tj ET";

    assert_eq!(document.extract_text(content, &resources), "Rtail");
}

#[test]
fn actual_text_applies_across_nested_sections() {
    let document = Document::new();
    let resources = resources_with_font("F1", winansi_type1_font());
    let content = b"BT /F1 12 Tf /Span <</ActualText (outer)>> BDC (a) Tj \
                    /Nested BMC (b) Tj /Deeper BMC (c) Tj EMC EMC EMC \
                    (tail) Tj ET";

    assert_eq!(document.extract_text(content, &resources), "outertail");
}

#[test]
fn nested_actual_text_winner_emits_once_before_outer() {
    let document = Document::new();
    let resources = resources_with_font("F1", winansi_type1_font());
    let content = b"BT /F1 12 Tf /Outer <</ActualText (outer)>> BDC \
                    /Inner <</ActualText (inner)>> BDC (b) Tj (c) Tj EMC \
                    (d) Tj EMC (tail) Tj ET";

    assert_eq!(document.extract_text(content, &resources), "innertail");
}

#[test]
fn actual_text_decodes_hex_string_property() {
    let document = Document::new();
    let resources = resources_with_font("F1", winansi_type1_font());
    // <FEFF0066> is UTF-16BE "f".
    let content = b"BT /F1 12 Tf /S <</ActualText <FEFF0066>>> BDC (gone) Tj ET EMC \
                    BT /F1 12 Tf (oo) Tj ET";

    assert_eq!(document.extract_text(content, &resources), "foo");
}

#[test]
fn bdc_property_name_resolves_through_properties_resource() {
    let document = document_from_objects(vec![
        Object::new(1, 0, catalog_with_pages(Value::Ref(2, 0))),
        Object::new(
            6,
            0,
            Value::Dict(Dict::new().insert("ActualText", Value::String("via-name".into()))),
        ),
    ]);
    let resources = marked_resources(&[("P0", Value::Ref(6, 0))]);
    let content = b"BT /F1 12 Tf /Span /P0 BDC (orig) Tj ET EMC";

    assert_eq!(document.extract_text(content, &resources), "via-name");
}

#[test]
fn hidden_ocg_section_is_not_extracted() {
    let document = oc_properties_document(&[], &[5]);
    let resources = marked_resources(&[("MC0", Value::Ref(5, 0))]);
    let content = b"/P <</OC /MC0>> BDC BT /F1 12 Tf (gone) Tj ET EMC \
                    BT /F1 12 Tf (kept) Tj ET";

    assert_eq!(document.extract_text(content, &resources), "kept");
}

#[test]
fn visible_ocg_section_is_extracted() {
    let document = oc_properties_document(&[5], &[]);
    let resources = marked_resources(&[("MC0", Value::Ref(5, 0))]);
    let content = b"/P <</OC /MC0>> BDC BT /F1 12 Tf (shown) Tj ET EMC";

    assert_eq!(document.extract_text(content, &resources), "shown");
}

#[test]
fn hidden_ocg_property_resolved_by_direct_reference() {
    let document = oc_properties_document(&[], &[5]);
    let resources = resources_with_font("F1", winansi_type1_font());
    let content = b"/P <</OC 5 0 R>> BDC BT /F1 12 Tf (gone) Tj ET EMC \
                    BT /F1 12 Tf (kept) Tj ET";

    assert_eq!(document.extract_text(content, &resources), "kept");
}

#[test]
fn bdc_name_pointing_at_ocg_dictionary_filters_hidden_group() {
    let document = oc_properties_document(&[], &[5]);
    let resources = marked_resources(&[("MC0", Value::Ref(5, 0))]);
    let content = b"/OC /MC0 BDC BT /F1 12 Tf (gone) Tj ET EMC \
                    BT /F1 12 Tf (kept) Tj ET";

    assert_eq!(document.extract_text(content, &resources), "kept");
}

#[test]
fn nested_section_inherits_hidden_state_from_parent() {
    let document = oc_properties_document(&[], &[5]);
    let resources = marked_resources(&[("MC0", Value::Ref(5, 0))]);
    let content = b"/Outer <</OC /MC0>> BDC /Inner BMC \
                      BT /F1 12 Tf (gone) Tj ET EMC EMC";

    assert_eq!(document.extract_text(content, &resources), "");
}

#[test]
fn indirect_ocmd_hides_section_by_policy() {
    let document = oc_properties_document_with(&[5], &[], vec![ocmd_object(7, &[5], "AllOff")]);
    let resources = resources_with_font("F1", winansi_type1_font());
    let content = b"/P <</OC 7 0 R>> BDC BT /F1 12 Tf (gone) Tj ET EMC \
                    BT /F1 12 Tf (kept) Tj ET";

    assert_eq!(document.extract_text(content, &resources), "kept");
}

#[test]
fn ocmd_resolved_through_properties_name() {
    let document = oc_properties_document_with(&[], &[5], vec![ocmd_object(7, &[5], "AllOn")]);
    let resources = marked_resources(&[("MC0", Value::Ref(7, 0))]);
    let content = b"/P /MC0 BDC BT /F1 12 Tf (gone) Tj ET EMC \
                    BT /F1 12 Tf (kept) Tj ET";

    assert_eq!(document.extract_text(content, &resources), "kept");
}

#[test]
fn inline_ocmd_visibility_expression_overrides_policy() {
    let document = oc_properties_document(&[], &[5]);
    let resources = resources_with_font("F1", winansi_type1_font());
    // /P /AnyOn alone would hide the section (group 5 is off), but
    // /VE [/Not 5 0 R] says the membership is on.
    let content = b"/P <</OC <</Type /OCMD /OCGs [5 0 R] /P /AnyOn /VE [/Not 5 0 R]>>>> BDC \
                    BT /F1 12 Tf (shown) Tj ET EMC";

    assert_eq!(document.extract_text(content, &resources), "shown");
}

#[test]
fn unbalanced_emc_does_not_break_extraction() {
    let document = Document::new();
    let resources = resources_with_font("F1", winansi_type1_font());
    let content = b"BT /F1 12 Tf (a) Tj ET EMC BT /F1 12 Tf (b) Tj ET";

    assert_eq!(document.extract_text(content, &resources), "ab");
}

#[test]
fn unclosed_marked_section_in_form_does_not_leak_to_caller() {
    let catalog = Value::Dict(
        Dict::new()
            .insert("Type", Value::Name("Catalog".into()))
            .insert(
                "OCProperties",
                Value::Dict(
                    Dict::new()
                        .insert("OCGs", Value::Array(Array::new().push(Value::Ref(5, 0))))
                        .insert(
                            "D",
                            Value::Dict(
                                Dict::new()
                                    .insert("BaseState", Value::Name("ON".into()))
                                    .insert(
                                        "OFF",
                                        Value::Array(Array::new().push(Value::Ref(5, 0))),
                                    ),
                            ),
                        ),
                ),
            ),
    );
    let document = document_from_objects(vec![
        Object::new(1, 0, catalog),
        Object::new(
            5,
            0,
            Value::Dict(
                Dict::new()
                    .insert("Type", Value::Name("OCG".into()))
                    .insert("Name", Value::String("Group 5".into())),
            ),
        ),
        Object::new(
            10,
            0,
            form_xobject(b"/H <</OC /MC0>> BDC BT /F1 12 Tf (gone) Tj ET", None),
        ),
    ]);
    let resources = marked_resources(&[("MC0", Value::Ref(5, 0)), ("Frm", Value::Ref(10, 0))]);
    let content = b"BT /F1 12 Tf (before) Tj ET /Frm Do \
                    BT /F1 12 Tf (after) Tj ET";

    assert_eq!(document.extract_text(content, &resources), "beforeafter");
}
