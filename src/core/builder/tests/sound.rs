//! 声音对象（13.3 Table 294）、Sound 动作（12.6.4.8 Table 208）与 Sound
//! 注解（12.5.6.16 Table 185）写端：独立流对象分配、`/Sound` 引用解析、
//! 采样参数契约与 PDF 1.2 门禁。

use super::support::annot_object;
use super::*;
use crate::core::builder::{Sound, SoundAction, SoundEncoding};
use crate::core::types::{Dict, Document, PdfFeature, StreamData, Value};

/// 按对象号顺序收集 `/Type /Sound` 的流对象。
fn sound_streams(doc: &Document) -> Vec<(i64, Dict, Vec<u8>)> {
    let mut streams = Vec::new();
    for num in 1..=64 {
        let Some(object) = doc.get_object(num).expect("object lookup") else {
            continue;
        };
        if let Value::Stream(dict, data) = object.value
            && dict.get("Type").and_then(Value::as_name) == Some("Sound")
        {
            let StreamData::Owned(data) = data else {
                panic!("expected owned stream data")
            };
            streams.push((num, dict, data));
        }
    }
    streams
}

fn sound_annotation(handle: usize) -> PdfAnnotation {
    PdfAnnotation::new(AnnotationKind::Sound, [72.0, 600.0, 100.0, 620.0]).with_sound(handle)
}

/// 三项采样参数与显式编码/压缩全部写出，采样数据原样保留。
#[test]
fn sound_objects_are_written_as_indirect_streams() {
    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    let handle = b.add_sound(
        Sound::new(vec![1, 2, 3, 4], 22050.0, 2, 16)
            .with_encoding(SoundEncoding::Signed)
            .with_compression("Pack"),
    );
    assert_eq!(handle, 0);

    let doc = b.assemble_document().expect("assemble");
    let streams = sound_streams(&doc);
    assert_eq!(streams.len(), 1);
    let (_, dict, data) = &streams[0];
    assert_eq!(dict.get("Type"), Some(&Value::Name("Sound".into())));
    assert_eq!(dict.get("R"), Some(&Value::Float(22050.0)));
    assert_eq!(dict.get("C"), Some(&Value::Integer(2)));
    assert_eq!(dict.get("B"), Some(&Value::Integer(16)));
    assert_eq!(dict.get("E"), Some(&Value::Name("Signed".into())));
    assert_eq!(dict.get("CO"), Some(&Value::Name("Pack".into())));
    assert_eq!(data, &vec![1, 2, 3, 4]);
    assert!(doc.features.borrow().contains(PdfFeature::SoundObjects));
}

/// Table 294 缺省值不物化：只写 `/Type`、`/R` 与 `/Length`。
#[test]
fn sound_defaults_are_not_materialized() {
    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    b.add_sound(Sound::new(vec![7], 8000.0, 1, 8));

    let doc = b.assemble_document().expect("assemble");
    let (_, dict, data) = &sound_streams(&doc)[0];
    assert_eq!(dict.get("R"), Some(&Value::Float(8000.0)));
    assert_eq!(data, &vec![7]);
    for key in ["C", "B", "E", "CO"] {
        assert_eq!(dict.get(key), None, "缺省 {key} 不物化");
    }
}

/// Sound 注解与 Link `/A` Sound 动作都解析为同一声音流对象号；
/// 播放参数只在显式声明时写出。
#[test]
fn sound_annotation_and_action_reference_sound_objects() {
    let mut b = PdfBuilder::new();
    let p0 = b.page(PageSize::A4);
    let sound = b.add_sound(Sound::new(vec![9, 8], 8000.0, 1, 8));
    b.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Sound, [72.0, 600.0, 100.0, 620.0])
            .with_sound(sound)
            .with_sound_icon_name("Mic"),
    );
    b.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Link, [72.0, 500.0, 200.0, 520.0]).with_action(
            Action::Sound(
                SoundAction::new(sound)
                    .with_volume(0.5)
                    .with_synchronous(true)
                    .with_repeat(true)
                    .with_mix(true),
            ),
        ),
    );

    let doc = b.assemble_document().expect("assemble");
    let (sound_num, _, _) = &sound_streams(&doc)[0];
    let sound_annot = annot_object(&doc, 3, 0);
    assert_eq!(
        sound_annot.get("Subtype"),
        Some(&Value::Name("Sound".into()))
    );
    assert_eq!(sound_annot.get("Sound"), Some(&Value::Ref(*sound_num, 0)));
    assert_eq!(sound_annot.get("Name"), Some(&Value::Name("Mic".into())));

    let link = annot_object(&doc, 3, 1);
    let action = link
        .get("A")
        .and_then(Value::as_dict)
        .expect("Link /A dictionary");
    assert_eq!(action.get("S"), Some(&Value::Name("Sound".into())));
    assert_eq!(action.get("Sound"), Some(&Value::Ref(*sound_num, 0)));
    assert_eq!(action.get("Volume"), Some(&Value::Float(0.5)));
    assert_eq!(action.get("Synchronous"), Some(&Value::Bool(true)));
    assert_eq!(action.get("Repeat"), Some(&Value::Bool(true)));
    assert_eq!(action.get("Mix"), Some(&Value::Bool(true)));
}

