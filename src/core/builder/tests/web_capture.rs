//! Web Capture 写端（14.10）：页面集内容集、URLS/IDS 名称树与版本门禁。

use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::core::reader::web_capture::{ContentSet, UrlAliases};
use crate::core::types::{Document, PdfVersion};

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
        "pdf-engine-web-capture-{}-{}.pdf",
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

#[test]
fn web_capture_page_set_roundtrips_through_parser() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let first = b.page(PageSize::A4);
    let second = b.page(PageSize::A4);
    let set = WebCapturePageSet::new(vec![0xAA, 0xBB], [first, second])
        .with_source(WebCaptureSource::new("http://example.com/index.html").with_submission(1))
        .with_content_type("text/html")
        .with_timestamp("D:20261030120000Z")
        .with_title("Index")
        .with_text_id(vec![0x01, 0x02]);
    b.add_web_capture_page_set(set);
    let doc = build_and_parse(&mut b);

    let info = doc.web_capture_info().expect("info").expect("present");
    assert_eq!(info.version, 1.0);

    let by_url = doc
        .web_capture_sets_by_url("http://example.com/index.html")
        .expect("url lookup");
    assert_eq!(by_url.len(), 1);
    let ContentSet::Page(set) = &by_url[0] else {
        panic!("expected page set");
    };
    assert_eq!(set.id, vec![0xAA, 0xBB]);
    assert_eq!(set.objects.len(), 2, "两个页面引用");
    assert_eq!(set.content_type.as_deref(), Some("text/html"));
    assert_eq!(set.title.as_deref(), Some("Index"));
    assert_eq!(set.text_id.as_deref(), Some(&[0x01, 0x02][..]));
    assert_eq!(set.sources.len(), 1);
    assert_eq!(set.sources[0].submission, Some(1));
    assert_eq!(
        set.sources[0].urls,
        UrlAliases::Single("http://example.com/index.html".to_string())
    );

    let by_id = doc
        .web_capture_sets_by_id(&[0xAA, 0xBB])
        .expect("id lookup");
    assert_eq!(by_id.len(), 1);
    assert_eq!(by_id[0].id(), &[0xAA, 0xBB]);
}

#[test]
fn web_capture_url_alias_chains_roundtrip() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let source = WebCaptureSource::new("http://final.example/").with_alias_chain(vec![
        "http://a.example/".to_string(),
        "http://b.example/".to_string(),
    ]);
    b.add_web_capture_page_set(WebCapturePageSet::new(vec![0x10], [page]).with_source(source));
    let doc = build_and_parse(&mut b);
    let sets = doc
        .web_capture_sets_by_url("http://final.example/")
        .expect("lookup");
    assert_eq!(
        sets[0].sources()[0].urls,
        UrlAliases::Chains(vec![
            vec!["http://final.example/".to_string()],
            vec![
                "http://a.example/".to_string(),
                "http://b.example/".to_string()
            ],
        ])
    );
}

#[test]
fn page_preferred_zoom_and_web_capture_id_roundtrip() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    b.set_page_preferred_zoom(page, 2.5);
    b.set_page_web_capture_id(page, vec![0xDE, 0xAD, 0xBE, 0xEF]);
    let doc = build_and_parse(&mut b);
    assert_eq!(doc.page_preferred_zoom(page).expect("pz"), Some(2.5));
    assert_eq!(
        doc.page_web_capture_id(page).expect("id"),
        Some(vec![0xDE, 0xAD, 0xBE, 0xEF])
    );
}

#[test]
fn page_preferred_zoom_rejects_non_positive_and_requires_1_3() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_2);
    let page = b.page(PageSize::A4);
    b.set_page_preferred_zoom(page, 1.5);
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("PZ needs 1.3");
    assert!(
        matches!(error, WriteError::UnsupportedVersion(_)),
        "{error}"
    );
    assert!(out.is_empty());

    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    b.set_page_preferred_zoom(page, 0.0);
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("non-positive zoom");
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert!(out.is_empty());
}

fn tiny_image() -> ImageData {
    ImageData {
        intent: None,
        alternates: Vec::new(),
        oc: None,
        metadata: None,
        matte: None,
        width: 2,
        height: 2,
        color_space: ColorSpace::DeviceRGB,
        bits: 8,
        data: vec![255, 0, 0, 255, 0, 0, 255, 0, 0, 255, 0, 0],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    }
}

