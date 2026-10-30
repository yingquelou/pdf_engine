//! Sound 动作读端测试（12.6.4.8 Table 208）。

use crate::core::reader::sound::{Sound, SoundEncoding};
use crate::core::types::{Dict, StreamData, Value};

use super::*;

const SOUND_OBJECT: i64 = 70;

fn insert_sound(doc: &Document, data: &[u8]) {
    insert(
        doc,
        SOUND_OBJECT,
        Value::Stream(
            Dict::new()
                .insert("Type", name("Sound"))
                .insert("R", Value::Integer(22050))
                .insert("C", Value::Integer(2))
                .insert("B", Value::Integer(16))
                .insert("E", name("Signed")),
            StreamData::Owned(data.to_vec()),
        ),
    );
}

fn expected_sound(data: &[u8]) -> Sound {
    Sound {
        sample_rate: 22050.0,
        channels: 2,
        bits_per_sample: 16,
        encoding: SoundEncoding::Signed,
        compression: None,
        data: data.to_vec(),
    }
}

#[test]
fn parses_sound_action_with_all_entries() {
    let doc = document_with_pages(1);
    insert_sound(&doc, &[1, 2, 3, 4]);
    let action = Value::Dict(
        action_dict("Sound")
            .insert("Sound", Value::Ref(SOUND_OBJECT, 0))
            .insert("Volume", Value::Float(0.5))
            .insert("Synchronous", Value::Bool(true))
            .insert("Repeat", Value::Bool(false))
            .insert("Mix", Value::Bool(true)),
    );

    assert_eq!(
        doc.action(&action).unwrap(),
        Action::Sound(SoundAction {
            sound: expected_sound(&[1, 2, 3, 4]),
            volume: Some(0.5),
            synchronous: Some(true),
            repeat: Some(false),
            mix: Some(true),
        })
    );
}

#[test]
fn sound_action_defaults_are_not_materialized() {
    let doc = document_with_pages(1);
    insert_sound(&doc, &[9]);
    let action = Value::Dict(action_dict("Sound").insert("Sound", Value::Ref(SOUND_OBJECT, 0)));

    assert_eq!(
        doc.action(&action).unwrap(),
        Action::Sound(SoundAction {
            sound: expected_sound(&[9]),
            volume: None,
            synchronous: None,
            repeat: None,
            mix: None,
        })
    );
}

#[test]
fn sound_action_volume_bounds_are_inclusive() {
    let doc = document_with_pages(1);
    insert_sound(&doc, &[]);
    for volume in [-1.0, 1.0] {
        let action = Value::Dict(
            action_dict("Sound")
                .insert("Sound", Value::Ref(SOUND_OBJECT, 0))
                .insert("Volume", Value::Float(volume)),
        );
        match doc.action(&action).unwrap() {
            Action::Sound(parsed) => assert_eq!(parsed.volume, Some(volume)),
            other => panic!("unexpected action: {other:?}"),
        }
    }
}

#[test]
fn malformed_sound_actions_are_typed_errors() {
    let doc = document_with_pages(1);
    insert_sound(&doc, b"data");
    insert(&doc, 71, name("not-a-stream"));
    insert(
        &doc,
        72,
        Value::Stream(Dict::new(), StreamData::Owned(Vec::new())),
    );

    let cases = [
        Value::Dict(action_dict("Sound")),
        Value::Dict(action_dict("Sound").insert(
            "Sound",
            Value::Stream(
                Dict::new().insert("R", Value::Integer(8000)),
                StreamData::Owned(Vec::new()),
            ),
        )),
        Value::Dict(action_dict("Sound").insert("Sound", Value::Ref(71, 0))),
        Value::Dict(action_dict("Sound").insert("Sound", Value::Ref(72, 0))),
        Value::Dict(
            action_dict("Sound")
                .insert("Sound", Value::Ref(SOUND_OBJECT, 0))
                .insert("Volume", Value::Float(1.5)),
        ),
        Value::Dict(
            action_dict("Sound")
                .insert("Sound", Value::Ref(SOUND_OBJECT, 0))
                .insert("Volume", Value::Integer(-2)),
        ),
        Value::Dict(
            action_dict("Sound")
                .insert("Sound", Value::Ref(SOUND_OBJECT, 0))
                .insert("Volume", Value::Bool(true)),
        ),
        Value::Dict(
            action_dict("Sound")
                .insert("Sound", Value::Ref(SOUND_OBJECT, 0))
                .insert("Synchronous", Value::Integer(1)),
        ),
        Value::Dict(
            action_dict("Sound")
                .insert("Sound", Value::Ref(SOUND_OBJECT, 0))
                .insert("Repeat", name("true")),
        ),
        Value::Dict(
            action_dict("Sound")
                .insert("Sound", Value::Ref(SOUND_OBJECT, 0))
                .insert("Mix", Value::Integer(0)),
        ),
    ];
    for case in cases {
        assert!(
            matches!(doc.action(&case), Err(ActionError::Malformed(_))),
            "{case:?}"
        );
    }
}
