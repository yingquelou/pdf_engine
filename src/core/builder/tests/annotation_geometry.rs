//! Square/Circle/Polygon/PolyLine 专有条目写端测试
//! （ISO 32000-1 12.5.6.8–12.5.6.9，Table 177/178）：
//! `/Vertices` `/IT` 的写出与取值校验、`/BE` `/RD` `/LE` 的跨子类型归属、
//! 逐条目版本门禁。

use super::support::annot_object;
use super::*;
use crate::core::types::PdfFeature;

/// 带全部几何专有条目的 Polygon 注解，供写出与门禁测试复用。
fn full_polygon_annotation() -> PdfAnnotation {
    PdfAnnotation::new(AnnotationKind::Polygon, [72.0, 700.0, 300.0, 640.0])
        .with_vertices(vec![72.0, 640.0, 300.0, 700.0, 180.0, 760.0])
        .with_polygon_intent(PolygonIntent::PolygonCloud)
        .with_border_effect(BorderEffect::cloudy(1.5))
        .with_interior_color(vec![0.9, 0.9, 0.6])
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
fn square_annotation_writes_border_effect_and_rect_differences() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_5);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Square, [10.0, 10.0, 110.0, 60.0])
            .with_border_effect(BorderEffect::cloudy(2.0))
            .with_text_rectangle([1.0, 2.0, 3.0, 4.0])
            .with_interior_color(vec![0.5]),
    );
    let doc = b.assemble_document().expect("assemble");
    let dict = annot_object(&doc, 3, 0);

    assert_eq!(dict.get("Subtype"), Some(&Value::Name("Square".into())));
    let effect = dict.get("BE").and_then(|v| v.as_dict()).expect("/BE");
    assert_eq!(effect.get("S"), Some(&Value::Name("C".into())));
    assert_eq!(effect.get("I"), Some(&Value::Float(2.0)));
    let diff = dict.get("RD").and_then(|v| v.as_array()).expect("/RD");
    assert_eq!(
        diff.0,
        vec![
            Value::Float(1.0),
            Value::Float(2.0),
            Value::Float(3.0),
            Value::Float(4.0)
        ]
    );
    let ic = dict.get("IC").and_then(|v| v.as_array()).expect("/IC");
    assert_eq!(ic.0, vec![Value::Float(0.5)]);
    assert!(dict.get("Vertices").is_none(), "Square 不写 /Vertices");
    assert!(dict.get("IT").is_none(), "Square 不写 /IT");

    let features = doc.features.borrow();
    for feature in [
        PdfFeature::AnnotationBorderEffect,
        PdfFeature::AnnotationRectDifferences,
    ] {
        assert!(features.contains(feature), "{feature} must be recorded");
    }
    assert!(!features.contains(PdfFeature::PolygonCloudIntent));
}

#[test]
fn polygon_annotation_writes_vertices_and_cloud_intent() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_6);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(p0, full_polygon_annotation());
    let doc = b.assemble_document().expect("assemble");
    let dict = annot_object(&doc, 3, 0);

    assert_eq!(dict.get("Subtype"), Some(&Value::Name("Polygon".into())));
    let vertices = dict.get("Vertices").and_then(|v| v.as_array()).expect("/V");
    assert_eq!(
        vertices.0,
        vec![
            Value::Float(72.0),
            Value::Float(640.0),
            Value::Float(300.0),
            Value::Float(700.0),
            Value::Float(180.0),
            Value::Float(760.0)
        ]
    );
    assert_eq!(dict.get("IT"), Some(&Value::Name("PolygonCloud".into())));
    let effect = dict.get("BE").and_then(|v| v.as_dict()).expect("/BE");
    assert_eq!(effect.get("S"), Some(&Value::Name("C".into())));

    let features = doc.features.borrow();
    for feature in [
        PdfFeature::PolygonCloudIntent,
        PdfFeature::AnnotationBorderEffect,
    ] {
        assert!(features.contains(feature), "{feature} must be recorded");
    }
    // Polygon 没有 `/RD`（Table 178 不含该条目）。
    assert!(!features.contains(PdfFeature::AnnotationRectDifferences));
}

#[test]
fn polyline_annotation_writes_vertices_and_line_endings() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::PolyLine, [0.0, 0.0, 100.0, 100.0])
            .with_vertices(vec![0.0, 0.0, 50.0, 80.0, 100.0, 100.0])
            .with_line_endings(LineEndStyle::OpenArrow, LineEndStyle::Slash)
            .with_polygon_intent(PolygonIntent::PolyLineDimension),
    );
    let doc = b.assemble_document().expect("assemble");
    let dict = annot_object(&doc, 3, 0);

    assert_eq!(dict.get("Subtype"), Some(&Value::Name("PolyLine".into())));
    let endings = dict.get("LE").and_then(|v| v.as_array()).expect("/LE");
    assert_eq!(
        endings.0,
        vec![Value::Name("OpenArrow".into()), Value::Name("Slash".into())]
    );
    assert_eq!(
        dict.get("IT"),
        Some(&Value::Name("PolyLineDimension".into()))
    );
    assert!(dict.get("L").is_none(), "PolyLine 不写 /L");

    let features = doc.features.borrow();
    assert!(features.contains(PdfFeature::PolygonDimensionIntent));
}

