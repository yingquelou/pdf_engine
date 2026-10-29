//! E2 Redact 注解：Table 192 专有条目（`/OverlayText` `/DA` `/Q` `/Repeat`
//! `/RO` `/IC` `/QuadPoints`）的写出 → 解析往返。

use super::*;

/// 写入 → 重新解析：叠加文本条目回读为读端类型化字段，`/DA` `/Q` `/IC`
/// 复用与通用条目同名的字段。
#[test]
fn builder_redact_overlay_text_entries_roundtrip_through_parser() {
    use pdf_engine::core::builder::{AnnotationKind, PageSize, PdfAnnotation, PdfBuilder};
    use pdf_engine::core::reader::annotation::AnnotationSubtype;

    let mut builder = PdfBuilder::new();
    let p0 = builder.page(PageSize::A4);
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Redact, [72.0, 700.0, 300.0, 720.0])
            .with_redact_overlay_text("REDACTED")
            .with_redact_appearance_string("/F1 12 Tf 0 g")
            .with_redact_quadding(2)
            .with_redact_overlay_repeated(true)
            .with_interior_color(vec![0.0, 0.0, 0.0])
            .with_quad_points(vec![72.0, 720.0, 300.0, 720.0, 72.0, 700.0, 300.0, 700.0]),
    );
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build redact");

    let doc = parse_buf(&buf);
    let annots = doc.page_annotations(0).expect("page 0 annotations");
    assert_eq!(annots.len(), 1);
    let redact = &annots[0];
    assert_eq!(redact.subtype, AnnotationSubtype::Redact);
    assert_eq!(redact.overlay_text.as_deref(), Some("REDACTED"));
    assert_eq!(
        redact.default_appearance.as_deref(),
        Some("/F1 12 Tf 0 g"),
        "Table 192 的 /DA 与 FreeText 同键，复用 default_appearance"
    );
    assert_eq!(redact.quadding, Some(2));
    assert_eq!(redact.repeat_overlay_text, Some(true));
    assert_eq!(redact.interior_color, Some(vec![0.0, 0.0, 0.0]));
    assert_eq!(
        redact
            .quad_points
            .as_ref()
            .map(|quad| quad.len())
            .unwrap_or_default(),
        8,
        "/QuadPoints 标出待移除的内容区域"
    );
    assert!(
        redact.overlay_appearance.is_none(),
        "未声明 /RO 时应为 None"
    );
}

/// 写入 → 重新解析：`/RO` 回读为 Form XObject 间接引用，且该表单虽未被页面
/// 绘制仍随文档写出。
#[test]
fn builder_redact_overlay_appearance_roundtrips_through_parser() {
    use pdf_engine::core::builder::{
        AnnotationKind, FormXObject, PageSize, PdfAnnotation, PdfBuilder,
    };
    use pdf_engine::core::content::ContentStream;
    use pdf_engine::core::reader::annotation::AnnotationSubtype;

    let mut builder = PdfBuilder::new();
    let p0 = builder.page(PageSize::A4);
    let overlay = builder.form_xobject(&FormXObject::new(
        [0.0, 0.0, 100.0, 20.0],
        ContentStream::new(),
    ));
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Redact, [72.0, 700.0, 300.0, 720.0])
            .with_redact_overlay_appearance(overlay),
    );
    let mut buf = Vec::new();
    builder
        .build(&mut buf)
        .expect("build redact with /RO overlay form");

    let doc = parse_buf(&buf);
    let annots = doc.page_annotations(0).expect("page 0 annotations");
    let redact = &annots[0];
    assert_eq!(redact.subtype, AnnotationSubtype::Redact);
    let (num, _) = redact
        .overlay_appearance
        .as_ref()
        .and_then(|value| value.as_ref())
        .expect("/RO 应写成间接引用");
    let overlay_obj = doc.get_object(num).expect("object").expect("exists");
    match &overlay_obj.value {
        Value::Stream(dict, _) => assert_eq!(
            dict.get("Subtype"),
            Some(&Value::Name("Form".into())),
            "/RO 指向 Form XObject"
        ),
        other => panic!("/RO references a non-stream object: {other:?}"),
    }
}
