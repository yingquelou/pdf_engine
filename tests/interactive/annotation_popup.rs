//! E2：Popup 关联的写出 → 解析往返（12.5.2、Table 163、Table 183）：
//! 根模块只声明一侧，另一侧由写端自动补出，读端两个方向都能解析为引用。

use super::*;

/// 写入 → 重新解析：Popup 注解的 `/Parent` 父注解引用与 `/Open` 展开标志
/// （Table 183）应回读为读端类型化字段，父引用指向父注解的间接对象；
/// 只声明 `/Parent` 时，父注解的 `/Popup` 由写端补出。
#[test]
fn builder_popup_parent_roundtrips_through_parser() {
    use pdf_engine::core::builder::{AnnotationKind, PageSize, PdfAnnotation, PdfBuilder};
    use pdf_engine::core::reader::annotation::AnnotationSubtype;

    let mut builder = PdfBuilder::new();
    let p0 = builder.page(PageSize::A4);
    let parent = builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Text, [72.0, 700.0, 92.0, 720.0])
            .with_contents("Parent note"),
    );
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Popup, [96.0, 690.0, 240.0, 750.0])
            .with_popup_parent(parent)
            .with_open(true),
    );
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");

    let doc = parse_buf(&buf);
    let annots = doc.page_annotations(0).expect("page 0 annotations");
    assert_eq!(annots.len(), 2);
    let text = &annots[0];
    assert_eq!(text.subtype, AnnotationSubtype::Text);
    assert!(text.parent.is_none(), "Text 注解无 /Parent");
    let parent_ref = text
        .reference
        .expect("Text annotation is an indirect object");
    let popup = &annots[1];
    assert_eq!(popup.subtype, AnnotationSubtype::Popup);
    assert_eq!(popup.parent, Some(parent_ref), "/Parent 指向父注解对象");
    assert_eq!(popup.is_open, Some(true));
    // 反方向由写端补出：Text 的 `/Popup` 指向同一个弹出框对象。
    let popup_handle_ref = annots[0].popup.expect("auto-paired /Popup");
    assert_eq!(
        popup_handle_ref,
        popup.reference.expect("popup is an indirect object"),
        "the parent's /Popup is auto-paired with the popup's /Parent"
    );
}

/// 写入 → 重新解析：只在父注解声明 `/Popup`（Table 163）时，弹出框的
/// `/Parent`（Table 183 必填）由写端补出，两端引用互相指向对方。
#[test]
fn builder_popup_parent_is_inferred_from_popup_reference() {
    use pdf_engine::core::builder::{AnnotationKind, PageSize, PdfAnnotation, PdfBuilder};
    use pdf_engine::core::reader::annotation::AnnotationSubtype;

    let mut builder = PdfBuilder::new();
    let p0 = builder.page(PageSize::A4);
    let popup = builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Popup, [96.0, 690.0, 240.0, 750.0]),
    );
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Highlight, [72.0, 700.0, 220.0, 712.0])
            .with_quad_points(vec![72.0, 712.0, 220.0, 712.0, 72.0, 700.0, 220.0, 700.0])
            .with_popup(popup),
    );
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");

    let doc = parse_buf(&buf);
    let annots = doc.page_annotations(0).expect("page 0 annotations");
    assert_eq!(annots.len(), 2);
    let (popup, markup) = (&annots[0], &annots[1]);
    assert_eq!(popup.subtype, AnnotationSubtype::Popup);
    assert_eq!(markup.subtype, AnnotationSubtype::Highlight);
    let markup_ref = markup
        .reference
        .expect("markup annotation is an indirect object");
    assert_eq!(
        popup.parent,
        Some(markup_ref),
        "/Parent is auto-paired from the markup's /Popup"
    );
    assert_eq!(
        markup.popup,
        Some(popup.reference.expect("popup is an indirect object")),
        "declared /Popup keeps pointing at the popup"
    );
}
