//! E2 文本标记与图章注解（ISO 32000-1 12.5.6.10/12.5.6.12，Table 179/181）：
//! Highlight/Underline/Squiggly/StrikeOut 的 `/QuadPoints` 分组与 Stamp
//! `/Name` 标准/未知图章名的写出 → 解析 → 读端类型化条目一致性往返。

use super::*;

#[test]
fn builder_highlight_quad_points_and_stamp_icon_roundtrip_through_parser() {
    use pdf_engine::core::builder::{
        AnnotationKind, PageSize, PdfAnnotation, PdfBuilder, StampIcon,
    };
    use pdf_engine::core::reader::annotation::{AnnotationSubtype, StampIcon as ReaderStampIcon};

    let mut builder = PdfBuilder::with_version(pdf_engine::core::types::PdfVersion::V1_4);
    let p0 = builder.page(PageSize::A4);
    let quad_a = [72.0, 712.0, 200.0, 712.0, 72.0, 700.0, 200.0, 700.0];
    let quad_b = [72.0, 692.0, 200.0, 692.0, 72.0, 680.0, 200.0, 680.0];
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Highlight, [72.0, 680.0, 200.0, 712.0])
            .with_quad_point_groups(vec![quad_a, quad_b])
            .with_contents("two words"),
    );
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Squiggly, [72.0, 660.0, 200.0, 672.0])
            .with_quad_point_groups(vec![quad_a]),
    );
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Stamp, [300.0, 700.0, 380.0, 720.0])
            .with_stamp_icon(StampIcon::Approved),
    );
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");

    let doc = parse_buf(&buf);
    let annotations = doc.page_annotations(0).expect("annotations");
    assert_eq!(annotations.len(), 3);

    let highlight = &annotations[0];
    assert_eq!(highlight.subtype, AnnotationSubtype::Highlight);
    assert!(highlight.subtype.is_text_markup());
    assert_eq!(
        highlight.text_markup_quad_groups().expect("quads"),
        vec![quad_a, quad_b],
        "两组四边形按文档序 8 数一组还原"
    );

    let squiggly = &annotations[1];
    assert!(squiggly.subtype.is_text_markup());
    assert_eq!(
        squiggly.text_markup_quad_groups().expect("quads"),
        vec![quad_a]
    );

    let stamp = &annotations[2];
    assert_eq!(stamp.subtype, AnnotationSubtype::Stamp);
    assert!(!stamp.subtype.is_text_markup());
    assert_eq!(stamp.stamp_icon(), Some(ReaderStampIcon::Approved));
    assert!(stamp.stamp_icon().unwrap().is_standard());
    assert!(stamp.text_markup_quad_groups().is_none());
}

#[test]
fn builder_underline_strikeout_and_unknown_stamp_icon_roundtrip_through_parser() {
    use pdf_engine::core::builder::{AnnotationKind, PageSize, PdfAnnotation, PdfBuilder};
    use pdf_engine::core::reader::annotation::{AnnotationSubtype, StampIcon as ReaderStampIcon};

    let mut builder = PdfBuilder::with_version(pdf_engine::core::types::PdfVersion::V1_3);
    let p0 = builder.page(PageSize::A4);
    let quad = [72.0, 712.0, 300.0, 712.0, 72.0, 700.0, 300.0, 700.0];
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Underline, [72.0, 700.0, 300.0, 712.0])
            .with_quad_points(vec![72.0, 712.0, 300.0, 712.0, 72.0, 700.0, 300.0, 700.0]),
    );
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::StrikeOut, [72.0, 680.0, 300.0, 692.0])
            .with_quad_point_groups(vec![quad]),
    );
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Stamp, [400.0, 700.0, 460.0, 720.0])
            .with_icon_name("RubberDuck"),
    );
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");

    let doc = parse_buf(&buf);
    let annotations = doc.page_annotations(0).expect("annotations");
    assert_eq!(annotations.len(), 3);

    for (index, expected) in [
        (0usize, AnnotationSubtype::Underline),
        (1, AnnotationSubtype::StrikeOut),
    ] {
        let annotation = &annotations[index];
        assert_eq!(annotation.subtype, expected);
        assert!(annotation.subtype.is_text_markup());
        assert_eq!(
            annotation
                .text_markup_quad_groups()
                .unwrap_or_else(|| panic!("{expected:?} quads")),
            vec![quad],
            "两种写出方法展平后同一读端形态"
        );
        assert!(annotation.stamp_icon().is_none());
    }

    let custom = &annotations[2];
    assert_eq!(
        custom.stamp_icon(),
        Some(ReaderStampIcon::Other("RubberDuck".into()))
    );
    assert!(!custom.stamp_icon().unwrap().is_standard());
    assert_eq!(custom.stamp_icon().unwrap().name(), "RubberDuck");
}
