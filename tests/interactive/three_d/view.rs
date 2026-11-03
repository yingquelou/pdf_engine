//! 3D 视图字典（Table 304）：`/O` 叠加外观与 PDF 1.7 条目 `/RM` `/LS` `/SA` `/NA` `/NR` 的写出→解析往返。

use super::*;

/// 3D 视图 `/O` 叠加外观（13.6.6）：注册但未页面绘制的 Form XObject 经
/// 3D 流 `/VA[0] /O` 与注解内联 `/3DV /O` 引用，解析后指向同一表单且
/// 字节原样回读。
#[test]
fn builder_three_d_view_overlay_roundtrips_through_parser() {
    use pdf_engine::core::builder::{
        AnnotationKind, FormXObject, PageSize, PdfAnnotation, PdfBuilder, ThreeDArtwork,
        ThreeDMatrixSource, ThreeDProjection, ThreeDProjectionType, ThreeDView,
        ThreeDViewSelection,
    };
    use pdf_engine::core::content::ContentStream;
    use pdf_engine::core::reader::three_d::ThreeDViewSelection as ReadSelection;

    const CAMERA: [f64; 12] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 30.0];
    let mut content = ContentStream::new();
    content
        .save_state()
        .set_fill_gray(0.5)
        .rect(0.0, 0.0, 100.0, 50.0)
        .fill()
        .restore_state();
    let expected = content.to_bytes();

    let view = ThreeDView::new("Front")
        .with_matrix_source(ThreeDMatrixSource::M)
        .with_camera_to_world(CAMERA)
        .with_projection(
            ThreeDProjection::new(ThreeDProjectionType::Perspective)
                .with_near(0.1)
                .with_field_of_view(60.0),
        );

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let form = builder.form_xobject(&FormXObject::new([0.0, 0.0, 100.0, 50.0], content));
    let overlay_view = view.with_overlay(form);
    let artwork = builder.add_three_d_artwork(
        ThreeDArtwork::new(vec![0x55, 0x33, 0x44, 0x00]).with_view(overlay_view.clone()),
    );
    builder.add_annotation(
        page,
        PdfAnnotation::new(AnnotationKind::ThreeD, [72.0, 600.0, 200.0, 680.0])
            .with_three_d(artwork),
    );
    builder.add_annotation(
        page,
        PdfAnnotation::new(AnnotationKind::ThreeD, [72.0, 400.0, 200.0, 480.0])
            .with_three_d(artwork)
            .with_three_d_view(ThreeDViewSelection::Inline(Box::new(overlay_view))),
    );

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build 3D view /O");
    let doc = parse_buf(&buf);
    let annotations = doc.page_annotations(0).expect("annotations");
    assert_eq!(annotations.len(), 2);

    let stream = &annotations[0].three_d.as_ref().expect("/3DD").stream;
    assert_eq!(stream.views.len(), 1);
    let overlay = stream.views[0].overlay.as_ref().expect("/VA[0] /O");
    let overlay_num = match overlay {
        Value::Ref(num, 0) => *num,
        other => panic!("expected /VA[0] /O indirect reference, got {other:?}"),
    };
    let overlay_object = doc
        .get_object(overlay_num)
        .expect("overlay object")
        .expect("overlay form exists");
    let Value::Stream(form_dict, _) = &overlay_object.value else {
        panic!(
            "expected /O Form XObject stream, got {:?}",
            overlay_object.value
        );
    };
    assert_eq!(form_dict.get("Subtype"), Some(&Value::Name("Form".into())));
    assert_eq!(
        doc.stream_bytes(&overlay_object.value)
            .expect("overlay form bytes"),
        expected
    );

    let inline = match annotations[1].three_d_view.as_ref().expect("/3DV") {
        ReadSelection::Inline(view) => view,
        other => panic!("expected inline /3DV, got {other:?}"),
    };
    let inline_overlay = inline.overlay.as_ref().expect("/3DV /O");
    let Value::Ref(inline_num, 0) = inline_overlay else {
        panic!("expected /3DV /O indirect reference, got {inline_overlay:?}");
    };
    assert_eq!(*inline_num, overlay_num, "both /O entries share one form");
    let inline_object = doc
        .get_object(*inline_num)
        .expect("inline overlay object")
        .expect("inline overlay form exists");
    assert_eq!(
        doc.stream_bytes(&inline_object.value)
            .expect("inline overlay form bytes"),
        expected
    );
}

