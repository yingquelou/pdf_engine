//! rendition / media clip 对象读端测试（13.2 Table 266–278）。

use crate::core::types::{Array, Dict, Object, Value};

use super::*;

mod params;

fn name(value: &str) -> Value {
    Value::Name(value.to_owned())
}

fn arr(values: Vec<Value>) -> Value {
    Value::Array(Array(values))
}

fn insert(doc: &Document, num: i64, value: Value) {
    doc.set_object(Object::new(num, 0, value));
}

fn filespec(name: &str) -> Dict {
    Dict::new()
        .insert("Type", Value::Name("Filespec".into()))
        .insert("FS", Value::Name("URL".into()))
        .insert("F", Value::String(name.into()))
}

fn clip_data(destination: Value) -> Dict {
    Dict::new()
        .insert("Type", name("MediaClip"))
        .insert("S", name("MCD"))
        .insert("D", destination)
}

fn media_rendition(clip: Value) -> Dict {
    Dict::new()
        .insert("Type", name("Rendition"))
        .insert("S", name("MR"))
        .insert("C", clip)
}

fn playback_clip() -> Value {
    Value::Dict(clip_data(Value::Dict(filespec("clip.mov"))))
}

fn rendition_with_play(play: Value) -> Value {
    Value::Dict(media_rendition(playback_clip()).insert("P", play))
}

fn rendition_with_screen(screen: Value) -> Value {
    Value::Dict(media_rendition(playback_clip()).insert("SP", screen))
}

fn settings(entries: Dict) -> Value {
    Value::Dict(entries)
}

fn duration(subtype: &str, timespan: Option<Value>) -> Value {
    let mut dict = Dict::new()
        .insert("Type", name("MediaDuration"))
        .insert("S", name(subtype));
    if let Some(timespan) = timespan {
        dict = dict.insert("T", timespan);
    }
    Value::Dict(dict)
}

fn timespan(seconds: Value) -> Value {
    Value::Dict(
        Dict::new()
            .insert("Type", name("Timespan"))
            .insert("S", name("S"))
            .insert("V", seconds),
    )
}

fn floating_window(entries: Dict) -> Value {
    Value::Dict(entries.insert("Type", name("FWParams")))
}

#[test]
fn parses_media_rendition_and_clip_data() {
    let doc = Document::new();
    insert(
        &doc,
        10,
        Value::Dict(filespec("https://example.com/clip.mov")),
    );
    insert(
        &doc,
        11,
        Value::Dict(
            clip_data(Value::Ref(10, 0))
                .insert("N", Value::String("clip".into()))
                .insert("CT", Value::String("video/quicktime".into()))
                .insert(
                    "P",
                    Value::Dict(
                        Dict::new()
                            .insert("Type", name("MediaPermissions"))
                            .insert("TF", Value::String("TEMPEXTRACT".into())),
                    ),
                )
                .insert(
                    "MH",
                    Value::Dict(Dict::new().insert("BU", Value::String("https://cdn".into()))),
                )
                .insert("PL", Value::Dict(Dict::new())),
        ),
    );
    insert(
        &doc,
        12,
        Value::Dict(
            media_rendition(Value::Ref(11, 0))
                .insert("N", Value::String("main".into()))
                .insert("P", Value::Dict(Dict::new()))
                .insert("SP", Value::Dict(Dict::new()))
                .insert("MH", Value::Dict(Dict::new()))
                .insert("BE", Value::Dict(Dict::new())),
        ),
    );

    let rendition = doc.rendition(&Value::Ref(12, 0)).expect("rendition");
    let Rendition::Media(media) = rendition else {
        panic!("expected media rendition");
    };
    assert_eq!(media.name.as_deref(), Some("main"));
    assert!(media.play.is_some());
    assert!(media.screen.is_some());
    assert!(media.mh.is_some());
    assert!(media.be.is_some());
    let clip = media.clip.expect("clip");
    let MediaClip::Data(data) = clip else {
        panic!("expected media clip data");
    };
    assert_eq!(data.name.as_deref(), Some("clip"));
    assert_eq!(data.content_type.as_deref(), Some("video/quicktime"));
    let MediaClipDataSource::FileSpec(filespec) = data.data else {
        panic!("expected filespec source");
    };
    assert_eq!(filespec.file_system.as_deref(), Some("URL"));
    assert_eq!(
        filespec.short_name.as_deref(),
        Some("https://example.com/clip.mov")
    );
    assert_eq!(
        data.permissions.expect("permissions").temporary_file,
        TemporaryFilePermission::Extract
    );
    assert_eq!(
        data.mh.expect("MH").base_url.as_deref(),
        Some("https://cdn")
    );
    assert!(data.players.is_some());
}

