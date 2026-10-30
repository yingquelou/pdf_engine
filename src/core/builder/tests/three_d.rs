//! 3D artwork 流（13.6 Table 300）与 3D 注解（Table 298）写端：流对象
//! 分配、`/3DD` 引用、视图选择、激活/交互/视图框条目、取值校验与
//! PDF 1.6 门禁。

use super::support::annot_object;
use super::*;
use crate::core::types::{Array, PdfFeature, Value};

fn camera() -> [f64; 12] {
    [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 30.0]
}

fn valid_artwork() -> ThreeDArtwork {
    ThreeDArtwork::new(vec![1, 2, 3, 4])
        .with_view(
            ThreeDView::new("Front")
                .with_internal_name("front")
                .with_matrix_source(ThreeDMatrixSource::M)
                .with_camera_to_world(camera()),
        )
        .with_default_view(ThreeDViewSelection::Index(0))
}

fn three_d_annotation(handle: usize) -> PdfAnnotation {
    PdfAnnotation::new(AnnotationKind::ThreeD, [72.0, 600.0, 200.0, 680.0]).with_three_d(handle)
}

/// 流字典、视图、激活、交互与视图框全部写出，特性画像含 3D 注解。
#[test]
fn three_d_annotation_writes_stream_views_and_activation() {
    let mut b = PdfBuilder::new();
    let p0 = b.page(PageSize::A4);
    let artwork = b.add_three_d_artwork(valid_artwork());
    assert_eq!(artwork, 0);
    b.add_annotation(
        p0,
        three_d_annotation(artwork)
            .with_three_d_view(ThreeDViewSelection::Index(0))
            .with_three_d_activation(
                ThreeDActivation::new()
                    .with_activation(ThreeDActivationPolicy::PageOpen)
                    .with_deactivation(ThreeDDeactivationPolicy::PageClose)
                    .with_toolbar(false)
                    .with_navigation_panel(true),
            )
            .with_three_d_interactive(false)
            .with_three_d_view_box([-10.0, -20.0, 10.0, 20.0]),
    );

    let doc = b.assemble_document().expect("assemble 3D annotation");
    let dict = annot_object(&doc, 3, 0);
    assert_eq!(dict.get("Subtype"), Some(&Value::Name("3D".into())));
    assert!(matches!(dict.get("3DD"), Some(Value::Ref(_, 0))));
    assert_eq!(dict.get("3DV"), Some(&Value::Integer(0)));
    assert_eq!(dict.get("3DI"), Some(&Value::Bool(false)));
    assert_eq!(
        dict.get("3DB").and_then(Value::as_array).map(Array::len),
        Some(4)
    );
    let activation = dict
        .get("3DA")
        .and_then(Value::as_dict)
        .expect("/3DA dictionary");
    assert_eq!(activation.get("A"), Some(&Value::Name("PO".into())));
    assert_eq!(activation.get("D"), Some(&Value::Name("PC".into())));
    assert_eq!(activation.get("TB"), Some(&Value::Bool(false)));
    assert_eq!(activation.get("NP"), Some(&Value::Bool(true)));
    assert!(
        doc.features
            .borrow()
            .contains(PdfFeature::ThreeDAnnotations)
    );

    let pdf = render(&mut b);
    for fragment in [
        "/Subtype /3D",
        "/Subtype /U3D",
        "/3DD",
        "/3DV",
        "/3DA",
        "/3DI",
        "/3DB",
    ] {
        assert!(pdf.contains(fragment), "missing {fragment}: {pdf}");
    }
}

/// 3D 自 PDF 1.6 引入；1.5 构建返回 UnsupportedVersion 且零字节。
#[test]
fn three_d_annotation_requires_pdf_1_6() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_5);
    let p0 = b.page(PageSize::A4);
    let artwork = b.add_three_d_artwork(valid_artwork());
    b.add_annotation(p0, three_d_annotation(artwork));

    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("version gate");
    assert!(
        matches!(error, WriteError::UnsupportedVersion(_)),
        "expected UnsupportedVersion, got {error}"
    );
    assert_eq!(out.len(), 0);
}

/// 未知句柄、越界/未匹配视图选择、矩阵依赖、负 `/CO`、非四项 `/3DB`
/// 与非 3D 子类型带 3D 条目均在写出前拒绝。
#[test]
fn invalid_three_d_values_are_rejected() {
    let cases: Vec<(ThreeDArtwork, PdfAnnotation)> = vec![
        (valid_artwork(), three_d_annotation(3)),
        (
            valid_artwork(),
            three_d_annotation(0).with_three_d_view(ThreeDViewSelection::Index(3)),
        ),
        (
            ThreeDArtwork::new(vec![1])
                .with_view(ThreeDView::new("Front").with_matrix_source(ThreeDMatrixSource::M)),
            three_d_annotation(0),
        ),
        (
            ThreeDArtwork::new(vec![1])
                .with_view(ThreeDView::new("Front").with_matrix_source(ThreeDMatrixSource::U3D)),
            three_d_annotation(0),
        ),
        (
            ThreeDArtwork::new(vec![1]).with_view(
                ThreeDView::new("Front")
                    .with_matrix_source(ThreeDMatrixSource::M)
                    .with_camera_to_world(camera())
                    .with_center_of_orbit(-1.0),
            ),
            three_d_annotation(0),
        ),
        (
            valid_artwork(),
            three_d_annotation(0).with_three_d_view_box(vec![0.0, 0.0, 10.0]),
        ),
        (
            valid_artwork(),
            PdfAnnotation::new(AnnotationKind::Text, [0.0, 0.0, 10.0, 10.0]).with_three_d(0),
        ),
    ];
    for (artwork, annot) in cases {
        let mut b = PdfBuilder::new();
        let p0 = b.page(PageSize::A4);
        b.add_three_d_artwork(artwork);
        b.add_annotation(p0, annot);
        let mut out = Vec::new();
        let error = b.build(&mut out).expect_err("invalid 3D values");
        assert!(
            matches!(error, WriteError::InvalidObject(_)),
            "expected InvalidObject, got {error}"
        );
        assert_eq!(out.len(), 0, "rejected build must not emit bytes");
    }
}
