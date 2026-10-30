//! 注解 `/ExData` 3D 标记（13.6.6，Table 170/313）写端测试：
//! `Markup3D` 的 `/3DA` `/3DV` `/MD5` 序列化、目标跨注解校验与 PDF 1.7 门禁。

use super::support::{annot_object, annot_object_with_num};
use super::*;
use crate::core::types::{PdfFeature, Value};

/// 最小 3D artwork 的注册句柄所对应的注解（Table 298）。
fn three_d_annotation(artwork: usize) -> PdfAnnotation {
    PdfAnnotation::new(AnnotationKind::ThreeD, [72.0, 600.0, 200.0, 680.0]).with_three_d(artwork)
}

/// FreeText 标记注解（Table 174 的 `/DA` 必填），挂上 `/ExData`。
fn markup_annotation(target: usize, view: ThreeDView, md5: Option<[u8; 16]>) -> PdfAnnotation {
    let mut markup = Markup3D::new(target, view);
    if let Some(md5) = md5 {
        markup = markup.with_md5(md5);
    }
    PdfAnnotation::new(AnnotationKind::FreeText, [72.0, 400.0, 200.0, 500.0])
        .with_default_appearance("/F1 12 Tf 0 g")
        .with_markup_3d(markup)
}

/// 追加注解并完整写出；失败时返回错误（用于零字节契约断言）。
fn build_with(version: PdfVersion, annotations: Vec<PdfAnnotation>) -> Result<Vec<u8>, WriteError> {
    let mut b = PdfBuilder::with_version(version);
    let p0 = b.page(PageSize::A4);
    for annot in annotations {
        b.add_annotation(p0, annot);
    }
    let mut out = Vec::new();
    b.build(&mut out)?;
    Ok(out)
}

/// `/ExData` 按 Table 313 写出：`/3DA` 指向目标 3D 注解对象号、`/3DV`
/// 为内联视图字典（文本串十六进制编码）、`/MD5` 为大写十六进制。
#[test]
fn markup_3d_exdata_serializes_typed_entries() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let p0 = b.page(PageSize::A4);
    let artwork = b.add_three_d_artwork(ThreeDArtwork::new(vec![0x55, 0x33, 0x44, 0x00]));
    b.add_annotation(p0, three_d_annotation(artwork));
    b.add_annotation(
        p0,
        markup_annotation(
            artwork,
            ThreeDView::new("CommentView1").with_internal_name("comment"),
            Some([0xAB; 16]),
        ),
    );

    let doc = b.assemble_document().expect("assemble /ExData");
    let (target_num, target) = annot_object_with_num(&doc, 3, 0);
    assert_eq!(target.get("Subtype"), Some(&Value::Name("3D".into())));

    let dict = annot_object(&doc, 3, 1);
    let ex_data = dict
        .get("ExData")
        .and_then(|value| value.as_dict())
        .expect("/ExData dictionary");
    assert_eq!(ex_data.get("Type"), Some(&Value::Name("ExData".into())));
    assert_eq!(
        ex_data.get("Subtype"),
        Some(&Value::Name("Markup3D".into()))
    );
    assert_eq!(ex_data.get("3DA"), Some(&Value::Ref(target_num, 0)));

    // `/3DV` 内联直写；文本串统一按 UTF-16BE/PDFDocEncoding 大写十六进制。
    let view = ex_data
        .get("3DV")
        .and_then(|value| value.as_dict())
        .expect("/3DV inline view");
    assert_eq!(view.get("Type"), Some(&Value::Name("3DView".into())));
    assert_eq!(
        view.get("XN"),
        Some(&Value::HexString("436F6D6D656E745669657731".into()))
    );
    assert_eq!(
        view.get("IN"),
        Some(&Value::HexString("636F6D6D656E74".into()))
    );
    assert_eq!(
        ex_data.get("MD5"),
        Some(&Value::HexString("ABABABABABABABABABABABABABABABAB".into()))
    );

    assert!(
        doc.features.borrow().contains(PdfFeature::ThreeDMarkup),
        "feature profile must record /ExData Markup3D"
    );
}