#[test]
fn geometry_entries_are_rejected_on_other_subtypes() {
    let cases = vec![
        PdfAnnotation::new(AnnotationKind::Square, [0.0, 0.0, 10.0, 10.0])
            .with_vertices(vec![0.0, 0.0, 5.0, 5.0]),
        PdfAnnotation::new(AnnotationKind::Line, [0.0, 0.0, 10.0, 10.0])
            .with_line_endpoints([0.0, 0.0, 10.0, 10.0])
            .with_polygon_intent(PolygonIntent::PolygonCloud),
        // `/RD` 不在 Table 178 中，Polygon 不得写出。
        PdfAnnotation::new(AnnotationKind::Polygon, [0.0, 0.0, 10.0, 10.0])
            .with_vertices(vec![0.0, 0.0, 5.0, 5.0])
            .with_text_rectangle([0.0, 0.0, 0.0, 0.0]),
        // `/BE` 对 PolyLine 无意义（Table 178 限定 polygon）。
        PdfAnnotation::new(AnnotationKind::PolyLine, [0.0, 0.0, 10.0, 10.0])
            .with_vertices(vec![0.0, 0.0, 5.0, 5.0])
            .with_border_effect(BorderEffect::new(BorderEffectStyle::Cloudy)),
        // `/LE` 仅 Line 与 PolyLine 有意义，Polygon/Square 不得写。
        PdfAnnotation::new(AnnotationKind::Polygon, [0.0, 0.0, 10.0, 10.0])
            .with_vertices(vec![0.0, 0.0, 5.0, 5.0])
            .with_line_endings(LineEndStyle::OpenArrow, LineEndStyle::None),
        PdfAnnotation::new(AnnotationKind::Square, [0.0, 0.0, 10.0, 10.0])
            .with_line_endings(LineEndStyle::OpenArrow, LineEndStyle::None),
    ];
    for annot in cases {
        let error =
            build_one(annot, PdfVersion::V1_7).expect_err("geometry entries on a foreign subtype");
        let message = error.to_string();
        assert!(matches!(error, WriteError::InvalidObject(_)), "{message}");
        assert!(message.contains("specific to"), "{message}");
    }
}

#[test]
fn polygon_and_polyline_require_vertices() {
    for kind in [AnnotationKind::Polygon, AnnotationKind::PolyLine] {
        let error = build_one(
            PdfAnnotation::new(kind, [0.0, 0.0, 10.0, 10.0]),
            PdfVersion::V1_7,
        )
        .expect_err("missing /Vertices");
        let message = error.to_string();
        assert!(matches!(error, WriteError::InvalidObject(_)), "{message}");
        assert!(message.contains("requires /Vertices"), "{message}");
    }
}

#[test]
fn invalid_geometry_entry_values_are_rejected() {
    let base = || PdfAnnotation::new(AnnotationKind::Polygon, [0.0, 0.0, 10.0, 10.0]);
    let cases = vec![
        // 顶点数是奇数
        base().with_vertices(vec![0.0, 0.0, 5.0]),
        // 少于两个顶点
        base().with_vertices(vec![0.0, 0.0]),
        // 非有限数
        base().with_vertices(vec![0.0, 0.0, f64::NAN, 5.0]),
        // 超过读端上限
        base().with_vertices(vec![0.0; 8194]),
        // 意图与子类型不匹配
        base()
            .with_vertices(vec![0.0, 0.0, 5.0, 5.0])
            .with_polygon_intent(PolygonIntent::PolyLineDimension),
    ];
    for annot in cases {
        let error = build_one(annot, PdfVersion::V1_7).expect_err("invalid geometry entry value");
        assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    }

    // PolyLine 不接受 PolygonCloud / PolygonDimension。
    for intent in [PolygonIntent::PolygonCloud, PolygonIntent::PolygonDimension] {
        let error = build_one(
            PdfAnnotation::new(AnnotationKind::PolyLine, [0.0, 0.0, 10.0, 10.0])
                .with_vertices(vec![0.0, 0.0, 5.0, 5.0])
                .with_polygon_intent(intent),
            PdfVersion::V1_7,
        )
        .expect_err("polyline intent mismatch");
        assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    }
}

#[test]
fn rect_differences_must_fit_inside_rect() {
    let square = || PdfAnnotation::new(AnnotationKind::Square, [0.0, 0.0, 10.0, 10.0]);
    // 四边差值之和须小于对应边长（Table 174/177）。
    for diff in [
        [5.0, 0.0, 5.0, 0.0],
        [0.0, 5.0, 0.0, 5.0],
        [6.0, 0.0, 4.0, 0.0],
    ] {
        let error = build_one(square().with_text_rectangle(diff), PdfVersion::V1_5)
            .expect_err("/RD must leave an inner rectangle");
        assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    }
    let free_text = || {
        PdfAnnotation::new(AnnotationKind::FreeText, [10.0, 10.0, 20.0, 20.0])
            .with_default_appearance("/F1 12 Tf")
    };
    let error = build_one(
        free_text().with_text_rectangle([0.0, 6.0, 0.0, 4.0]),
        PdfVersion::V1_6,
    )
    .expect_err("FreeText /RD must fit inside /Rect");
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    build_one(
        square().with_text_rectangle([4.9, 4.9, 4.9, 4.9]),
        PdfVersion::V1_5,
    )
    .expect("/RD leaves a positive inner rectangle");
}

