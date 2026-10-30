//! 电影对象与激活字典读端测试（13.4 Table 295/296）。

use crate::core::types::{Dict, StreamData, Value};

use super::*;

fn filespec(name: &str) -> Value {
    Value::Dict(
        Dict::new()
            .insert("Type", Value::Name("Filespec".into()))
            .insert("F", Value::String(name.into())),
    )
}

fn movie_dict() -> Dict {
    Dict::new().insert("F", filespec("clip.mov"))
}

#[test]
fn parses_movie_dictionary_entries() {
    let dict = movie_dict()
        .insert(
            "Aspect",
            Value::Array(crate::core::types::Array(vec![
                Value::Integer(640),
                Value::Integer(480),
            ])),
        )
        .insert("Rotate", Value::Integer(270))
        .insert("Poster", Value::Bool(true));

    let movie = Document::new().movie(&Value::Dict(dict)).unwrap();
    assert_eq!(movie.file.short_name.as_deref(), Some("clip.mov"));
    assert_eq!(movie.aspect, Some((640, 480)));
    assert_eq!(movie.rotate, 270);
    assert_eq!(movie.poster, Poster::FromMovieFile);
}

#[test]
fn movie_defaults_are_applied() {
    let movie = Document::new().movie(&Value::Dict(movie_dict())).unwrap();
    assert_eq!(movie.aspect, None);
    assert_eq!(movie.rotate, 0);
    assert_eq!(movie.poster, Poster::Disabled);
}

#[test]
fn poster_stream_is_preserved_as_original_value() {
    let stream = Value::Stream(Dict::new(), StreamData::Owned(vec![1, 2]));
    let dict = movie_dict().insert("Poster", stream.clone());

    let movie = Document::new().movie(&Value::Dict(dict)).unwrap();
    assert_eq!(movie.poster, Poster::Stream(stream));
}

#[test]
fn movie_requires_filespec() {
    let error = Document::new()
        .movie(&Value::Dict(Dict::new()))
        .expect_err("missing /F");
    assert!(error.to_string().contains("/F"), "{error}");

    let error = Document::new()
        .movie(&Value::Integer(1))
        .expect_err("non-dictionary movie");
    assert!(error.to_string().contains("not a dictionary"), "{error}");
}

#[test]
fn rejects_malformed_movie_entries() {
    for dict in [
        movie_dict().insert(
            "Aspect",
            Value::Array(crate::core::types::Array(vec![Value::Integer(0)])),
        ),
        movie_dict().insert("Rotate", Value::Integer(45)),
        movie_dict().insert("Poster", Value::Integer(1)),
    ] {
        let error = Document::new()
            .movie(&Value::Dict(dict))
            .expect_err("malformed movie");
        assert!(
            matches!(error, MovieError::Malformed(_)),
            "unexpected error: {error}"
        );
    }
}

#[test]
fn parses_movie_activation_entries() {
    let dict = Dict::new()
        .insert(
            "Start",
            Value::Array(crate::core::types::Array(vec![
                Value::Integer(100),
                Value::Integer(600),
            ])),
        )
        .insert("Duration", Value::Integer(250))
        .insert("Rate", Value::Float(-2.0))
        .insert("Volume", Value::Float(0.5))
        .insert("ShowControls", Value::Bool(true))
        .insert("Mode", Value::Name("Palindrome".into()))
        .insert("Synchronous", Value::Bool(true))
        .insert(
            "FWScale",
            Value::Array(crate::core::types::Array(vec![
                Value::Integer(2),
                Value::Integer(1),
            ])),
        )
        .insert(
            "FWPosition",
            Value::Array(crate::core::types::Array(vec![
                Value::Float(0.25),
                Value::Float(0.75),
            ])),
        );

    let activation = parse_movie_activation(&Document::new(), &dict).unwrap();
    assert_eq!(
        activation.start,
        Some(MovieTime {
            value: MovieTimeValue::Integer(100),
            time_scale: Some(600),
        })
    );
    assert_eq!(
        activation.duration,
        Some(MovieTime {
            value: MovieTimeValue::Integer(250),
            time_scale: None,
        })
    );
    assert_eq!(activation.rate, Some(-2.0));
    assert_eq!(activation.volume, Some(0.5));
    assert_eq!(activation.show_controls, Some(true));
    assert_eq!(activation.mode, MovieMode::Palindrome);
    assert_eq!(activation.synchronous, Some(true));
    assert_eq!(activation.fw_scale, Some((2, 1)));
    assert_eq!(activation.fw_position, Some((0.25, 0.75)));
}

#[test]
fn movie_activation_defaults_follow_table_296() {
    let activation = parse_movie_activation(&Document::new(), &Dict::new()).unwrap();
    assert_eq!(activation.start, None);
    assert_eq!(activation.duration, None);
    assert_eq!(activation.rate, None);
    assert_eq!(activation.volume, None);
    assert_eq!(activation.show_controls, None);
    assert_eq!(activation.mode, MovieMode::Once);
    assert_eq!(activation.synchronous, None);
    assert_eq!(activation.fw_scale, None);
    assert_eq!(activation.fw_position, None);
}

#[test]
fn movie_time_accepts_eight_byte_hex_strings() {
    let mut expected = [0u8; 8];
    expected[6] = 0x03;
    expected[7] = 0xE8;
    let dict = Dict::new().insert("Duration", Value::HexString("00000000000003E8".into()));

    let activation = parse_movie_activation(&Document::new(), &dict).unwrap();
    assert_eq!(
        activation.duration,
        Some(MovieTime {
            value: MovieTimeValue::Bytes(expected),
            time_scale: None,
        })
    );
}

#[test]
fn movie_mode_preserves_unknown_names() {
    let dict = Dict::new().insert("Mode", Value::Name("Bounce".into()));
    let activation = parse_movie_activation(&Document::new(), &dict).unwrap();
    assert_eq!(activation.mode, MovieMode::Other("Bounce".into()));
    assert_eq!(activation.mode.name(), "Bounce");
}

#[test]
fn rejects_malformed_activation_entries() {
    for dict in [
        Dict::new().insert("Start", Value::Integer(-1)),
        Dict::new().insert("Start", Value::String("\x00\x01".into())),
        Dict::new().insert(
            "Start",
            Value::Array(crate::core::types::Array(vec![
                Value::Integer(1),
                Value::Integer(0),
            ])),
        ),
        Dict::new().insert("Duration", Value::HexString("8000000000000000".into())),
        Dict::new().insert("Rate", Value::Name("fast".into())),
        Dict::new().insert("Volume", Value::Float(1.5)),
        Dict::new().insert("ShowControls", Value::Integer(1)),
        Dict::new().insert("Mode", Value::Integer(1)),
        Dict::new().insert(
            "FWScale",
            Value::Array(crate::core::types::Array(vec![Value::Integer(0)])),
        ),
        Dict::new().insert(
            "FWPosition",
            Value::Array(crate::core::types::Array(vec![
                Value::Float(0.5),
                Value::Float(1.5),
            ])),
        ),
    ] {
        let error =
            parse_movie_activation(&Document::new(), &dict).expect_err("malformed activation");
        assert!(
            matches!(error, MovieError::Malformed(_)),
            "unexpected error: {error}"
        );
    }
}
