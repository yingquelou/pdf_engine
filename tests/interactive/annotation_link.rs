//! E2 Link 注解：`/H` `/PA` 专有条目（Table 173）的写出 → 解析往返。

use super::*;

/// 写入 → 重新解析：`/H` 类型化、`/PA` 经 `Document::annotation_pressed_action`
/// 还原为 URI 动作，且与激活动作 `/A` 互不覆盖。
#[test]
fn builder_link_annotation_entries_roundtrip_through_parser() {
    use pdf_engine::core::builder::{
        Action as BuilderAction, AnnotationKind, LinkHighlightMode as BuilderLinkHighlightMode,
        PageSize, PdfAnnotation, PdfBuilder,
    };
    use pdf_engine::core::reader::action::Action as ReaderAction;
    use pdf_engine::core::reader::annotation::{
        AnnotationSubtype, LinkHighlightMode as ReaderLinkHighlightMode,
    };

    let mut builder = PdfBuilder::with_version(pdf_engine::core::types::PdfVersion::V1_4);
    let p0 = builder.page(PageSize::A4);
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Link, [72.0, 700.0, 300.0, 720.0])
            .with_link_highlight(BuilderLinkHighlightMode::Outline)
            .with_pressed_action(BuilderAction::Uri("https://example.com/legacy".into()))
            .with_action(BuilderAction::Uri("https://example.com/current".into())),
    );
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Link, [72.0, 660.0, 300.0, 680.0])
            .with_link_highlight(BuilderLinkHighlightMode::None),
    );
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");

    let doc = parse_buf(&buf);
    let annotations = doc.page_annotations(0).expect("annotations");
    assert_eq!(annotations.len(), 2);

    let link = &annotations[0];
    assert_eq!(link.subtype, AnnotationSubtype::Link);
    assert_eq!(link.link_highlight, Some(ReaderLinkHighlightMode::Outline));
    assert_eq!(
        doc.annotation_pressed_action(link)
            .expect("pressed action")
            .expect("/PA"),
        ReaderAction::Uri("https://example.com/legacy".into())
    );
    let active = doc
        .action(link.action.as_ref().expect("/A"))
        .expect("action");
    assert_eq!(
        active,
        ReaderAction::Uri("https://example.com/current".into())
    );

    let plain = &annotations[1];
    assert_eq!(plain.link_highlight, Some(ReaderLinkHighlightMode::None));
    assert!(plain.pressed_action.is_none());
    assert!(
        doc.annotation_pressed_action(plain)
            .expect("absent /PA")
            .is_none()
    );
}