#[test]
fn geometry_border_entries_require_pdf_1_5() {
    let square = || PdfAnnotation::new(AnnotationKind::Square, [0.0, 0.0, 10.0, 10.0]);
    for annot in [
        square().with_border_effect(BorderEffect::cloudy(1.0)),
        square().with_text_rectangle([0.0, 0.0, 0.0, 0.0]),
    ] {
        let error = build_one(annot, PdfVersion::V1_4).expect_err("1.4 rejects /BE /RD");
        assert!(
            matches!(&error, WriteError::UnsupportedVersion(message)
                if message.contains("border effect") || message.contains("rect differences")),
            "{error}"
        );
    }
    build_one(
        square().with_border_effect(BorderEffect::cloudy(1.0)),
        PdfVersion::V1_5,
    )
    .expect("/BE at 1.5");

    // 1.4 仍可写裸 Polygon（无 `/BE`），证明门禁只看条目不看子类型。
    build_one(
        PdfAnnotation::new(AnnotationKind::Polygon, [0.0, 0.0, 10.0, 10.0])
            .with_vertices(vec![0.0, 0.0, 5.0, 5.0]),
        PdfVersion::V1_5,
    )
    .expect("polygon at 1.5");
}

#[test]
fn polygon_intent_requires_pdf_1_6_and_1_7() {
    let polygon = || {
        PdfAnnotation::new(AnnotationKind::Polygon, [0.0, 0.0, 10.0, 10.0])
            .with_vertices(vec![0.0, 0.0, 5.0, 5.0])
    };
    let error = build_one(
        polygon().with_polygon_intent(PolygonIntent::PolygonCloud),
        PdfVersion::V1_5,
    )
    .expect_err("1.5 rejects /PolygonCloud");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(message)
            if message.contains("polygon cloud") && message.contains("1.6")),
        "{error}"
    );
    build_one(
        polygon().with_polygon_intent(PolygonIntent::PolygonCloud),
        PdfVersion::V1_6,
    )
    .expect("/PolygonCloud at 1.6");

    let error = build_one(
        polygon().with_polygon_intent(PolygonIntent::PolygonDimension),
        PdfVersion::V1_6,
    )
    .expect_err("1.6 rejects /PolygonDimension");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(message)
            if message.contains("dimension intent") && message.contains("1.7")),
        "{error}"
    );
    build_one(
        PdfAnnotation::new(AnnotationKind::PolyLine, [0.0, 0.0, 10.0, 10.0])
            .with_vertices(vec![0.0, 0.0, 5.0, 5.0])
            .with_polygon_intent(PolygonIntent::PolyLineDimension),
        PdfVersion::V1_7,
    )
    .expect("/PolyLineDimension at 1.7");
}

#[test]
fn polyline_end_style_gates_follow_table_176() {
    let annot = || {
        PdfAnnotation::new(AnnotationKind::PolyLine, [0.0, 0.0, 10.0, 10.0])
            .with_vertices(vec![0.0, 0.0, 5.0, 5.0])
    };
    // `Slash` 是 PDF 1.6 端点样式：1.5 的 PolyLine 不得写出。
    let error = build_one(
        annot().with_line_endings(LineEndStyle::OpenArrow, LineEndStyle::Slash),
        PdfVersion::V1_5,
    )
    .expect_err("1.5 rejects /Slash");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(message)
            if message.contains("Slash") && message.contains("1.6")),
        "{error}"
    );
    // `Butt` 是 PDF 1.5 样式，与 PolyLine 引入版本一致。
    build_one(
        annot().with_line_endings(LineEndStyle::Butt, LineEndStyle::None),
        PdfVersion::V1_5,
    )
    .expect("/Butt at 1.5");
}

#[test]
fn geometry_entries_record_their_features() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Circle, [0.0, 0.0, 10.0, 10.0])
            .with_border_effect(BorderEffect::cloudy(1.0))
            .with_text_rectangle([0.5, 0.5, 0.5, 0.5]),
    );
    b.add_annotation(p0, full_polygon_annotation());
    let doc = b.assemble_document().expect("assemble");
    let features = doc.features.borrow();
    for feature in [
        PdfFeature::AnnotationBorderEffect,
        PdfFeature::AnnotationRectDifferences,
        PdfFeature::PolygonCloudIntent,
        PdfFeature::MarkupAnnotations,
    ] {
        assert!(features.contains(feature), "{feature} must be recorded");
    }
}

#[test]
fn rejected_geometry_version_writes_no_bytes() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_5);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(p0, full_polygon_annotation());
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("1.5 must reject /PolygonCloud");
    assert!(
        matches!(error, WriteError::UnsupportedVersion(_)),
        "{error}"
    );
    assert_eq!(out.len(), 0);
}
