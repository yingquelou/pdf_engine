//! `/AN` 动画样式字典（13.6.3.2 Table 301/302）的写出→解析往返。

use super::*;

/// `/AN` 动画样式字典（13.6.3.2 Table 301/302）写出 → 解析 → 回读：
/// 显式条目按类型读回，未声明动画样式的 artwork 解析为 `None`。
#[test]
fn builder_three_d_animation_style_roundtrips_through_parser() {
    use pdf_engine::core::builder::{
        AnnotationKind, PageSize, PdfAnnotation, PdfBuilder, ThreeDAnimationStyle,
        ThreeDAnimationStyleType, ThreeDArtwork,
    };

    let mut builder = PdfBuilder::new();
    let p0 = builder.page(PageSize::A4);
    let animated = builder.add_three_d_artwork(
        ThreeDArtwork::new(vec![0x55, 0x33, 0x44, 0x00]).with_animation_style(
            ThreeDAnimationStyle::new(ThreeDAnimationStyleType::Linear)
                .with_play_count(-1)
                .with_time_multiplier(2.0),
        ),
    );
    let plain = builder.add_three_d_artwork(ThreeDArtwork::new(vec![0x55, 0x33, 0x44, 0x00]));
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::ThreeD, [72.0, 600.0, 200.0, 680.0])
            .with_three_d(animated),
    );
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::ThreeD, [72.0, 400.0, 200.0, 480.0]).with_three_d(plain),
    );

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build 3D animation style");
    let doc = parse_buf(&buf);
    let annotations = doc.page_annotations(0).expect("annotations");
    assert_eq!(annotations.len(), 2);

    let animation = annotations[0]
        .three_d
        .as_ref()
        .expect("/3DD")
        .stream
        .animation
        .as_ref()
        .expect("/AN animation style");
    assert_eq!(animation.subtype, Some(ThreeDAnimationStyleType::Linear));
    assert_eq!(animation.play_count, Some(-1));
    assert_eq!(animation.time_multiplier, Some(2.0));

    assert_eq!(
        annotations[1]
            .three_d
            .as_ref()
            .expect("/3DD")
            .stream
            .animation,
        None,
        "artwork without /AN parses to None"
    );
}