/// `/ExData` 只能挂在标记类注解上，`/3DA` 必须是另一条 3D 注解；
/// 违规在写出任何字节前以 `InvalidObject` 拒绝。
#[test]
fn markup_3d_requires_markup_annotation_and_3d_target() {
    // 非标记类注解（Link）携带 `/ExData`。
    {
        let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
        let p0 = b.page(PageSize::A4);
        let artwork = b.add_three_d_artwork(ThreeDArtwork::new(vec![0x55, 0x33, 0x44, 0x00]));
        b.add_annotation(p0, three_d_annotation(artwork));
        b.add_annotation(
            p0,
            PdfAnnotation::new(AnnotationKind::Link, [0.0, 0.0, 10.0, 10.0])
                .with_markup_3d(Markup3D::new(artwork, ThreeDView::new("CommentView1"))),
        );
        let mut out = Vec::new();
        let error = b.build(&mut out).expect_err("non-markup annotation");
        assert!(
            matches!(&error, WriteError::InvalidObject(message)
                if message.contains("/ExData is only permitted on markup annotations")
                    && message.contains("Table 170")),
            "{error}"
        );
        assert_eq!(out.len(), 0, "validation must precede any bytes");
    }

    // `/3DA` 指向非 3D 注解。
    {
        let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
        let p0 = b.page(PageSize::A4);
        b.add_annotation(
            p0,
            PdfAnnotation::new(AnnotationKind::Square, [0.0, 0.0, 10.0, 10.0]),
        );
        b.add_annotation(
            p0,
            markup_annotation(0, ThreeDView::new("CommentView1"), None),
        );
        let mut out = Vec::new();
        let error = b.build(&mut out).expect_err("/3DA target is not 3D");
        assert!(
            matches!(&error, WriteError::InvalidObject(message)
                if message.contains("/3DA must reference a 3D annotation")
                    && message.contains("Square")),
            "{error}"
        );
        assert_eq!(out.len(), 0, "validation must precede any bytes");
    }

    // 未注册的目标句柄。
    let error = build_with(
        PdfVersion::V1_7,
        vec![markup_annotation(99, ThreeDView::new("CommentView1"), None)],
    )
    .expect_err("unknown /3DA handle");
    assert!(
        matches!(&error, WriteError::InvalidObject(message)
            if message.contains("/3DA references unknown annotation handle 99")),
        "{error}"
    );

    // 自引用同样拒绝（Table 313 要求目标为另一条 3D 注解）。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::FreeText, [0.0, 0.0, 10.0, 10.0])
            .with_default_appearance("/F1 12 Tf 0 g")
            .with_markup_3d(Markup3D::new(0, ThreeDView::new("CommentView1"))),
    );
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("/3DA self reference");
    assert!(
        matches!(&error, WriteError::InvalidObject(message)
            if message.contains("/3DA must not reference the annotation itself")),
        "{error}"
    );
    assert_eq!(out.len(), 0, "validation must precede any bytes");
}

/// `/ExData` 是 PDF 1.7 条目：1.6 目标在写出前拒绝。
#[test]
fn markup_3d_requires_pdf_1_7() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_6);
    let p0 = b.page(PageSize::A4);
    let artwork = b.add_three_d_artwork(ThreeDArtwork::new(vec![0x55, 0x33, 0x44, 0x00]));
    b.add_annotation(p0, three_d_annotation(artwork));
    b.add_annotation(
        p0,
        markup_annotation(artwork, ThreeDView::new("CommentView1"), Some([0xAB; 16])),
    );

    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("PDF 1.6 rejects /ExData");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(message)
            if message.contains("3D markup annotations") && message.contains("1.7")),
        "{error}"
    );
    assert_eq!(out.len(), 0, "version gate must precede any bytes");
}
