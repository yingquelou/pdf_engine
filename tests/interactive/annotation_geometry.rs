//! E2 几何注解：Square/Circle/Polygon/PolyLine 专有条目（Table 177/178）
//! 的写出 → 解析往返。

use super::*;

/// 写入 → 重新解析：`/BE` `/RD` `/Vertices` `/LE` `/IT` 经页面 `/Annots`
/// 由读端注解模型完整提取，且各子类型只落在自己的载体字段上。
#[test]
fn builder_geometry_annotation_entries_roundtrip_through_parser() {
    use pdf_engine::core::builder::{
        AnnotationKind, BorderEffect, LineEndStyle as BuilderLineEndStyle, PageSize, PdfAnnotation,
        PdfBuilder, PolygonIntent as BuilderPolygonIntent,
    };
    use pdf_engine::core::reader::annotation::{
        AnnotationSubtype, BorderEffectStyle as ReaderBorderEffectStyle,
        LineEndStyle as ReaderLineEndStyle, PolygonIntent as ReaderPolygonIntent,
    };

    let mut builder = PdfBuilder::new();
    let p0 = builder.page(PageSize::A4);
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Square, [72.0, 640.0, 200.0, 700.0])
            .with_interior_color(vec![0.2, 0.4, 0.6])
            .with_border_effect(BorderEffect::cloudy(1.5))
            .with_text_rectangle([1.0, 2.0, 3.0, 4.0]),
    );
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Circle, [72.0, 560.0, 200.0, 620.0])
            .with_text_rectangle([0.0, 0.5, 0.0, 0.5]),
    );
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Polygon, [72.0, 480.0, 200.0, 540.0])
            .with_vertices(vec![10.0, 20.0, 30.5, 40.0, 50.0, 60.0])
            .with_polygon_intent(BuilderPolygonIntent::PolygonCloud)
            .with_border_effect(BorderEffect::cloudy(0.5)),
    );
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::PolyLine, [72.0, 400.0, 200.0, 460.0])
            .with_vertices(vec![0.0, 0.0, 10.0, 10.0])
            .with_line_endings(BuilderLineEndStyle::OpenArrow, BuilderLineEndStyle::Slash)
            .with_polygon_intent(BuilderPolygonIntent::PolyLineDimension),
    );
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");

    let doc = parse_buf(&buf);
    let annotations = doc.page_annotations(0).expect("annotations");
    assert_eq!(annotations.len(), 4);

    let square = &annotations[0];
    assert_eq!(square.subtype, AnnotationSubtype::Square);
    assert_eq!(square.interior_color, Some(vec![0.2, 0.4, 0.6]));
    let effect = square.border_effect.as_ref().expect("square /BE");
    assert_eq!(effect.style, ReaderBorderEffectStyle::Cloudy);
    assert_eq!(effect.intensity, Some(1.5));
    assert_eq!(square.text_rectangle, Some([1.0, 2.0, 3.0, 4.0]));
    assert!(square.vertices.is_none());
    assert!(square.polygon_intent.is_none());

    let circle = &annotations[1];
    assert_eq!(circle.subtype, AnnotationSubtype::Circle);
    assert_eq!(circle.text_rectangle, Some([0.0, 0.5, 0.0, 0.5]));
    assert!(circle.border_effect.is_none());
    assert!(circle.vertices.is_none());

    let polygon = &annotations[2];
    assert_eq!(polygon.subtype, AnnotationSubtype::Polygon);
    assert_eq!(
        polygon.vertices,
        Some(vec![10.0, 20.0, 30.5, 40.0, 50.0, 60.0])
    );
    assert_eq!(
        polygon.polygon_intent,
        Some(ReaderPolygonIntent::PolygonCloud)
    );
    assert_eq!(
        polygon
            .border_effect
            .as_ref()
            .map(|effect| effect.style.clone()),
        Some(ReaderBorderEffectStyle::Cloudy)
    );
    assert!(polygon.text_rectangle.is_none());

    let polyline = &annotations[3];
    assert_eq!(polyline.subtype, AnnotationSubtype::PolyLine);
    assert_eq!(polyline.vertices, Some(vec![0.0, 0.0, 10.0, 10.0]));
    assert_eq!(
        polyline.line_endings,
        Some([ReaderLineEndStyle::OpenArrow, ReaderLineEndStyle::Slash])
    );
    assert_eq!(
        polyline.polygon_intent,
        Some(ReaderPolygonIntent::PolyLineDimension)
    );
    // 数组 `/LE` 不落入 FreeText 的单名字字段。
    assert!(polyline.callout_ending.is_none());
    assert!(polyline.border_effect.is_none());
}
