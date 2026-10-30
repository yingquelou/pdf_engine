//! Rendition 动作读端测试（12.6.4.13 Table 214）。

use crate::core::reader::rendition::{MediaClip, MediaClipDataSource, Rendition};
use crate::core::types::{Dict, Value};

use super::*;

const SCREEN_ANNOTATION: i64 = 80;
const OTHER_ANNOTATION: i64 = 81;

fn insert_annotation(doc: &Document, num: i64, subtype: &str) {
    insert(
        doc,
        num,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Annot"))
                .insert("Subtype", name(subtype)),
        ),
    );
}

fn insert_details(doc: &Document) {
    insert(
        doc,
        90,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Rendition"))
                .insert("S", name("MR"))
                .insert("C", Value::Ref(91, 0)),
        ),
    );
    insert(
        doc,
        91,
        Value::Dict(
            Dict::new()
                .insert("Type", name("MediaClip"))
                .insert("S", name("MCD"))
                .insert(
                    "D",
                    Value::Dict(
                        Dict::new()
                            .insert("Type", name("Filespec"))
                            .insert("F", Value::String("clip.mov".into())),
                    ),
                ),
        ),
    );
}

#[test]
fn parses_rendition_action_play() {
    let doc = document_with_pages(1);
    insert_annotation(&doc, SCREEN_ANNOTATION, "Screen");
    insert_details(&doc);
    let action = Value::Dict(
        action_dict("Rendition")
            .insert("OP", Value::Integer(0))
            .insert("AN", Value::Ref(SCREEN_ANNOTATION, 0))
            .insert("R", Value::Ref(90, 0)),
    );

    match doc.action(&action).unwrap() {
        Action::Rendition(action) => {
            assert_eq!(action.operation, Some(RenditionOperation::Play));
            assert_eq!(action.annotation, Some((SCREEN_ANNOTATION, 0)));
            assert!(action.javascript.is_none());
            let rendition = action.rendition.expect("/R");
            let Rendition::Media(media) = rendition else {
                panic!("expected media rendition");
            };
            let MediaClip::Data(data) = media.clip.expect("/C") else {
                panic!("expected media clip data");
            };
            let MediaClipDataSource::FileSpec(filespec) = data.data else {
                panic!("expected filespec source");
            };
            assert_eq!(filespec.short_name.as_deref(), Some("clip.mov"));
        }
        other => panic!("unexpected action: {other:?}"),
    }
}

#[test]
fn parses_rendition_action_javascript_with_unknown_operation() {
    let doc = document_with_pages(1);
    let action = Value::Dict(
        action_dict("Rendition")
            .insert("OP", Value::Integer(9))
            .insert("JS", Value::String("app.alert('hi')".into())),
    );

    match doc.action(&action).unwrap() {
        Action::Rendition(action) => {
            assert_eq!(action.operation, Some(RenditionOperation::Other(9)));
            assert_eq!(action.javascript.as_deref(), Some("app.alert('hi')"));
            assert!(action.annotation.is_none());
            assert!(action.rendition.is_none());
        }
        other => panic!("unexpected action: {other:?}"),
    }
}

#[test]
fn parses_rendition_action_stop_without_rendition() {
    let doc = document_with_pages(1);
    insert_annotation(&doc, SCREEN_ANNOTATION, "Screen");
    let action = Value::Dict(
        action_dict("Rendition")
            .insert("OP", Value::Integer(1))
            .insert("AN", Value::Ref(SCREEN_ANNOTATION, 0)),
    );

    match doc.action(&action).unwrap() {
        Action::Rendition(action) => {
            assert_eq!(action.operation, Some(RenditionOperation::Stop));
            assert!(action.rendition.is_none());
        }
        other => panic!("unexpected action: {other:?}"),
    }
}

#[test]
fn rendition_action_requires_operation_or_javascript() {
    let doc = document_with_pages(1);
    let error = doc
        .action(&Value::Dict(action_dict("Rendition")))
        .expect_err("no /OP or /JS");
    assert!(error.to_string().contains("at least one of /OP or /JS"));
}

#[test]
fn rendition_action_enforces_operation_dependencies() {
    let doc = document_with_pages(1);
    insert_annotation(&doc, SCREEN_ANNOTATION, "Screen");
    insert_annotation(&doc, OTHER_ANNOTATION, "Text");
    insert_details(&doc);
    let cases = [
        (
            action_dict("Rendition")
                .insert("OP", Value::Integer(0))
                .insert("AN", Value::Ref(SCREEN_ANNOTATION, 0)),
            "/R is required",
        ),
        (
            action_dict("Rendition").insert("OP", Value::Integer(2)),
            "/AN is required",
        ),
        (
            action_dict("Rendition").insert("OP", Value::Integer(9)),
            "no /JS fallback",
        ),
        (
            action_dict("Rendition")
                .insert("OP", Value::Integer(1))
                .insert("AN", Value::Ref(OTHER_ANNOTATION, 0)),
            "targets a Text annotation",
        ),
        (
            action_dict("Rendition")
                .insert("OP", Value::Integer(1))
                .insert(
                    "AN",
                    Value::Dict(Dict::new().insert("Subtype", name("Screen"))),
                ),
            "must be an indirect reference",
        ),
        (
            action_dict("Rendition")
                .insert("OP", name("Play"))
                .insert("AN", Value::Ref(SCREEN_ANNOTATION, 0)),
            "/OP is not an integer",
        ),
        (
            action_dict("Rendition").insert("JS", Value::Integer(1)),
            "not a text string or UTF-8 stream",
        ),
    ];
    for (dict, message) in cases {
        let error = doc
            .action(&Value::Dict(dict))
            .expect_err("invalid rendition action");
        assert!(
            error.to_string().contains(message),
            "expected {message:?} in {error}"
        );
    }
}

#[test]
fn rendition_action_surfaces_rendition_errors() {
    let doc = document_with_pages(1);
    insert_annotation(&doc, SCREEN_ANNOTATION, "Screen");
    let action = Value::Dict(
        action_dict("Rendition")
            .insert("OP", Value::Integer(0))
            .insert("AN", Value::Ref(SCREEN_ANNOTATION, 0))
            .insert("R", Value::Dict(Dict::new().insert("S", name("SR")))),
    );
    let error = doc.action(&action).expect_err("malformed /R");
    assert!(error.to_string().contains("Rendition action /R"), "{error}");
}

#[test]
fn rendition_operation_values_roundtrip() {
    for (value, operation) in [
        (0, RenditionOperation::Play),
        (1, RenditionOperation::Stop),
        (2, RenditionOperation::Pause),
        (3, RenditionOperation::Resume),
        (4, RenditionOperation::PlayIfPaused),
        (9, RenditionOperation::Other(9)),
    ] {
        assert_eq!(operation.value(), value);
        assert_eq!(operation.is_defined(), value < 5);
    }
}
