//! 3D artwork 流（13.6 Table 300/303）与 3D 注解（Table 298）写出 → 解析 → 回读。

use super::*;

/// 启用引用字典（Table 303）与 `/OnInstantiate` JavaScript 流的 3D
/// artwork：两个注解共享引用字典与运行实例，脚本流经间接引用回读。
#[test]
fn builder_three_d_reference_and_on_instantiate_roundtrip() {
    use pdf_engine::core::builder::{
        AnnotationKind, PageSize, PdfAnnotation, PdfBuilder, ThreeDArtwork,
    };
    use pdf_engine::core::types::StreamData;

    const SCRIPT: &[u8] = b"app.alert('3D');";
    let mut builder = PdfBuilder::new();
    let p0 = builder.page(PageSize::A4);
    let artwork = builder.add_three_d_artwork(
        ThreeDArtwork::new(vec![0x55, 0x33, 0x44, 0x00])
            .with_reference_dictionary()
            .with_on_instantiate(SCRIPT.to_vec()),
    );
    for y in [600.0, 400.0] {
        builder.add_annotation(
            p0,
            PdfAnnotation::new(AnnotationKind::ThreeD, [72.0, y, 200.0, y + 80.0])
                .with_three_d(artwork),
        );
    }

    let mut buf = Vec::new();
    builder
        .build(&mut buf)
        .expect("build 3D reference dictionary and /OnInstantiate");
    let doc = parse_buf(&buf);
    let annotations = doc.page_annotations(0).expect("annotations");
    assert_eq!(annotations.len(), 2);

    let first = annotations[0].three_d.as_ref().expect("/3DD");
    let second = annotations[1].three_d.as_ref().expect("/3DD");
    let reference = first.reference.expect("reference dictionary");
    assert_eq!(second.reference, Some(reference));
    let source = first.source.expect("3D stream reference");
    assert_eq!(second.source, Some(source));
    assert_ne!(reference, source, "reference dictionary wraps the stream");

    let script_num = match first.stream.on_instantiate.as_ref() {
        Some(Value::Ref(num, 0)) => *num,
        other => panic!("expected /OnInstantiate reference, got {other:?}"),
    };
    let object = doc
        .get_object(script_num)
        .expect("OnInstantiate object")
        .expect("OnInstantiate exists");
    let Value::Stream(_, data) = &object.value else {
        panic!("expected /OnInstantiate stream, got {:?}", object.value);
    };
    assert!(
        matches!(
            data,
            StreamData::Ref { .. } | StreamData::EncryptedRef { .. }
        ),
        "parsed streams reference file bytes, not owned copies"
    );
    assert_eq!(
        doc.stream_bytes(&object.value).expect("script bytes"),
        SCRIPT
    );
}

