//! `/Artifact` skipping and positioned [`TextFragment`] output.

use crate::core::types::{Array, Document, Object, Value};

use super::super::{TextExtractionError, TextFragment};
use super::support::{
    contents_stream, form_xobject, marked_resources, oc_properties_document, resources_with_font,
    resources_with_font_and_xobjects, single_page_document, winansi_type1_font,
};

#[test]
fn artifact_marked_content_is_skipped() {
    let document = Document::new();
    let resources = resources_with_font("F1", winansi_type1_font());
    let content = b"BT /F1 12 Tf (before) Tj ET \
                    /Artifact BMC BT /F1 12 Tf (foot) Tj ET EMC \
                    BT /F1 12 Tf (after) Tj ET";

    assert_eq!(document.extract_text(content, &resources), "beforeafter");
}

#[test]
fn nested_sections_inherit_artifact_state() {
    let document = Document::new();
    let resources = resources_with_font("F1", winansi_type1_font());
    let content = b"/Artifact BMC /Span BMC BT /F1 12 Tf (hidden) Tj ET EMC EMC \
                    BT /F1 12 Tf (shown) Tj ET";

    assert_eq!(document.extract_text(content, &resources), "shown");
}

#[test]
fn artifact_with_inline_properties_is_skipped() {
    let document = Document::new();
    let resources = resources_with_font("F1", winansi_type1_font());
    let content = b"/Artifact <</Type /Pagination>> BDC \
                    BT /F1 12 Tf (foot) Tj ET EMC \
                    BT /F1 12 Tf (body) Tj ET";

    assert_eq!(document.extract_text(content, &resources), "body");
}

#[test]
fn artifact_breaks_do_not_insert_separators() {
    let document = Document::new();
    let resources = resources_with_font("F1", winansi_type1_font());
    let content = b"BT /F1 12 Tf (A) Tj ET /Artifact BMC Td EMC \
                    BT /F1 12 Tf (B) Tj ET";

    assert_eq!(document.extract_text(content, &resources), "AB");
}

#[test]
fn artifact_text_is_absent_from_fragments() {
    let document = Document::new();
    let resources = resources_with_font("F1", winansi_type1_font());
    let content = b"/Artifact BMC BT /F1 12 Tf 10 10 Td (stamp) Tj ET EMC \
                    BT /F1 12 Tf 20 20 Td (body) Tj ET";

    assert!(!document.extract_text(content, &resources).contains("stamp"));
    assert_eq!(
        document.extract_text_fragments(content, &resources),
        vec![TextFragment {
            text: "body".into(),
            x: 20.0,
            y: 20.0,
        }]
    );
}

#[test]
fn fragments_report_show_positions_and_merge_same_position_runs() {
    let document = Document::new();
    let resources = resources_with_font("F1", winansi_type1_font());
    let content = b"BT /F1 12 Tf 100 700 Td [(H) (i)] TJ ET \
                    BT /F1 12 Tf 80 680 Td (There) Tj ET";

    assert_eq!(
        document.extract_text_fragments(content, &resources),
        vec![
            TextFragment {
                text: "Hi".into(),
                x: 100.0,
                y: 700.0,
            },
            TextFragment {
                text: "There".into(),
                x: 80.0,
                y: 680.0,
            },
        ]
    );
}

#[test]
fn fragments_apply_cm_and_q_q_at_the_ctm_level() {
    let document = Document::new();
    let resources = resources_with_font("F1", winansi_type1_font());
    let content = b"1 0 0 1 10 20 cm \
                    BT /F1 12 Tf 100 50 Td (A) Tj ET \
                    q 2 0 0 2 5 5 cm BT /F1 12 Tf 100 50 Td (B) Tj ET Q \
                    BT /F1 12 Tf 100 50 Td (C) Tj ET";

    assert_eq!(
        document.extract_text_fragments(content, &resources),
        vec![
            TextFragment {
                text: "A".into(),
                x: 110.0,
                y: 70.0,
            },
            TextFragment {
                text: "B".into(),
                x: 215.0,
                y: 125.0,
            },
            TextFragment {
                text: "C".into(),
                x: 110.0,
                y: 70.0,
            },
        ]
    );
}