#[test]
fn parses_media_clip_section_chain_and_offsets() {
    let doc = Document::new();
    insert(
        &doc,
        20,
        Value::Dict(clip_data(Value::Dict(filespec("clip.mov")))),
    );
    insert(
        &doc,
        21,
        Value::Dict(
            Dict::new()
                .insert("S", name("MCS"))
                .insert("D", Value::Ref(20, 0))
                .insert("N", Value::String("intro".into()))
                .insert(
                    "MH",
                    Value::Dict(
                        Dict::new()
                            .insert("B", Value::Dict(Dict::new().insert("S", name("T"))))
                            .insert("E", Value::Dict(Dict::new().insert("S", name("F")))),
                    ),
                ),
        ),
    );

    let clip = doc.media_clip(&Value::Ref(21, 0)).expect("clip");
    let MediaClip::Section(section) = clip else {
        panic!("expected media clip section");
    };
    assert_eq!(section.name.as_deref(), Some("intro"));
    let handler = section.mh.expect("MH");
    assert!(handler.begin.is_some());
    assert!(handler.end.is_some());
    assert!(section.be.is_none());
    assert!(matches!(*section.next, MediaClip::Data(_)));
}

#[test]
fn parses_selector_renditions_recursively() {
    let doc = Document::new();
    insert(
        &doc,
        30,
        Value::Dict(media_rendition(Value::Dict(clip_data(Value::Dict(
            filespec("a.mov"),
        ))))),
    );
    insert(
        &doc,
        31,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Rendition"))
                .insert("S", name("SR"))
                .insert("R", arr(vec![Value::Ref(30, 0), Value::Ref(32, 0)])),
        ),
    );
    insert(
        &doc,
        32,
        Value::Dict(Dict::new().insert("S", name("SR")).insert("R", arr(vec![]))),
    );

    let rendition = doc.rendition(&Value::Ref(31, 0)).expect("rendition");
    let Rendition::Selector(selector) = rendition else {
        panic!("expected selector rendition");
    };
    assert_eq!(selector.renditions.len(), 2);
    assert!(matches!(selector.renditions[0], Rendition::Media(_)));
    match &selector.renditions[1] {
        Rendition::Selector(empty) => assert!(empty.renditions.is_empty()),
        other => panic!("unexpected rendition: {other:?}"),
    }
}

#[test]
fn media_rendition_defaults_keep_optional_entries_absent() {
    let clip = Value::Dict(clip_data(Value::Dict(filespec("clip.mov"))));
    let rendition = Document::new()
        .rendition(&Value::Dict(media_rendition(clip)))
        .expect("rendition");
    let Rendition::Media(media) = rendition else {
        panic!("expected media rendition");
    };
    assert!(media.name.is_none());
    assert!(media.play.is_none());
    assert!(media.screen.is_none());
    assert!(media.mh.is_none());
    assert!(media.be.is_none());
    let MediaClip::Data(data) = media.clip.expect("clip") else {
        panic!("expected media clip data");
    };
    assert!(data.content_type.is_none());
    assert!(data.permissions.is_none());
    assert!(data.mh.is_none());
    assert!(data.be.is_none());
}

