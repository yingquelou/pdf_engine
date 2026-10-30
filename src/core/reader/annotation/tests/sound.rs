//! Sound 注解专有条目读取（12.5.6.16 Table 185）：`/Sound` 与 `/Name`。

use crate::core::reader::annotation::*;
use crate::core::reader::sound::SoundEncoding;
use crate::core::types::{Dict, Object, StreamData, Value};

use super::support::*;

const SOUND_OBJECT: i64 = 30;

fn sound_stream() -> Object {
    Object::new(
        SOUND_OBJECT,
        0,
        Value::Stream(
            Dict::new()
                .insert("Type", Value::Name("Sound".into()))
                .insert("R", Value::Integer(22050))
                .insert("C", Value::Integer(2))
                .insert("B", Value::Integer(16))
                .insert("E", Value::Name("Signed".into())),
            StreamData::Owned(vec![1, 2, 3, 4]),
        ),
    )
}

#[test]
fn sound_annotation_reads_sound_and_icon_name() {
    let annotation_dict = base_annotation("Sound")
        .insert("Sound", Value::Ref(SOUND_OBJECT, 0))
        .insert("Name", Value::Name("Mic".into()));
    let document = doc_with_annots(
        vec![Value::Ref(20, 0)],
        vec![
            Object::new(20, 0, Value::Dict(annotation_dict)),
            sound_stream(),
        ],
    );

    let annotations = document.page_annotations(0).expect("page annotations");
    assert_eq!(annotations.len(), 1);
    let annotation = &annotations[0];
    assert_eq!(annotation.subtype, AnnotationSubtype::Sound);
    assert_eq!(annotation.sound, Some(Value::Ref(SOUND_OBJECT, 0)));
    assert_eq!(annotation.icon_name.as_deref(), Some("Mic"));

    let sound = document
        .annotation_sound(annotation)
        .expect("typed sound")
        .expect("present");
    assert_eq!(sound.sample_rate, 22050.0);
    assert_eq!(sound.channels, 2);
    assert_eq!(sound.bits_per_sample, 16);
    assert_eq!(sound.encoding, SoundEncoding::Signed);
    assert_eq!(sound.data, vec![1, 2, 3, 4]);
}

#[test]
fn sound_annotation_icon_default_is_not_materialized() {
    let document = doc_with_annots(
        vec![Value::Ref(20, 0)],
        vec![
            Object::new(
                20,
                0,
                Value::Dict(base_annotation("Sound").insert("Sound", Value::Ref(SOUND_OBJECT, 0))),
            ),
            sound_stream(),
        ],
    );
    let annotation = &document.page_annotations(0).expect("annotations")[0];
    assert_eq!(annotation.icon_name, None, "缺省 Speaker 由呈现端应用");
}

#[test]
fn non_sound_annotations_have_no_sound() {
    let document = doc(vec![catalog_object()]);
    let annotation =
        parse_annotation(&document, 0, None, &base_annotation("Text")).expect("text annotation");
    assert_eq!(annotation.sound, None);
    assert_eq!(
        document
            .annotation_sound(&annotation)
            .expect("absent /Sound"),
        None
    );

    let error = parse_annotation(
        &document,
        0,
        None,
        &base_annotation("Text").insert("Sound", Value::Ref(SOUND_OBJECT, 0)),
    )
    .expect_err("/Sound on Text");
    assert!(
        matches!(&error, AnnotationError::Malformed(message) if message.contains("/Sound")),
        "{error:?}"
    );
}

#[test]
fn malformed_sound_annotations_are_typed_errors() {
    let document = doc_with_annots(
        vec![],
        vec![
            sound_stream(),
            Object::new(
                31,
                0,
                Value::Dict(Dict::new().insert("Type", Value::Name("NotASound".into()))),
            ),
        ],
    );

    for dict in [
        base_annotation("Sound"),
        base_annotation("Sound").insert(
            "Sound",
            Value::Stream(
                Dict::new().insert("R", Value::Integer(8000)),
                StreamData::Owned(Vec::new()),
            ),
        ),
        base_annotation("Sound")
            .insert("Sound", Value::Ref(SOUND_OBJECT, 0))
            .insert("Name", Value::Integer(7)),
    ] {
        let error = parse_annotation(&document, 0, None, &dict).expect_err("malformed sound");
        assert!(
            matches!(&error, AnnotationError::Malformed(_)),
            "{dict:?}: {error:?}"
        );
    }

    let annotation = parse_annotation(
        &document,
        0,
        None,
        &base_annotation("Sound").insert("Sound", Value::Ref(31, 0)),
    )
    .expect("reference is well-formed");
    assert!(matches!(
        document.annotation_sound(&annotation),
        Err(AnnotationError::Malformed(_))
    ));
}
