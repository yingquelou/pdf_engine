//! Link 注解专有条目写端测试（ISO 32000-1 12.5.6.5，Table 173）：
//! `/H` 高亮模式与 `/PA` 按下 URI 动作的写出、归属校验、URI 约束与
//! 逐条目版本门禁。

use super::support::annot_object;
use super::*;
use crate::core::reader::content::text_value;
use crate::core::types::PdfFeature;

/// 带 `/H` 与 `/PA` 的 Link 注解，供写出与门禁测试复用。
fn full_link_annotation() -> PdfAnnotation {
    PdfAnnotation::new(AnnotationKind::Link, [72.0, 700.0, 300.0, 720.0])
        .with_link_highlight(LinkHighlightMode::Push)
        .with_pressed_action(Action::Uri("https://example.com/legacy".into()))
}

/// 把注解挂到单页文档并写出。
fn build_one(annot: PdfAnnotation, version: PdfVersion) -> Result<Vec<u8>, WriteError> {
    let mut b = PdfBuilder::with_version(version);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(p0, annot);
    let mut out = Vec::new();
    b.build(&mut out)?;
    Ok(out)
}

#[test]
fn link_annotation_writes_highlight_mode_and_pressed_action() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_3);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(p0, full_link_annotation());
    let doc = b.assemble_document().expect("assemble");
    let dict = annot_object(&doc, 3, 0);

    assert_eq!(dict.get("Subtype"), Some(&Value::Name("Link".into())));
    assert_eq!(dict.get("H"), Some(&Value::Name("P".into())));
    let pressed = dict.get("PA").and_then(|v| v.as_dict()).expect("/PA");
    assert_eq!(pressed.get("S"), Some(&Value::Name("URI".into())));
    assert_eq!(
        pressed.get("URI").and_then(text_value).as_deref(),
        Some("https://example.com/legacy")
    );
    assert!(dict.get("A").is_none(), "未设置 /A 时不写");

    let features = doc.features.borrow();
    for feature in [
        PdfFeature::LinkAnnotationHighlighting,
        PdfFeature::LinkAnnotationPressedAction,
    ] {
        assert!(features.contains(feature), "{feature} must be recorded");
    }
}

#[test]
fn minimal_link_annotation_writes_no_link_entries() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_4);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Link, [0.0, 0.0, 10.0, 10.0]),
    );
    let doc = b.assemble_document().expect("assemble");
    let dict = annot_object(&doc, 3, 0);
    assert!(dict.get("H").is_none(), "/H 缺省 Invert，不写");
    assert!(dict.get("PA").is_none());
    assert!(
        !doc.features
            .borrow()
            .contains(PdfFeature::LinkAnnotationHighlighting)
    );
}

#[test]
fn link_entries_are_rejected_on_other_subtypes() {
    let cases = vec![
        PdfAnnotation::new(AnnotationKind::Text, [0.0, 0.0, 10.0, 10.0])
            .with_link_highlight(LinkHighlightMode::Invert),
        PdfAnnotation::new(AnnotationKind::Text, [0.0, 0.0, 10.0, 10.0])
            .with_pressed_action(Action::Uri("https://example.com".into())),
        PdfAnnotation::new(AnnotationKind::Square, [0.0, 0.0, 10.0, 10.0])
            .with_link_highlight(LinkHighlightMode::Outline),
    ];
    for annot in cases {
        let error =
            build_one(annot, PdfVersion::V1_4).expect_err("link entries on a foreign subtype");
        let message = error.to_string();
        assert!(matches!(error, WriteError::InvalidObject(_)), "{message}");
        assert!(message.contains("specific to Link"), "{message}");
    }
}

#[test]
fn pressed_action_must_be_uri_action() {
    for action in [
        Action::Named(ActionNamedAction::NextPage),
        Action::JavaScript("app.alert('x')".into()),
    ] {
        let annot = PdfAnnotation::new(AnnotationKind::Link, [0.0, 0.0, 10.0, 10.0])
            .with_pressed_action(action);
        let error = build_one(annot, PdfVersion::V1_4).expect_err("non-URI /PA");
        let message = error.to_string();
        assert!(matches!(error, WriteError::InvalidObject(_)), "{message}");
        assert!(message.contains("/PA must be a URI action"), "{message}");
    }
}

#[test]
fn link_highlight_requires_pdf_1_2() {
    let annot = || {
        PdfAnnotation::new(AnnotationKind::Link, [0.0, 0.0, 10.0, 10.0])
            .with_link_highlight(LinkHighlightMode::Outline)
    };
    let error = build_one(annot(), PdfVersion::V1_1).expect_err("1.1 rejects /H");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(message)
            if message.contains("highlighting") && message.contains("1.2")),
        "{error}"
    );
    build_one(annot(), PdfVersion::V1_2).expect("/H at 1.2");
}

#[test]
fn link_pressed_action_requires_pdf_1_3() {
    let annot = || {
        PdfAnnotation::new(AnnotationKind::Link, [0.0, 0.0, 10.0, 10.0])
            .with_pressed_action(Action::Uri("https://example.com".into()))
    };
    let error = build_one(annot(), PdfVersion::V1_2).expect_err("1.2 rejects /PA");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(message)
            if message.contains("pressed action") && message.contains("1.3")),
        "{error}"
    );
    build_one(annot(), PdfVersion::V1_3).expect("/PA at 1.3");
}

#[test]
fn rejected_link_version_writes_no_bytes() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_1);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(p0, full_link_annotation());
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("1.1 must reject /H /PA");
    assert!(
        matches!(error, WriteError::UnsupportedVersion(_)),
        "{error}"
    );
    assert_eq!(out.len(), 0);
}