/// 未声明播放参数时不物化 Table 208 的缺省值。
#[test]
fn sound_action_defaults_are_not_materialized() {
    let mut b = PdfBuilder::new();
    let p0 = b.page(PageSize::A4);
    let sound = b.add_sound(Sound::new(vec![1], 8000.0, 1, 8));
    b.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Link, [0.0, 0.0, 10.0, 10.0])
            .with_action(Action::Sound(SoundAction::new(sound))),
    );

    let doc = b.assemble_document().expect("assemble");
    let action = annot_object(&doc, 3, 0)
        .get("A")
        .and_then(Value::as_dict)
        .cloned()
        .expect("/A dictionary");
    for key in ["Volume", "Synchronous", "Repeat", "Mix"] {
        assert_eq!(action.get(key), None, "缺省 {key} 不物化");
    }
}

/// 采样参数非法、编码非标准或数据为空时零字节输出。
#[test]
fn invalid_sounds_are_rejected_without_output() {
    let cases = [
        Sound::new(vec![1], 0.0, 1, 8),
        Sound::new(vec![1], -8000.0, 1, 8),
        Sound::new(vec![1], 8000.0, 0, 8),
        Sound::new(vec![1], 8000.0, 1, 12),
        Sound::new(Vec::new(), 8000.0, 1, 8),
        Sound::new(vec![1], 8000.0, 1, 8).with_encoding(SoundEncoding::Other("Flac".into())),
        Sound::new(vec![1], 8000.0, 1, 8).with_compression("bad name"),
    ];
    for sound in cases {
        let mut b = PdfBuilder::new();
        b.page(PageSize::A4);
        b.add_sound(sound);
        let mut out = Vec::new();
        let error = b.build(&mut out).expect_err("invalid sound");
        assert!(
            matches!(error, WriteError::InvalidObject(_)),
            "expected InvalidObject, got {error}"
        );
        assert_eq!(out.len(), 0, "rejected build must not emit bytes");
    }
}

/// 声音对象自 PDF 1.2 引入；1.1 构建零字节输出。
#[test]
fn sound_objects_require_pdf_1_2() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_1);
    b.page(PageSize::A4);
    b.add_sound(Sound::new(vec![1], 8000.0, 1, 8));
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("version gate");
    assert!(
        matches!(error, WriteError::UnsupportedVersion(_)),
        "expected UnsupportedVersion, got {error}"
    );
    assert_eq!(out.len(), 0);
}

/// Sound 注解未声明 `/Sound`、或非 Sound 子类型声明 `/Sound` 均被拒绝。
#[test]
fn sound_annotation_requires_a_sound_reference() {
    let cases = [
        (
            PdfAnnotation::new(AnnotationKind::Sound, [0.0, 0.0, 10.0, 10.0]),
            "requires /Sound",
        ),
        (
            PdfAnnotation::new(AnnotationKind::Text, [0.0, 0.0, 10.0, 10.0]).with_sound(0),
            "specific to Sound",
        ),
    ];
    for (annot, fragment) in cases {
        let mut b = PdfBuilder::new();
        let p0 = b.page(PageSize::A4);
        b.add_sound(Sound::new(vec![1], 8000.0, 1, 8));
        b.add_annotation(p0, annot);
        let mut out = Vec::new();
        let error = b.build(&mut out).expect_err("sound annotation contract");
        let message = error.to_string();
        assert!(
            matches!(error, WriteError::InvalidObject(_)),
            "expected InvalidObject, got {message}"
        );
        assert!(message.contains(fragment), "unexpected message: {message}");
        assert_eq!(out.len(), 0);
    }
}

/// 未注册的声音句柄在注解与动作两路都被拒绝。
#[test]
fn unknown_sound_handles_are_rejected() {
    for annot in [
        sound_annotation(3),
        PdfAnnotation::new(AnnotationKind::Link, [0.0, 0.0, 10.0, 10.0])
            .with_action(Action::Sound(SoundAction::new(3))),
    ] {
        let mut b = PdfBuilder::new();
        let p0 = b.page(PageSize::A4);
        b.add_sound(Sound::new(vec![1], 8000.0, 1, 8));
        b.add_annotation(p0, annot);
        let mut out = Vec::new();
        let error = b.build(&mut out).expect_err("unknown sound handle");
        assert!(
            matches!(error, WriteError::InvalidObject(_)),
            "expected InvalidObject, got {error}"
        );
        assert_eq!(out.len(), 0);
    }
}

/// 音量越界与未注册句柄同样在写出前拒绝。
#[test]
fn invalid_sound_actions_are_rejected() {
    for action in [
        SoundAction::new(0).with_volume(1.5),
        SoundAction::new(0).with_volume(f64::NAN),
        SoundAction::new(9),
    ] {
        let mut b = PdfBuilder::new();
        let p0 = b.page(PageSize::A4);
        b.add_sound(Sound::new(vec![1], 8000.0, 1, 8));
        b.add_annotation(
            p0,
            PdfAnnotation::new(AnnotationKind::Link, [0.0, 0.0, 10.0, 10.0])
                .with_action(Action::Sound(action)),
        );
        let mut out = Vec::new();
        let error = b.build(&mut out).expect_err("invalid sound action");
        assert!(
            matches!(error, WriteError::InvalidObject(_)),
            "expected InvalidObject, got {error}"
        );
        assert_eq!(out.len(), 0);
    }
}