#[test]
fn fragments_track_leading_and_text_rise() {
    let document = Document::new();
    let resources = resources_with_font("F1", winansi_type1_font());
    let content = b"BT /F1 12 Tf 14 TL 50 700 Td (One) Tj T* 5 Ts (Two) Tj ET";

    assert_eq!(
        document.extract_text_fragments(content, &resources),
        vec![
            TextFragment {
                text: "One".into(),
                x: 50.0,
                y: 700.0,
            },
            TextFragment {
                text: "Two".into(),
                x: 50.0,
                y: 691.0,
            },
        ]
    );
}

#[test]
fn actual_text_replacement_becomes_a_fragment() {
    let document = Document::new();
    let resources = resources_with_font("F1", winansi_type1_font());
    let content = b"/Span <</ActualText (replaced)>> BDC \
                    BT /F1 12 Tf 7 9 Td (abc) Tj ET EMC";

    assert_eq!(document.extract_text(content, &resources), "replaced");
    assert_eq!(
        document.extract_text_fragments(content, &resources),
        vec![TextFragment {
            text: "replaced".into(),
            x: 7.0,
            y: 9.0,
        }]
    );
}

fn form_with_matrix(form: Value, matrix: [f64; 6]) -> Value {
    let Value::Stream(dict, data) = form else {
        panic!("form should be a stream");
    };
    let mut array = Array::new();
    for value in matrix {
        array = array.push(Value::Float(value));
    }
    Value::Stream(dict.insert("Matrix", Value::Array(array)), data)
}

#[test]
fn fragments_apply_form_matrix_and_restore_ctm() {
    let resources = Value::Dict(resources_with_font_and_xobjects(
        winansi_type1_font(),
        &[("Frm", 10)],
    ));
    let form = form_with_matrix(
        form_xobject(b"BT /F1 12 Tf 0 0 Td (F) Tj ET", None),
        [1.0, 0.0, 0.0, 1.0, 5.0, 20.0],
    );
    let document = single_page_document(
        contents_stream(b"1 0 0 1 10 0 cm /Frm Do BT /F1 12 Tf 0 0 Td (P) Tj ET"),
        resources,
        vec![Object::new(10, 0, form)],
    );

    assert_eq!(
        document.extract_page_text_fragments(0).unwrap(),
        vec![
            TextFragment {
                text: "F".into(),
                x: 15.0,
                y: 20.0,
            },
            TextFragment {
                text: "P".into(),
                x: 10.0,
                y: 0.0,
            },
        ]
    );
}

#[test]
fn extract_page_text_fragments_reports_page_errors() {
    let document = single_page_document(
        contents_stream(b"BT /F1 12 Tf 30 40 Td (Hi) Tj ET"),
        Value::Dict(resources_with_font("F1", winansi_type1_font())),
        Vec::new(),
    );

    assert_eq!(
        document.extract_page_text_fragments(0),
        Some(vec![TextFragment {
            text: "Hi".into(),
            x: 30.0,
            y: 40.0,
        }])
    );
    assert_eq!(document.extract_page_text_fragments(1), None);
    assert_eq!(
        document.try_extract_page_text_fragments(1),
        Err(TextExtractionError::PageNotFound)
    );
    assert_eq!(
        document.try_extract_page_text(1),
        Err(TextExtractionError::PageNotFound)
    );
}

#[test]
fn hidden_ocg_text_is_absent_from_fragments() {
    let document = oc_properties_document(&[], &[5]);
    let resources = marked_resources(&[("MC0", Value::Ref(5, 0))]);
    let content = b"/P <</OC /MC0>> BDC BT /F1 12 Tf 10 10 Td (gone) Tj ET EMC \
                    BT /F1 12 Tf 20 20 Td (kept) Tj ET";

    assert_eq!(document.extract_text(content, &resources), "kept");
    assert_eq!(
        document.extract_text_fragments(content, &resources),
        vec![TextFragment {
            text: "kept".into(),
            x: 20.0,
            y: 20.0,
        }]
    );
}