/// 3D 视图 PDF 1.7 条目（13.6.4 Table 304/307–312）：`/RM` `/LS` `/SA`
/// `/NA` `/NR` 写出 → 解析 → 回读，负载字典按类型读回且特征画像含 3D 视图
/// 扩展。
#[test]
fn builder_three_d_view_pdf17_entries_roundtrip_through_parser() {
    use pdf_engine::core::builder::{
        AnnotationKind, PageSize, PdfAnnotation, PdfBuilder, ThreeDArtwork, ThreeDColor,
        ThreeDCrossSection, ThreeDFaceColor, ThreeDLightingScheme, ThreeDLightingSchemeType,
        ThreeDMatrixSource, ThreeDNode, ThreeDRenderMode, ThreeDRenderModeType, ThreeDView,
    };
    use pdf_engine::core::types::PdfFeature;

    const CAMERA: [f64; 12] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 30.0];
    let view = ThreeDView::new("Front")
        .with_matrix_source(ThreeDMatrixSource::M)
        .with_camera_to_world(CAMERA)
        .with_render_mode(
            ThreeDRenderMode::new(ThreeDRenderModeType::Illustration)
                .with_auxiliary_color(ThreeDColor::rgb([0.1, 0.2, 0.3]))
                .with_face_color(ThreeDFaceColor::Background)
                .with_opacity(0.7)
                .with_crease_value(30.0),
        )
        .with_lighting(ThreeDLightingScheme::new(
            ThreeDLightingSchemeType::Headlamp,
        ))
        .with_cross_sections(vec![
            ThreeDCrossSection::new()
                .with_center([1.0, 2.0, 3.0])
                .with_orientation([None, Some(-30.0), Some(30.0)])
                .with_plane_opacity(0.25)
                .with_plane_color(ThreeDColor::rgb([0.5, 0.5, 0.5]))
                .with_intersection_visible(true)
                .with_intersection_color(ThreeDColor::rgb([0.0, 1.0, 0.0])),
        ])
        .with_nodes(vec![
            ThreeDNode::new("Wheel")
                .with_opacity(0.5)
                .with_visible(false)
                .with_matrix([1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0, 2.0, 3.0]),
        ])
        .with_restore_nodes(true);

    let mut builder = PdfBuilder::new();
    let p0 = builder.page(PageSize::A4);
    let artwork = builder
        .add_three_d_artwork(ThreeDArtwork::new(vec![0x55, 0x33, 0x44, 0x00]).with_view(view));
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::ThreeD, [72.0, 600.0, 200.0, 680.0])
            .with_three_d(artwork),
    );

    let mut buf = Vec::new();
    builder
        .build(&mut buf)
        .expect("build 3D view PDF 1.7 entries");
    let doc = parse_buf(&buf);
    let annotations = doc.page_annotations(0).expect("annotations");
    assert_eq!(annotations.len(), 1);
    let parsed = annotations[0].three_d.as_ref().expect("/3DD");
    assert_eq!(parsed.stream.views.len(), 1);
    let parsed_view = &parsed.stream.views[0];

    let render_mode = parsed_view.render_mode.as_ref().expect("/RM");
    assert_eq!(render_mode.subtype, ThreeDRenderModeType::Illustration);
    assert_eq!(render_mode.opacity, Some(0.7));
    assert_eq!(render_mode.crease_value, Some(30.0));
    assert_eq!(
        render_mode.auxiliary_color,
        Some(ThreeDColor::rgb([0.1, 0.2, 0.3]))
    );
    assert_eq!(render_mode.face_color, Some(ThreeDFaceColor::Background));
    assert_eq!(
        parsed_view
            .lighting
            .as_ref()
            .map(|lighting| &lighting.subtype),
        Some(&ThreeDLightingSchemeType::Headlamp)
    );

    let cross_sections = parsed_view.cross_sections.as_ref().expect("/SA");
    assert_eq!(cross_sections.len(), 1);
    assert_eq!(cross_sections[0].center, Some([1.0, 2.0, 3.0]));
    assert_eq!(
        cross_sections[0].orientation,
        Some([None, Some(-30.0), Some(30.0)])
    );
    assert_eq!(cross_sections[0].plane_opacity, Some(0.25));
    assert_eq!(
        cross_sections[0].plane_color,
        Some(ThreeDColor::rgb([0.5, 0.5, 0.5]))
    );
    assert_eq!(cross_sections[0].intersection_visible, Some(true));
    assert_eq!(
        cross_sections[0].intersection_color,
        Some(ThreeDColor::rgb([0.0, 1.0, 0.0]))
    );

    let nodes = parsed_view.nodes.as_ref().expect("/NA");
    assert_eq!(nodes.len(), 1);
    // 写端统一以 UTF-16BE/PDFDocEncoding 十六进制串写出文本。
    assert_eq!(nodes[0].name, Value::HexString("576865656C".into()));
    assert_eq!(nodes[0].opacity, Some(0.5));
    assert_eq!(nodes[0].visible, Some(false));
    assert_eq!(
        nodes[0].matrix,
        Some([1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 1.0, 2.0, 3.0])
    );
    assert_eq!(parsed_view.restore_nodes, Some(true));
    assert!(
        doc.features
            .borrow()
            .contains(PdfFeature::ThreeDViewExtensions),
        "parsed 3D view feature profile should include PDF 1.7 entries"
    );
}
