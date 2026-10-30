//! 3D 视图负载字典（13.6.4 Table 307–312）写端：`/RM` `/LS` `/SA` `/NA`
//! 的类型化序列化、缺省不物化与嵌套校验失败。

use super::support::{annot_object, assert_text_entry};
use super::*;
use crate::core::types::{Array, Dict, Document, Value};

fn camera() -> [f64; 12] {
    [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 30.0]
}

fn camera_view(name: &str) -> ThreeDView {
    ThreeDView::new(name)
        .with_matrix_source(ThreeDMatrixSource::M)
        .with_camera_to_world(camera())
}

fn three_d_annotation(handle: usize) -> PdfAnnotation {
    PdfAnnotation::new(AnnotationKind::ThreeD, [72.0, 600.0, 200.0, 680.0]).with_three_d(handle)
}

/// 构建 PDF 1.7 文档；失败时返回错误与已写字节。
fn build_document(view: ThreeDView) -> Result<Document, (WriteError, Vec<u8>)> {
    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    let p0 = builder.page(PageSize::A4);
    let artwork = builder.add_three_d_artwork(ThreeDArtwork::new(vec![1, 2, 3, 4]).with_view(view));
    builder.add_annotation(p0, three_d_annotation(artwork));
    let mut out = Vec::new();
    match builder.build(&mut out) {
        Ok(()) => builder.assemble_document().map_err(|error| (error, out)),
        Err(error) => Err((error, out)),
    }
}

/// 取条目中的间接引用对象号；缺失或非引用即 panic。
fn referenced_num(value: Option<&Value>) -> i64 {
    match value {
        Some(Value::Ref(num, 0)) => *num,
        other => panic!("expected indirect reference, got {other:?}"),
    }
}

/// 取 3D 流对象 `/VA[0]` 的视图字典。
fn first_view(doc: &Document, stream_num: i64) -> Dict {
    let object = doc
        .get_object(stream_num)
        .expect("3D stream object")
        .expect("3D stream exists");
    let Value::Stream(dict, _) = &object.value else {
        panic!(
            "expected 3D stream at object {stream_num}, got {:?}",
            object.value
        );
    };
    dict.get("VA")
        .and_then(Value::as_array)
        .and_then(|views| views.0.first())
        .and_then(Value::as_dict)
        .cloned()
        .expect("/VA[0] view dictionary")
}

fn rgb(values: [f64; 3]) -> Value {
    let mut items = vec![Value::Name("DeviceRGB".into())];
    items.extend(values.iter().map(|value| Value::Float(*value)));
    Value::Array(Array(items))
}

fn assemble_view(view: ThreeDView) -> Dict {
    let doc = build_document(view).expect("assemble 3D view");
    let annot = annot_object(&doc, 3, 0);
    let stream_num = referenced_num(annot.get("3DD"));
    first_view(&doc, stream_num)
}