#[test]
fn unknown_subtypes_are_preserved() {
    let doc = Document::new();
    let rendition = doc
        .rendition(&Value::Dict(
            Dict::new()
                .insert("S", name("XR"))
                .insert("N", Value::String("future".into())),
        ))
        .expect("rendition");
    assert_eq!(
        rendition,
        Rendition::Other {
            subtype: "XR".into(),
            name: Some("future".into()),
        }
    );
    let clip = doc
        .media_clip(&Value::Dict(
            Dict::new()
                .insert("S", name("MCX"))
                .insert("N", Value::String("future".into())),
        ))
        .expect("clip");
    assert_eq!(
        clip,
        MediaClip::Other {
            subtype: "MCX".into(),
            name: Some("future".into()),
        }
    );
}

#[test]
fn media_permissions_default_to_never_and_preserve_unknown_names() {
    let doc = Document::new();
    let clip = doc
        .media_clip(&Value::Dict(
            clip_data(Value::Dict(filespec("clip.mov"))).insert("P", Value::Dict(Dict::new())),
        ))
        .expect("clip");
    let MediaClip::Data(data) = clip else {
        panic!("expected media clip data");
    };
    assert_eq!(
        data.permissions.expect("permissions").temporary_file,
        TemporaryFilePermission::Never
    );

    let clip = doc
        .media_clip(&Value::Dict(
            clip_data(Value::Dict(filespec("clip.mov"))).insert(
                "P",
                Value::Dict(Dict::new().insert("TF", Value::String("TEMPSOMETIME".into()))),
            ),
        ))
        .expect("clip");
    let MediaClip::Data(data) = clip else {
        panic!("expected media clip data");
    };
    assert_eq!(
        data.permissions.expect("permissions").temporary_file,
        TemporaryFilePermission::Other("TEMPSOMETIME".into())
    );
}

#[test]
fn rejects_malformed_rendition_and_clip_entries() {
    let doc = Document::new();
    for (value, message) in [
        (Value::Dict(Dict::new()), "missing a name /S"),
        (
            Value::Dict(Dict::new().insert("S", name("SR"))),
            "missing /R",
        ),
        (
            Value::Dict(Dict::new().insert("S", name("SR")).insert("R", name("x"))),
            "/R is not an array",
        ),
        (
            Value::Dict(
                Dict::new()
                    .insert("S", name("MR"))
                    .insert("C", Value::Dict(Dict::new().insert("S", name("MCD")))),
            ),
            "missing /D",
        ),
        (
            Value::Dict(
                Dict::new()
                    .insert("S", name("MR"))
                    .insert("C", Value::Dict(clip_data(Value::String("x".into())))),
            ),
            "neither a filespec nor a stream",
        ),
        (
            Value::Dict(Dict::new().insert("S", name("MR")).insert(
                "C",
                Value::Dict(clip_data(Value::Dict(filespec("clip.mov"))).insert(
                    "P",
                    Value::Dict(Dict::new().insert("TF", Value::String("临时".into()))),
                )),
            )),
            "media permissions /TF is not ASCII",
        ),
        (
            Value::Dict(
                Dict::new()
                    .insert("Type", name("MediaClip"))
                    .insert("S", name("MR")),
            ),
            "/Type is MediaClip, expected Rendition",
        ),
    ] {
        let error = doc.rendition(&value).expect_err("malformed rendition");
        assert!(
            error.to_string().contains(message),
            "expected {message:?} in {error}"
        );
    }
}

#[test]
fn rendition_reference_cycles_are_bounded() {
    let doc = Document::new();
    insert(
        &doc,
        40,
        Value::Dict(
            Dict::new()
                .insert("S", name("MR"))
                .insert("C", Value::Ref(41, 0)),
        ),
    );
    insert(
        &doc,
        41,
        Value::Dict(
            Dict::new()
                .insert("S", name("MCS"))
                .insert("D", Value::Ref(40, 0)),
        ),
    );
    let error = doc.rendition(&Value::Ref(40, 0)).expect_err("cycle");
    assert!(error.to_string().contains("forms a cycle"), "{error}");
}
