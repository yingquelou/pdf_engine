//! media clip `/Alt` 写端（13.2.7 Table 274/277，14.9.2.4）：
//! 单一文本串与多语言文本数组的写出、解析回环与构建期校验。

use std::sync::atomic::{AtomicUsize, Ordering};

use super::super::support::annot_object;
use super::super::*;
use super::support::{action_dict, link_with, screen};
use crate::core::builder::types::AlternateText;
use crate::core::reader::accessibility::LanguageIdentifier;
use crate::core::reader::rendition::MediaClip as ReaderMediaClip;
use crate::core::types::Document;

fn build_and_parse(b: &mut PdfBuilder) -> Document {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let mut out = Vec::new();
    b.build(&mut out).expect("build");
    let base = std::env::current_exe()
        .ok()
        .and_then(|p| p.ancestors().nth(3).map(|t| t.to_path_buf()))
        .filter(|p| p.file_name() == Some(std::ffi::OsStr::new("target")))
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join("test-tmp");
    std::fs::create_dir_all(&dir).expect("create test-tmp");
    let path = dir.join(format!(
        "pdf-engine-rendition-alt-{}-{}.pdf",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&path, &out).expect("write temp pdf");
    let file = std::fs::File::open(&path).expect("open temp pdf");
    let mut parser = crate::core::reader::Parser::new(file).expect("parser");
    let doc = parser.parse_document().expect("parse");
    let _ = std::fs::remove_file(&path);
    doc
}

/// 构造带单个 Rendition 动作的文档，返回解析后的文档。
fn doc_with_clip(clip: impl Into<MediaClip>) -> Document {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_5);
    let page = b.page(PageSize::A4);
    let target = screen(&mut b, page);
    b.add_annotation(
        page,
        link_with(Action::Rendition(RenditionAction::play(
            target,
            Rendition::media(clip),
        ))),
    );
    let doc = build_and_parse(&mut b);
    let action = action_dict(&annot_object(&doc, 3, 1));
    let rendition = action.get("R").and_then(Value::as_dict).expect("/R");
    let clip_value = rendition.get("C").expect("/C");
    assert!(doc.media_clip(clip_value).is_ok(), "clip parses");
    doc
}

fn data_clip_alt(clip: &ReaderMediaClip) -> crate::core::reader::accessibility::AlternateText {
    let ReaderMediaClip::Data(data) = clip else {
        panic!("expected media clip data");
    };
    data.alternate_text()
        .expect("valid /Alt")
        .expect("/Alt present")
}

#[test]
fn media_clip_alt_text_roundtrips() {
    let doc = doc_with_clip(
        MediaClipData::new(MediaClipSource::Url("clip.mp4".into())).with_alt("intro"),
    );
    let action = action_dict(&annot_object(&doc, 3, 1));
    let rendition = action.get("R").and_then(Value::as_dict).expect("/R");
    let clip = doc
        .media_clip(rendition.get("C").expect("/C"))
        .expect("clip");
    assert_eq!(
        data_clip_alt(&clip).text_for(&LanguageIdentifier::parse("en").unwrap()),
        Some("intro")
    );

    let ReaderMediaClip::Data(data) = clip else {
        unreachable!()
    };
    match data.alt.as_ref().expect("/Alt") {
        Value::String(_) | Value::HexString(_) => {}
        other => panic!("expected text string at /Alt, got {other:?}"),
    }
}

#[test]
fn media_clip_multilingual_alt_roundtrips_and_selects_language() {
    let entries = vec![
        ("en-US".to_string(), "My vacation".to_string()),
        ("fr".to_string(), "mes vacances".to_string()),
        (String::new(), "default text".to_string()),
    ];
    let doc = doc_with_clip(
        MediaClipData::new(MediaClipSource::Url("clip.mp4".into()))
            .with_alt(AlternateText::multilingual(entries)),
    );
    let action = action_dict(&annot_object(&doc, 3, 1));
    let rendition = action.get("R").and_then(Value::as_dict).expect("/R");
    let clip = doc
        .media_clip(rendition.get("C").expect("/C"))
        .expect("clip");
    let alt = data_clip_alt(&clip);
    let lookup = |tag: &str| {
        alt.text_for(&LanguageIdentifier::parse(tag).unwrap())
            .map(str::to_string)
    };
    assert_eq!(lookup("en-US"), Some("My vacation".to_string()));
    assert_eq!(lookup("en"), Some("My vacation".to_string()), "前缀匹配");
    assert_eq!(lookup("fr"), Some("mes vacances".to_string()));
    assert_eq!(
        lookup("de"),
        Some("default text".to_string()),
        "默认文本兜底"
    );
}

#[test]
fn media_clip_section_alt_roundtrips() {
    let section =
        MediaClipSection::new(MediaClipData::new(MediaClipSource::Url("clip.mp4".into())))
            .with_alt("section description");
    let doc = doc_with_clip(section);
    let action = action_dict(&annot_object(&doc, 3, 1));
    let rendition = action.get("R").and_then(Value::as_dict).expect("/R");
    let clip = doc
        .media_clip(rendition.get("C").expect("/C"))
        .expect("clip");
    let ReaderMediaClip::Section(section) = clip else {
        panic!("expected media clip section");
    };
    let alt = section
        .alternate_text()
        .expect("valid /Alt")
        .expect("/Alt present");
    assert_eq!(
        alt.text_for(&LanguageIdentifier::parse("en").unwrap()),
        Some("section description")
    );
}

#[test]
fn media_clip_rejects_duplicate_or_invalid_alt_languages() {
    for entries in [
        vec![
            ("en".to_string(), "a".to_string()),
            ("EN".to_string(), "b".to_string()),
        ],
        vec![("en_US".to_string(), "a".to_string())],
    ] {
        let mut b = PdfBuilder::with_version(PdfVersion::V1_5);
        let page = b.page(PageSize::A4);
        let target = screen(&mut b, page);
        b.add_annotation(
            page,
            link_with(Action::Rendition(RenditionAction::play(
                target,
                Rendition::media(
                    MediaClipData::new(MediaClipSource::Url("clip.mp4".into()))
                        .with_alt(AlternateText::multilingual(entries)),
                ),
            ))),
        );
        let mut out = Vec::new();
        let error = b
            .build(&mut out)
            .expect_err("invalid /Alt must be rejected");
        assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
        assert!(
            error.to_string().contains("media clip /Alt identifier"),
            "{error}"
        );
        assert!(out.is_empty());
    }
}
