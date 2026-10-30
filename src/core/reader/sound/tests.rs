//! 声音对象读端测试（13.3 Table 294）。

use crate::core::types::{Dict, Document, Object, StreamData, Value};

use super::*;

/// 构造带一个声音流对象（对象号 10）的文档。
fn doc_with_sound(dict: Dict, data: &[u8]) -> (Document, i64) {
    let doc = Document::new();
    let number = 10;
    doc.set_object(Object::new(
        number,
        0,
        Value::Stream(dict, StreamData::Owned(data.to_vec())),
    ));
    (doc, number)
}

fn sound_dict() -> Dict {
    Dict::new().insert("Type", Value::Name("Sound".into()))
}

#[test]
fn parses_sound_stream_fields_and_data() {
    let dict = sound_dict()
        .insert("R", Value::Integer(22050))
        .insert("C", Value::Integer(2))
        .insert("B", Value::Integer(16))
        .insert("E", Value::Name("Signed".into()));
    let data = [0x01u8, 0x02, 0x03, 0x04];
    let (doc, number) = doc_with_sound(dict, &data);

    let sound = doc.sound(&Value::Ref(number, 0)).unwrap();
    assert_eq!(sound.sample_rate, 22050.0);
    assert_eq!(sound.channels, 2);
    assert_eq!(sound.bits_per_sample, 16);
    assert_eq!(sound.encoding, SoundEncoding::Signed);
    assert_eq!(sound.compression, None);
    assert_eq!(sound.data, data);
}

#[test]
fn applies_table_294_defaults() {
    let (doc, number) = doc_with_sound(Dict::new().insert("R", Value::Float(8000.0)), b"\x01\x02");

    let sound = doc.sound(&Value::Ref(number, 0)).unwrap();
    assert_eq!(sound.sample_rate, 8000.0);
    assert_eq!(sound.channels, 1);
    assert_eq!(sound.bits_per_sample, 8);
    assert_eq!(sound.encoding, SoundEncoding::Raw);
    assert_eq!(sound.compression, None);
    assert_eq!(sound.data, b"\x01\x02");
}

#[test]
fn accepts_direct_streams() {
    let dict = Dict::new().insert("R", Value::Integer(11025));
    let value = Value::Stream(dict, StreamData::Owned(vec![7, 8, 9]));

    let sound = Document::new().sound(&value).unwrap();
    assert_eq!(sound.sample_rate, 11025.0);
    assert_eq!(sound.encoding, SoundEncoding::Raw);
    assert_eq!(sound.data, vec![7, 8, 9]);
}

#[test]
fn parses_nonstandard_encoding_and_compression_names() {
    let dict = sound_dict()
        .insert("R", Value::Integer(44100))
        .insert("E", Value::Name("Flac".into()))
        .insert("CO", Value::Name("CustomPack".into()));
    let (doc, number) = doc_with_sound(dict, b"abc");

    let sound = doc.sound(&Value::Ref(number, 0)).unwrap();
    assert_eq!(sound.encoding, SoundEncoding::Other("Flac".into()));
    assert_eq!(sound.encoding.name(), "Flac");
    assert_eq!(sound.compression.as_deref(), Some("CustomPack"));
}

#[test]
fn malformed_sound_objects_are_typed_errors() {
    let (doc, _) = doc_with_sound(Dict::new(), b"");
    let cases = [
        Value::Integer(3),
        Value::Dict(Dict::new().insert("R", Value::Integer(8000))),
        Value::String("sound".into()),
    ];
    for case in cases {
        assert!(
            matches!(doc.sound(&case), Err(SoundError::Malformed(_))),
            "{case:?}"
        );
    }

    let bad_dicts = [
        sound_dict(),
        sound_dict().insert("R", Value::String("8000".into())),
        sound_dict().insert("R", Value::Integer(0)),
        sound_dict().insert("R", Value::Float(-1.0)),
        sound_dict()
            .insert("R", Value::Integer(8000))
            .insert("C", Value::Integer(0)),
        sound_dict()
            .insert("R", Value::Integer(8000))
            .insert("C", Value::Float(2.0)),
        sound_dict()
            .insert("R", Value::Integer(8000))
            .insert("B", Value::Integer(-8)),
        sound_dict()
            .insert("R", Value::Integer(8000))
            .insert("E", Value::Integer(1)),
        sound_dict()
            .insert("R", Value::Integer(8000))
            .insert("CO", Value::String("pack".into())),
        Dict::new()
            .insert("Type", Value::Name("NotSound".into()))
            .insert("R", Value::Integer(8000)),
    ];
    for dict in bad_dicts {
        let (doc, number) = doc_with_sound(dict, b"");
        assert!(
            matches!(
                doc.sound(&Value::Ref(number, 0)),
                Err(SoundError::Malformed(_))
            ),
            "expected malformed"
        );
    }
}

#[test]
fn undecodable_sound_streams_are_decode_errors() {
    let dict = sound_dict()
        .insert("R", Value::Integer(8000))
        .insert("Filter", Value::Name("NoSuchFilter".into()));
    let (doc, number) = doc_with_sound(dict, b"data");

    assert!(matches!(
        doc.sound(&Value::Ref(number, 0)),
        Err(SoundError::Decode(_))
    ));
}