#[test]
fn three_d_view_payload_dictionaries_serialize_typed_entries() {
    let view = camera_view("Front")
        .with_render_mode(
            ThreeDRenderMode::new(ThreeDRenderModeType::Illustration)
                .with_auxiliary_color(ThreeDColor::rgb([0.1, 0.2, 0.3]))
                .with_face_color(ThreeDFaceColor::Background)
                .with_opacity(0.75)
                .with_crease_value(60.0),
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
    let dict = assemble_view(view);

    let render_mode = dict
        .get("RM")
        .and_then(Value::as_dict)
        .expect("/RM dictionary");
    assert_eq!(
        render_mode.get("Type"),
        Some(&Value::Name("3DRenderMode".into()))
    );
    assert_eq!(
        render_mode.get("Subtype"),
        Some(&Value::Name("Illustration".into()))
    );
    assert_eq!(render_mode.get("AC"), Some(&rgb([0.1, 0.2, 0.3])));
    assert_eq!(render_mode.get("FC"), Some(&Value::Name("BG".into())));
    assert_eq!(render_mode.get("O"), Some(&Value::Float(0.75)));
    assert_eq!(render_mode.get("CV"), Some(&Value::Float(60.0)));

    let lighting = dict
        .get("LS")
        .and_then(Value::as_dict)
        .expect("/LS dictionary");
    assert_eq!(
        lighting.get("Type"),
        Some(&Value::Name("3DLightingScheme".into()))
    );
    assert_eq!(
        lighting.get("Subtype"),
        Some(&Value::Name("Headlamp".into()))
    );
    assert_eq!(lighting.0.len(), 2);

    let cross_sections = dict.get("SA").and_then(Value::as_array).expect("/SA array");
    assert_eq!(cross_sections.0.len(), 1);
    let cross_section = cross_sections.0[0].as_dict().expect("/SA[0] dictionary");
    assert_eq!(
        cross_section.get("Type"),
        Some(&Value::Name("3DCrossSection".into()))
    );
    assert_eq!(
        cross_section.get("C"),
        Some(&Value::Array(Array(vec![
            Value::Float(1.0),
            Value::Float(2.0),
            Value::Float(3.0),
        ])))
    );
    assert_eq!(
        cross_section.get("O"),
        Some(&Value::Array(Array(vec![
            Value::Null,
            Value::Float(-30.0),
            Value::Float(30.0),
        ])))
    );
    assert_eq!(cross_section.get("PO"), Some(&Value::Float(0.25)));
    assert_eq!(cross_section.get("PC"), Some(&rgb([0.5, 0.5, 0.5])));
    assert_eq!(cross_section.get("IV"), Some(&Value::Bool(true)));
    assert_eq!(cross_section.get("IC"), Some(&rgb([0.0, 1.0, 0.0])));

    let nodes = dict.get("NA").and_then(Value::as_array).expect("/NA array");
    assert_eq!(nodes.0.len(), 1);
    let node = nodes.0[0].as_dict().expect("/NA[0] dictionary");
    assert_eq!(node.get("Type"), Some(&Value::Name("3DNode".into())));
    assert_text_entry(node, "N");
    assert_eq!(node.get("O"), Some(&Value::Float(0.5)));
    assert_eq!(node.get("V"), Some(&Value::Bool(false)));
    let matrix = node.get("M").and_then(Value::as_array).expect("/M array");
    assert_eq!(matrix.0.len(), 12);
    assert_eq!(matrix.0[11], Value::Float(3.0));
    assert_eq!(dict.get("NR"), Some(&Value::Bool(true)));
}

#[test]
fn three_d_view_empty_payload_arrays_and_defaults_serialize() {
    let dict = assemble_view(
        camera_view("Front")
            .with_render_mode(ThreeDRenderMode::new(ThreeDRenderModeType::Solid))
            .with_lighting(ThreeDLightingScheme::new(ThreeDLightingSchemeType::Artwork))
            .with_cross_sections(Vec::new()),
    );

    assert_eq!(
        dict.get("RM"),
        Some(&Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("3DRenderMode".into()))
                .insert("Subtype", Value::Name("Solid".into()))
        )),
        "缺省 /AC /FC /O /CV 不物化"
    );
    assert_eq!(
        dict.get("LS"),
        Some(&Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("3DLightingScheme".into()))
                .insert("Subtype", Value::Name("Artwork".into()))
        ))
    );
    assert_eq!(dict.get("SA"), Some(&Value::Array(Array(Vec::new()))));
    assert_eq!(dict.get("NA"), None);
}

#[test]
fn invalid_payload_dictionaries_are_rejected_without_output() {
    let cases = [
        (
            camera_view("Front").with_render_mode(
                ThreeDRenderMode::new(ThreeDRenderModeType::Solid)
                    .with_face_color(ThreeDFaceColor::Background),
            ),
            "/RM",
            "/FC is meaningful only for Illustration (Table 307)",
        ),
        (
            camera_view("Front").with_cross_sections(vec![
                ThreeDCrossSection::new().with_orientation([None, None, Some(0.0)]),
            ]),
            "/SA[0]",
            "exactly one null entry (Table 311)",
        ),
        (
            camera_view("Front").with_cross_sections(vec![
                ThreeDCrossSection::new()
                    .with_intersection_color(ThreeDColor::rgb([0.0, 1.0, 0.0])),
            ]),
            "/SA[0]",
            "/IC is meaningful only if /IV is true (Table 311)",
        ),
        (
            camera_view("Front")
                .with_nodes(vec![ThreeDNode::new("Wheel").with_opacity(1.5)])
                .with_restore_nodes(true),
            "/NA[0]",
            "/O must be within 0..=1",
        ),
    ];
    for (view, context, fragment) in cases {
        let (error, out) = build_document(view).expect_err("invalid payload dictionary");
        assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
        let message = error.to_string();
        assert!(message.contains(context), "{message}");
        assert!(message.contains(fragment), "{message}");
        assert!(out.is_empty(), "build failure must not emit bytes");
    }
}
