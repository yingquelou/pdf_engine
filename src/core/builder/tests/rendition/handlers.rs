//! media clip data / section 的 `/MH`/`/BE` 处理器字典写出
//! （13.2.4 Table 276/278）。

use std::sync::atomic::{AtomicUsize, Ordering};

use super::super::support::annot_object;
use super::super::*;
use super::support::{action_dict, link_with, screen};
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
        "pdf-engine-rendition-handlers-{}-{}.pdf",
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

fn clip_of(doc: &Document) -> ReaderMediaClip {
    let action = action_dict(&annot_object(doc, 3, 1));
    let rendition = action.get("R").and_then(Value::as_dict).expect("/R");
    doc.media_clip(rendition.get("C").expect("/C"))
        .expect("media clip parses")
}

fn section_clip(build: impl FnOnce(MediaClipSection) -> MediaClipSection) -> Document {
    let section = build(MediaClipSection::new(MediaClipData::new(
        MediaClipSource::Url("clip.mp4".into()),
    )));
    let mut b = PdfBuilder::with_version(PdfVersion::V1_5);
    let page = b.page(PageSize::A4);
    let target = screen(&mut b, page);
    b.add_annotation(
        page,
        link_with(Action::Rendition(RenditionAction::play(
            target,
            Rendition::media(section),
        ))),
    );
    build_and_parse(&mut b)
}

#[test]
fn media_clip_data_writes_mh_and_be_base_urls() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_5);
    let page = b.page(PageSize::A4);
    let target = screen(&mut b, page);
    b.add_annotation(
        page,
        link_with(Action::Rendition(RenditionAction::play(
            target,
            Rendition::media(
                MediaClipData::new(MediaClipSource::Url("clip.mp4".into()))
                    .with_base_url("https://must.example/")
                    .with_best_effort_base_url("https://best.example/"),
            ),
        ))),
    );
    let doc = build_and_parse(&mut b);
    let ReaderMediaClip::Data(data) = clip_of(&doc) else {
        panic!("expected media clip data");
    };
    assert_eq!(
        data.mh.as_ref().and_then(|mh| mh.base_url.as_deref()),
        Some("https://must.example/")
    );
    assert_eq!(
        data.be.as_ref().and_then(|be| be.base_url.as_deref()),
        Some("https://best.example/")
    );
}

#[test]
fn media_clip_section_writes_mh_and_be_offsets() {
    let begin = Value::Dict(Dict::new().insert("S", Value::Name("M".into())));
    let end = Value::Dict(Dict::new().insert("S", Value::Name("T".into())));
    let doc = section_clip(|section| {
        section
            .with_must_honour(
                MediaClipSectionHandler::new()
                    .with_begin(begin.clone())
                    .with_end(end.clone()),
            )
            .with_best_effort(MediaClipSectionHandler::new().with_begin(begin.clone()))
    });
    let ReaderMediaClip::Section(section) = clip_of(&doc) else {
        panic!("expected media clip section");
    };
    let mh = section.mh.expect("/MH present");
    assert_eq!(mh.begin, Some(begin.clone()));
    assert_eq!(mh.end, Some(end));
    let be = section.be.expect("/BE present");
    assert_eq!(be.begin, Some(begin));
    assert_eq!(be.end, None);
}

#[test]
fn media_clip_section_handler_rejects_non_dict_offset() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_5);
    let page = b.page(PageSize::A4);
    let target = screen(&mut b, page);
    let section =
        MediaClipSection::new(MediaClipData::new(MediaClipSource::Url("clip.mp4".into())))
            .with_must_honour(MediaClipSectionHandler::new().with_begin(Value::Integer(1)));
    b.add_annotation(
        page,
        link_with(Action::Rendition(RenditionAction::play(
            target,
            Rendition::media(section),
        ))),
    );
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("non-dict offset must be rejected");
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert!(
        error
            .to_string()
            .contains("must be a media offset dictionary"),
        "{error}"
    );
    assert!(out.is_empty());
}
