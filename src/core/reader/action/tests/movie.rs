//! Movie 动作读端测试（12.6.4.9 Table 209）。

use crate::core::reader::movie::MovieMode;
use crate::core::types::{Dict, Value};

use super::*;

const MOVIE_ANNOTATION: i64 = 80;
const OTHER_ANNOTATION: i64 = 81;

fn insert_annotation(doc: &Document, num: i64, subtype: &str) {
    insert(
        doc,
        num,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Annot"))
                .insert("Subtype", name(subtype))
                .insert(
                    "Rect",
                    arr(vec![
                        Value::Integer(0),
                        Value::Integer(0),
                        Value::Integer(1),
                        Value::Integer(1),
                    ]),
                )
                .insert(
                    "Movie",
                    Value::Dict(
                        Dict::new().insert(
                            "F",
                            Value::Dict(
                                Dict::new()
                                    .insert("Type", name("Filespec"))
                                    .insert("F", Value::String("clip.mov".into())),
                            ),
                        ),
                    ),
                ),
        ),
    );
}

#[test]
fn parses_movie_action_annotation_target() {
    let doc = document_with_pages(1);
    insert_annotation(&doc, MOVIE_ANNOTATION, "Movie");
    let action = Value::Dict(
        action_dict("Movie")
            .insert("Annotation", Value::Ref(MOVIE_ANNOTATION, 0))
            .insert("Volume", Value::Float(0.5))
            .insert("Mode", name("Repeat")),
    );

    match doc.action(&action).unwrap() {
        Action::Movie(movie) => {
            assert_eq!(movie.annotation, Some((MOVIE_ANNOTATION, 0)));
            assert_eq!(movie.title, None);
            assert_eq!(movie.activation.volume, Some(0.5));
            assert_eq!(movie.activation.mode, MovieMode::Repeat);
            assert_eq!(movie.activation.rate, None);
        }
        other => panic!("unexpected action: {other:?}"),
    }
}

#[test]
fn parses_movie_action_title_target() {
    let doc = document_with_pages(1);
    let action = Value::Dict(
        action_dict("Movie")
            .insert("T", Value::String("Intro".into()))
            .insert("ShowControls", Value::Bool(true)),
    );

    match doc.action(&action).unwrap() {
        Action::Movie(movie) => {
            assert_eq!(movie.annotation, None);
            assert_eq!(movie.title.as_deref(), Some("Intro"));
            assert_eq!(movie.activation.show_controls, Some(true));
        }
        other => panic!("unexpected action: {other:?}"),
    }
}

#[test]
fn movie_action_requires_exactly_one_target() {
    let doc = document_with_pages(1);
    insert_annotation(&doc, MOVIE_ANNOTATION, "Movie");
    for dict in [
        action_dict("Movie"),
        action_dict("Movie")
            .insert("Annotation", Value::Ref(MOVIE_ANNOTATION, 0))
            .insert("T", Value::String("Intro".into())),
    ] {
        let error = doc.action(&Value::Dict(dict)).expect_err("bad target");
        assert!(
            error.to_string().contains("exactly one"),
            "unexpected error: {error}"
        );
    }
}

#[test]
fn movie_action_rejects_non_movie_annotation_target() {
    let doc = document_with_pages(1);
    insert_annotation(&doc, OTHER_ANNOTATION, "Link");
    let action =
        Value::Dict(action_dict("Movie").insert("Annotation", Value::Ref(OTHER_ANNOTATION, 0)));

    let error = doc.action(&action).expect_err("wrong target subtype");
    assert!(
        error.to_string().contains("Link"),
        "unexpected error: {error}"
    );
}

#[test]
fn movie_action_rejects_direct_annotation_dict() {
    let doc = document_with_pages(1);
    let action = Value::Dict(action_dict("Movie").insert(
        "Annotation",
        Value::Dict(Dict::new().insert("Subtype", name("Movie"))),
    ));

    let error = doc.action(&action).expect_err("direct /Annotation");
    assert!(
        error.to_string().contains("indirect reference"),
        "unexpected error: {error}"
    );
}

#[test]
fn movie_action_surfaces_activation_errors() {
    let doc = document_with_pages(1);
    let action = Value::Dict(
        action_dict("Movie")
            .insert("T", Value::String("Intro".into()))
            .insert("Volume", Value::Float(2.0)),
    );

    let error = doc.action(&action).expect_err("bad volume");
    assert!(
        error.to_string().contains("Volume"),
        "unexpected error: {error}"
    );
}
