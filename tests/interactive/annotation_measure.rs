//! E2 注解 `/Measure`：measure 字典（12.9，Table 261–263）的
//! 写出 → 解析 → 回读往返。

use super::*;

/// 写入 → 重新解析：Line 与 Polygon 的 `/Measure` 经页面 `/Annots`
/// 由读端 measure 模型完整提取（直角坐标条目 + 数字格式数组）。
#[test]
fn builder_measure_annotation_roundtrips_through_parser() {
    use pdf_engine::core::builder::{
        AnnotationKind, Measure, NumberFormat, NumberFraction, NumberLabelOrder, PageSize,
        PdfAnnotation, PdfBuilder,
    };
    use pdf_engine::core::reader::annotation::{
        AnnotationSubtype, NumberFraction as ReaderNumberFraction,
        NumberLabelOrder as ReaderNumberLabelOrder,
    };

    let mut builder = PdfBuilder::new();
    let p0 = builder.page(PageSize::A4);
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Line, [72.0, 600.0, 400.0, 700.0])
            .with_line_endpoints([72.0, 700.0, 400.0, 600.0])
            .with_measure(
                Measure::new(
                    "1 in = 1 ft",
                    vec![
                        NumberFormat::new("in", 72.0)
                            .with_precision(100)
                            .with_label_order(NumberLabelOrder::Prefix),
                    ],
                    vec![
                        NumberFormat::new("ft", 12.0)
                            .with_fraction(NumberFraction::Fraction)
                            .with_precision(16)
                            .with_force_denominator(true),
                        NumberFormat::new("in", 1.0),
                    ],
                    vec![NumberFormat::new("sq ft", 144.0)],
                )
                .with_y(vec![NumberFormat::new("cm", 2.54)])
                .with_origin([10.0, 20.5])
                .with_conversion_yx(1.0),
            ),
    );
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Polygon, [72.0, 400.0, 300.0, 500.0])
            .with_vertices(vec![72.0, 400.0, 300.0, 420.0, 180.0, 500.0])
            .with_measure(Measure::new(
                "1:1",
                vec![NumberFormat::new("pt", 1.0)],
                vec![NumberFormat::new("pt", 1.0)],
                vec![NumberFormat::new("sq pt", 1.0)],
            )),
    );
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");

    let doc = parse_buf(&buf);
    let annotations = doc.page_annotations(0).expect("annotations");
    assert_eq!(annotations.len(), 2);

    let line = &annotations[0];
    assert_eq!(line.subtype, AnnotationSubtype::Line);
    let measure = line.measure.as_ref().expect("line /Measure");
    assert_eq!(measure.subtype.as_deref(), Some("RL"));
    assert_eq!(measure.ratio.as_deref(), Some("1 in = 1 ft"));
    let x = measure.x.as_ref().expect("/X");
    assert_eq!(x.len(), 1);
    assert_eq!(x[0].unit.as_deref(), Some("in"));
    assert_eq!(x[0].conversion, Some(72.0));
    assert_eq!(x[0].precision, Some(100));
    assert_eq!(x[0].label_order, Some(ReaderNumberLabelOrder::Prefix));
    let distance = measure.distance.as_ref().expect("/D");
    assert_eq!(distance.len(), 2);
    assert_eq!(distance[0].unit.as_deref(), Some("ft"));
    assert_eq!(distance[0].fraction, Some(ReaderNumberFraction::Fraction));
    assert_eq!(distance[0].precision, Some(16));
    assert_eq!(distance[0].force_denominator, Some(true));
    assert_eq!(distance[1].unit.as_deref(), Some("in"));
    assert_eq!(
        measure.area.as_ref().map(|area| area[0].unit.as_deref()),
        Some(Some("sq ft"))
    );
    assert_eq!(
        measure.y.as_ref().map(|y| y[0].unit.as_deref()),
        Some(Some("cm"))
    );
    assert_eq!(measure.origin, Some([10.0, 20.5]));
    assert_eq!(measure.conversion_yx, Some(1.0));
    assert!(measure.angle.is_none());
    assert!(measure.slope.is_none());

    let polygon = &annotations[1];
    assert_eq!(polygon.subtype, AnnotationSubtype::Polygon);
    let measure = polygon.measure.as_ref().expect("polygon /Measure");
    assert_eq!(measure.ratio.as_deref(), Some("1:1"));
    assert_eq!(
        measure.distance.as_ref().map(|d| d[0].unit.as_deref()),
        Some(Some("pt"))
    );
    assert!(measure.y.is_none());
    assert!(measure.origin.is_none());
    assert!(measure.conversion_yx.is_none());
}