#[test]
fn web_capture_image_set_roundtrips() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let image = tiny_image();
    b.image(page, 10.0, 10.0, 20.0, 20.0, &image);
    b.add_web_capture_image_set(
        WebCaptureImageSet::new(vec![0x11, 0x22], [(page, 0)])
            .with_source(WebCaptureSource::new("http://example.com/pic.png"))
            .with_content_type("image/png")
            .with_reference_counts([3]),
    );
    let doc = build_and_parse(&mut b);

    let by_id = doc
        .web_capture_sets_by_id(&[0x11, 0x22])
        .expect("id lookup");
    assert_eq!(by_id.len(), 1);
    let ContentSet::Image(set) = &by_id[0] else {
        panic!("expected image set");
    };
    assert_eq!(set.objects.len(), 1, "一个图像 XObject 引用");
    assert_eq!(set.reference_counts, vec![3]);
    assert_eq!(set.content_type.as_deref(), Some("image/png"));

    let by_url = doc
        .web_capture_sets_by_url("http://example.com/pic.png")
        .expect("url lookup");
    assert_eq!(by_url.len(), 1);
}

#[test]
fn web_capture_rejects_image_in_two_sets_and_out_of_range_slot() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let image = tiny_image();
    b.image(page, 10.0, 10.0, 20.0, 20.0, &image);
    b.add_web_capture_image_set(
        WebCaptureImageSet::new(vec![0x01], [(page, 0)])
            .with_source(WebCaptureSource::new("http://a/")),
    )
    .add_web_capture_image_set(
        WebCaptureImageSet::new(vec![0x02], [(page, 0)])
            .with_source(WebCaptureSource::new("http://b/")),
    );
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("image shared by two sets");
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert!(error.to_string().contains("more than one"), "{error}");
    assert!(out.is_empty());

    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let image = tiny_image();
    b.image(page, 10.0, 10.0, 20.0, 20.0, &image);
    b.add_web_capture_image_set(
        WebCaptureImageSet::new(vec![0x01], [(page, 4)])
            .with_source(WebCaptureSource::new("http://a/")),
    );
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("out-of-range image slot");
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert!(out.is_empty());
}

#[test]
fn web_capture_command_roundtrips_through_spider_info() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let _ = b.page(PageSize::A4);
    b.add_web_capture_command(
        WebCaptureCommand::new("http://example.com/")
            .with_levels(2)
            .with_flags(0b011)
            .with_content_type("text/html")
            .with_headers("X-Test: 1"),
    );
    let doc = build_and_parse(&mut b);
    let info = doc.web_capture_info().expect("info").expect("present");
    let commands = doc.web_capture_commands(&info).expect("commands");
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].url, "http://example.com/");
    assert_eq!(commands[0].levels, 2);
    assert!(commands[0].same_site());
    assert!(commands[0].same_path());
    assert!(!commands[0].submit());
    assert_eq!(commands[0].content_type.as_deref(), Some("text/html"));
    assert_eq!(commands[0].headers.as_deref(), Some("X-Test: 1"));
}

#[test]
fn web_capture_command_rejects_reserved_flags() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let _ = b.page(PageSize::A4);
    b.add_web_capture_command(WebCaptureCommand::new("http://a/").with_flags(0b1000));
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("reserved flags");
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert!(out.is_empty());
}

#[test]
fn web_capture_requires_pdf_1_3() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_2);
    let page = b.page(PageSize::A4);
    b.add_web_capture_page_set(
        WebCapturePageSet::new(vec![0x01], [page])
            .with_source(WebCaptureSource::new("http://example.com/")),
    );
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("web capture needs 1.3");
    assert!(
        matches!(error, WriteError::UnsupportedVersion(_)),
        "{error}"
    );
    assert!(out.is_empty());
}

#[test]
fn web_capture_rejects_page_in_two_sets_and_out_of_range() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    b.add_web_capture_page_set(
        WebCapturePageSet::new(vec![0x01], [page]).with_source(WebCaptureSource::new("http://a/")),
    )
    .add_web_capture_page_set(
        WebCapturePageSet::new(vec![0x02], [page]).with_source(WebCaptureSource::new("http://b/")),
    );
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("page shared by two sets");
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert!(error.to_string().contains("more than one"), "{error}");
    assert!(out.is_empty());

    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let _ = b.page(PageSize::A4);
    b.add_web_capture_page_set(
        WebCapturePageSet::new(vec![0x01], [5]).with_source(WebCaptureSource::new("http://a/")),
    );
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("out-of-range page");
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert!(out.is_empty());
}
