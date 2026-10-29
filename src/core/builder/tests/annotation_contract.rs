//! 注解写端契约：文本标记 `/QuadPoints` 必填（12.5.6.10，Table 179）
//! 与 Popup 不得带外观流或关联动作（12.5.6.14）。

use super::*;
use crate::core::content::ContentStream;

/// 把单条注解挂到一页文档并写出。
fn build_one(annot: PdfAnnotation, version: PdfVersion) -> Result<Vec<u8>, WriteError> {
    let mut b = PdfBuilder::with_version(version);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(p0, annot);
    let mut out = Vec::new();
    b.build(&mut out)?;
    Ok(out)
}

#[test]
fn text_markup_annotations_require_quad_points() {
    for kind in [
        AnnotationKind::Highlight,
        AnnotationKind::Underline,
        AnnotationKind::Squiggly,
        AnnotationKind::StrikeOut,
    ] {
        let error = build_one(
            PdfAnnotation::new(kind, [0.0, 0.0, 10.0, 10.0]),
            PdfVersion::V1_7,
        )
        .expect_err("text markup without /QuadPoints");
        let message = error.to_string();
        assert!(matches!(error, WriteError::InvalidObject(_)), "{message}");
        assert!(message.contains("requires /QuadPoints"), "{message}");
    }

    // 空数组同样视为缺失。
    let error = build_one(
        PdfAnnotation::new(AnnotationKind::Highlight, [0.0, 0.0, 10.0, 10.0])
            .with_quad_points(Vec::new()),
        PdfVersion::V1_7,
    )
    .expect_err("empty /QuadPoints");
    assert!(
        error.to_string().contains("requires /QuadPoints"),
        "{error}"
    );

    // 补齐 `/QuadPoints` 后正常写出。
    build_one(
        PdfAnnotation::new(AnnotationKind::Highlight, [0.0, 0.0, 10.0, 10.0])
            .with_quad_points(vec![0.0, 0.0, 10.0, 0.0, 0.0, 10.0, 10.0, 10.0]),
        PdfVersion::V1_7,
    )
    .expect("complete text markup");
}

#[test]
fn popup_annotation_rejects_appearance_stream() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let p0 = b.page(PageSize::A4);
    let form = b.form_xobject(&FormXObject::new(
        [0.0, 0.0, 10.0, 10.0],
        ContentStream::new(),
    ));
    b.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Popup, [0.0, 0.0, 10.0, 10.0]).with_appearance(form),
    );
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("popup /AP");
    let message = error.to_string();
    assert!(matches!(error, WriteError::InvalidObject(_)), "{message}");
    assert!(
        message.contains("shall not have an appearance stream"),
        "{message}"
    );
    assert_eq!(out.len(), 0);
}

#[test]
fn popup_annotation_rejects_associated_actions() {
    let error = build_one(
        PdfAnnotation::new(AnnotationKind::Popup, [0.0, 0.0, 10.0, 10.0])
            .with_action(Action::Uri("https://example.com".into())),
        PdfVersion::V1_7,
    )
    .expect_err("popup /A");
    let message = error.to_string();
    assert!(matches!(error, WriteError::InvalidObject(_)), "{message}");
    assert!(
        message.contains("shall not have associated actions"),
        "{message}"
    );
}