/// 注册含两个预设视图的 3D artwork，分别内联与按索引选择视图；
/// 解析后校验流格式、视图矩阵来源、激活/交互/视图框与 `/3DV` 选择。
#[test]
fn builder_three_d_annotation_roundtrips_through_parser() {
    use pdf_engine::core::builder::{
        AnnotationKind, PageSize, PdfAnnotation, PdfBuilder, ThreeDActivation,
        ThreeDActivationPolicy, ThreeDArtwork, ThreeDArtworkState, ThreeDDeactivationPolicy,
        ThreeDMatrixSource, ThreeDStreamFormat, ThreeDView, ThreeDViewSelection,
    };
    use pdf_engine::core::reader::annotation::AnnotationSubtype;
    use pdf_engine::core::reader::three_d::ThreeDViewSelection as ReadSelection;

    const CAMERA: [f64; 12] = [
        1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 10.0, 20.0, 30.0,
    ];
    let front = ThreeDView::new("Front")
        .with_internal_name("front")
        .with_matrix_source(ThreeDMatrixSource::M)
        .with_camera_to_world(CAMERA);
    let back = ThreeDView::new("Back")
        .with_internal_name("back")
        .with_matrix_source(ThreeDMatrixSource::U3D)
        .with_u3d_path(vec!["Back".into()]);

    let mut builder = PdfBuilder::new();
    let p0 = builder.page(PageSize::A4);
    let artwork = builder.add_three_d_artwork(
        ThreeDArtwork::new(vec![0x55, 0x33, 0x44, 0x00])
            .with_view(front)
            .with_view(back)
            .with_default_view(ThreeDViewSelection::Index(0)),
    );
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::ThreeD, [72.0, 600.0, 200.0, 680.0])
            .with_three_d(artwork)
            .with_three_d_view(ThreeDViewSelection::Inline(Box::new(
                ThreeDView::new("Inline")
                    .with_internal_name("inline")
                    .with_matrix_source(ThreeDMatrixSource::M)
                    .with_camera_to_world(CAMERA),
            )))
            .with_three_d_activation(
                ThreeDActivation::new()
                    .with_activation(ThreeDActivationPolicy::PageVisible)
                    .with_activation_state(ThreeDArtworkState::Instantiated)
                    .with_deactivation(ThreeDDeactivationPolicy::Explicit)
                    .with_deactivation_state(ThreeDArtworkState::Uninstantiated)
                    .with_toolbar(false)
                    .with_navigation_panel(true),
            )
            .with_three_d_interactive(false)
            .with_three_d_view_box([-10.0, -20.0, 10.0, 20.0]),
    );
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::ThreeD, [72.0, 400.0, 200.0, 500.0])
            .with_three_d(artwork)
            .with_three_d_view(ThreeDViewSelection::Index(1)),
    );

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build 3D annotations");
    let doc = parse_buf(&buf);
    let annotations = doc.page_annotations(0).expect("annotations");
    assert_eq!(annotations.len(), 2);

    let annotation = &annotations[0];
    assert_eq!(annotation.subtype, AnnotationSubtype::ThreeD);
    let parsed = annotation.three_d.as_ref().expect("/3DD");
    assert!(parsed.source.is_some(), "3D stream should be indirect");
    assert_eq!(
        parsed.reference, None,
        "writer references the stream directly"
    );
    assert_eq!(parsed.stream.format, ThreeDStreamFormat::U3d);
    assert_eq!(parsed.stream.default_view, Some(ReadSelection::Index(0)));

    assert_eq!(parsed.stream.views.len(), 2);
    let front_view = &parsed.stream.views[0];
    assert_eq!(
        front_view.external_name,
        Value::HexString("46726F6E74".into())
    );
    assert_eq!(
        front_view.internal_name,
        Some(Value::HexString("66726F6E74".into()))
    );
    assert_eq!(front_view.matrix_source, Some(ThreeDMatrixSource::M));
    assert_eq!(front_view.camera_to_world, Some(CAMERA));
    let back_view = &parsed.stream.views[1];
    assert_eq!(back_view.external_name, Value::HexString("4261636B".into()));
    assert_eq!(back_view.matrix_source, Some(ThreeDMatrixSource::U3D));
    assert_eq!(
        back_view.u3d_path,
        Some(vec![Value::HexString("4261636B".into())])
    );

    match annotation.three_d_view.as_ref().expect("/3DV") {
        ReadSelection::Inline(view) => {
            assert_eq!(view.external_name, Value::HexString("496E6C696E65".into()));
            assert_eq!(
                view.internal_name,
                Some(Value::HexString("696E6C696E65".into()))
            );
            assert_eq!(view.matrix_source, Some(ThreeDMatrixSource::M));
            assert_eq!(view.camera_to_world, Some(CAMERA));
        }
        other => panic!("expected inline view, got {other:?}"),
    }
    let activation = annotation.three_d_activation.as_ref().expect("/3DA");
    assert_eq!(
        activation.activation,
        Some(ThreeDActivationPolicy::PageVisible)
    );
    assert_eq!(
        activation.activation_state,
        Some(ThreeDArtworkState::Instantiated)
    );
    assert_eq!(
        activation.deactivation,
        Some(ThreeDDeactivationPolicy::Explicit)
    );
    assert_eq!(
        activation.deactivation_state,
        Some(ThreeDArtworkState::Uninstantiated)
    );
    assert_eq!(activation.toolbar, Some(false));
    assert_eq!(activation.navigation_panel, Some(true));
    assert_eq!(annotation.three_d_interactive, Some(false));
    assert_eq!(
        annotation.three_d_view_box,
        Some([-10.0, -20.0, 10.0, 20.0])
    );

    let index_annotation = &annotations[1];
    assert_eq!(index_annotation.subtype, AnnotationSubtype::ThreeD);
    assert_eq!(index_annotation.three_d_view, Some(ReadSelection::Index(1)));
    assert!(index_annotation.three_d_activation.is_none());
    assert_eq!(index_annotation.three_d_interactive, None);
    assert_eq!(index_annotation.three_d_view_box, None);
    assert_eq!(
        index_annotation.three_d.as_ref().expect("/3DD").source,
        annotation.three_d.as_ref().expect("/3DD").source,
        "both annotations share the artwork stream object"
    );
}

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

/// 3D 视图 PDF 1.7 条目（13.6.4.1 Table 304）：`/RM` `/LS` `/SA` `/NA`
/// `/NR` 写出 → 解析 → 回读，原值保留且特征画像含 3D 视图扩展。
#[test]
fn builder_three_d_view_pdf17_entries_roundtrip_through_parser() {
    use pdf_engine::core::builder::{
        AnnotationKind, PageSize, PdfAnnotation, PdfBuilder, ThreeDArtwork, ThreeDMatrixSource,
        ThreeDView,
    };
    use pdf_engine::core::types::PdfFeature;

    const CAMERA: [f64; 12] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 30.0];
    let render_mode = Dict::new()
        .insert("Type", Value::Name("3DRenderMode".into()))
        .insert("Subtype", Value::Name("Solid".into()));
    let lighting = Dict::new()
        .insert("Type", Value::Name("3DLightingScheme".into()))
        .insert("Subtype", Value::Name("Headlamp".into()));
    let cross_section = Dict::new()
        .insert("Type", Value::Name("3DCrossSection".into()))
        .insert(
            "Center",
            Value::Array(Array(vec![
                Value::Integer(1),
                Value::Integer(2),
                Value::Integer(3),
            ])),
        );
    let node = Dict::new()
        .insert("Type", Value::Name("3DNode".into()))
        .insert("AP", Value::Name("true".into()));
    let cross_sections = Value::Array(Array(vec![Value::Dict(cross_section.clone())]));
    let nodes = Value::Array(Array(vec![Value::Dict(node.clone())]));

    let view = ThreeDView::new("Front")
        .with_matrix_source(ThreeDMatrixSource::M)
        .with_camera_to_world(CAMERA)
        .with_render_mode(Value::Dict(render_mode.clone()))
        .with_lighting(Value::Dict(lighting.clone()))
        .with_cross_sections(cross_sections.clone())
        .with_nodes(nodes.clone())
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
    assert_eq!(parsed_view.render_mode, Some(Value::Dict(render_mode)));
    assert_eq!(parsed_view.lighting, Some(Value::Dict(lighting)));
    assert_eq!(parsed_view.cross_sections, Some(cross_sections));
    assert_eq!(parsed_view.nodes, Some(nodes));
    assert_eq!(parsed_view.restore_nodes, Some(true));
    assert!(
        doc.features
            .borrow()
            .contains(PdfFeature::ThreeDViewExtensions),
        "parsed 3D view feature profile should include PDF 1.7 entries"
    );
}

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
