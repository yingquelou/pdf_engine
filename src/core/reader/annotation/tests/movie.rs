//! Movie 注解专有条目读取（12.5.6.17 Table 186）：`/Movie` 与 `/A` 播放设置。

use crate::core::reader::annotation::*;
use crate::core::reader::movie::{MovieMode, MoviePlayback, Poster};
use crate::core::types::{Dict, Value};

use super::support::*;

fn movie_dictionary(name: &str) -> Value {
    Value::Dict(
        Dict::new().insert(
            "F",
            Value::Dict(
                Dict::new()
                    .insert("Type", Value::Name("Filespec".into()))
                    .insert("F", Value::String(name.into())),
            ),
        ),
    )
}

#[test]
fn movie_annotation_reads_movie_and_boolean_playback() {
    let document = doc(vec![catalog_object()]);
    let dict = base_annotation("Movie")
        .insert("Movie", movie_dictionary("clip.mov"))
        .insert("A", Value::Bool(false));

    let annotation = parse_annotation(&document, 0, None, &dict).expect("movie annotation");
    assert_eq!(annotation.subtype, AnnotationSubtype::Movie);
    let movie = annotation.movie.as_ref().expect("/Movie");
    assert_eq!(movie.file.short_name.as_deref(), Some("clip.mov"));
    assert_eq!(movie.aspect, None);
    assert_eq!(movie.rotate, 0);
    assert_eq!(movie.poster, Poster::Disabled);
    assert_eq!(annotation.movie_playback, Some(MoviePlayback::Flag(false)));
}

#[test]
fn movie_annotation_reads_activation_dictionary() {
    let document = doc(vec![catalog_object()]);
    let dict = base_annotation("Movie")
        .insert("Movie", movie_dictionary("clip.mov"))
        .insert("T", Value::String("Intro".into()))
        .insert(
            "A",
            Value::Dict(
                Dict::new()
                    .insert("Volume", Value::Float(0.5))
                    .insert("Mode", Value::Name("Repeat".into())),
            ),
        );

    let annotation = parse_annotation(&document, 0, None, &dict).expect("movie annotation");
    assert_eq!(annotation.title, Some(Value::String("Intro".into())));
    match annotation.movie_playback.as_ref().expect("/A") {
        MoviePlayback::Activation(activation) => {
            assert_eq!(activation.volume, Some(0.5));
            assert_eq!(activation.mode, MovieMode::Repeat);
        }
        other => panic!("unexpected playback: {other:?}"),
    }
}

#[test]
fn movie_annotation_default_playback_is_not_materialized() {
    let document = doc(vec![catalog_object()]);
    let dict = base_annotation("Movie").insert("Movie", movie_dictionary("clip.mov"));

    let annotation = parse_annotation(&document, 0, None, &dict).expect("movie annotation");
    assert_eq!(annotation.movie_playback, None, "缺省 true 由呈现端应用");
}

#[test]
fn movie_annotation_requires_movie_dictionary() {
    let document = doc(vec![catalog_object()]);
    let error = parse_annotation(&document, 0, None, &base_annotation("Movie"))
        .expect_err("missing /Movie");
    assert!(
        matches!(&error, AnnotationError::Malformed(message) if message.contains("/Movie")),
        "{error:?}"
    );
}

#[test]
fn movie_entry_is_specific_to_movie_annotations() {
    let document = doc(vec![catalog_object()]);
    let error = parse_annotation(
        &document,
        0,
        None,
        &base_annotation("Text").insert("Movie", movie_dictionary("clip.mov")),
    )
    .expect_err("/Movie on Text");
    assert!(
        matches!(&error, AnnotationError::Malformed(message) if message.contains("/Movie")),
        "{error:?}"
    );
}

#[test]
fn malformed_movie_annotations_are_typed_errors() {
    let document = doc(vec![catalog_object()]);
    for dict in [
        base_annotation("Movie").insert("Movie", Value::Integer(7)),
        base_annotation("Movie")
            .insert("Movie", movie_dictionary("clip.mov"))
            .insert("A", Value::Integer(1)),
        base_annotation("Movie")
            .insert("Movie", movie_dictionary(""))
            .insert(
                "A",
                Value::Dict(Dict::new().insert("Volume", Value::Float(2.0))),
            ),
    ] {
        let error = parse_annotation(&document, 0, None, &dict).expect_err("malformed movie");
        assert!(
            matches!(&error, AnnotationError::Malformed(_)),
            "{dict:?}: {error:?}"
        );
    }
}
