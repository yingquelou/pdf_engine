//! 声音对象（13.3 Table 294）、Sound 注解（12.5.6.16 Table 185）与
//! Sound 动作（12.6.4.8 Table 208）写出 → 解析 → 回读往返。

use super::*;

/// 注册一个声音对象，同时挂到 Sound 注解与 Link `/A` 动作；解析后
/// 两路 `/Sound` 都回读为同一份类型化声音数据，播放参数按声明回读。
#[test]
fn builder_sounds_roundtrip_through_parser() {
    use pdf_engine::core::builder::{
        Action, AnnotationKind, PageSize, PdfAnnotation, PdfBuilder, Sound, SoundAction,
        SoundEncoding,
    };
    use pdf_engine::core::reader::action::Action as ReadAction;
    use pdf_engine::core::reader::annotation::AnnotationSubtype;

    let mut builder = PdfBuilder::new();
    let p0 = builder.page(PageSize::A4);
    let sound = builder.add_sound(
        Sound::new(vec![1, 2, 3, 4], 22050.0, 2, 16).with_encoding(SoundEncoding::Signed),
    );
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Sound, [72.0, 600.0, 100.0, 620.0])
            .with_sound(sound)
            .with_sound_icon_name("Mic"),
    );
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Link, [72.0, 500.0, 200.0, 520.0]).with_action(
            Action::Sound(SoundAction::new(sound).with_volume(0.75).with_repeat(true)),
        ),
    );
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build sounds");
    let doc = parse_buf(&buf);

    let annotations = doc.page_annotations(0).expect("annotations");
    assert_eq!(annotations.len(), 2);

    let annotation = &annotations[0];
    assert_eq!(annotation.subtype, AnnotationSubtype::Sound);
    assert_eq!(annotation.icon_name.as_deref(), Some("Mic"));
    let parsed = doc
        .annotation_sound(annotation)
        .expect("typed sound")
        .expect("present");
    assert_eq!(parsed.sample_rate, 22050.0);
    assert_eq!(parsed.channels, 2);
    assert_eq!(parsed.bits_per_sample, 16);
    assert_eq!(parsed.encoding, SoundEncoding::Signed);
    assert_eq!(parsed.compression, None);
    assert_eq!(parsed.data, vec![1, 2, 3, 4]);

    let link = &annotations[1];
    assert_eq!(link.subtype, AnnotationSubtype::Link);
    let action = link.action.as_ref().expect("/A");
    match doc.action(action).expect("typed action") {
        ReadAction::Sound(action) => {
            assert_eq!(action.sound.sample_rate, 22050.0);
            assert_eq!(action.sound.data, vec![1, 2, 3, 4]);
            assert_eq!(action.volume, Some(0.75));
            assert_eq!(action.repeat, Some(true));
            assert_eq!(action.synchronous, None);
            assert_eq!(action.mix, None);
        }
        other => panic!("unexpected action: {other:?}"),
    }
}
