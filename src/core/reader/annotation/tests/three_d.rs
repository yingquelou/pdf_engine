//! 3D 注解读端测试（13.6，Table 298–306）。
//!
//! 覆盖 `/3DD` 必填与直接流/引用字典形态、实例共享引用、`/3DV` 选择、
//! 激活与视图框条目、视图/投影/背景嵌套解析，以及畸形输入与上限。

use crate::core::reader::annotation::*;
use crate::core::reader::three_d::*;
use crate::core::types::{Array, Dict, Object, StreamData, Value};

use super::support::*;

fn numbers(values: &[f64]) -> Value {
    Value::Array(Array(
        values.iter().map(|value| Value::Float(*value)).collect(),
    ))
}

fn view(name: &str) -> Dict {
    Dict::new()
        .insert("Type", Value::Name("3DView".into()))
        .insert("XN", Value::String(name.into()))
}

fn stream(subtype: &str) -> Dict {
    Dict::new()
        .insert("Type", Value::Name("3D".into()))
        .insert("Subtype", Value::Name(subtype.into()))
}

fn stream_value(dict: Dict) -> Value {
    Value::Stream(dict, StreamData::Owned(Vec::new()))
}

fn stream_object(num: i64, dict: Dict) -> Object {
    Object::new(num, 0, stream_value(dict))
}

fn annotation_with_stream(dict: Dict, stream: Dict) -> Annotation {
    let document = doc(vec![catalog_object()]);
    let annotation = dict.insert("3DD", stream_value(stream));
    parse_annotation(&document, 0, None, &annotation).expect("3D annotation")
}

#[test]
fn three_d_annotation_requires_3dd_and_reads_inline_stream() {
    let document = doc(vec![catalog_object()]);
    let error =
        parse_annotation(&document, 0, None, &base_annotation("3D")).expect_err("missing /3DD");
    assert!(
        matches!(&error, AnnotationError::Malformed(message)
            if message.contains("3D annotation") && message.contains("/3DD")),
        "{error:?}"
    );

    let annotation = annotation_with_stream(
        base_annotation("3D"),
        stream("U3D").insert("DV", Value::Name("F".into())),
    );
    assert_eq!(annotation.subtype, AnnotationSubtype::ThreeD);
    let artwork = annotation.three_d.as_ref().expect("/3DD");
    assert_eq!(artwork.source, None, "直接内联流没有来源引用");
    assert_eq!(artwork.reference, None);
    assert_eq!(artwork.stream.format, ThreeDStreamFormat::U3d);
    assert_eq!(
        artwork.stream.default_view,
        Some(ThreeDViewSelection::First)
    );
    assert_eq!(annotation.three_d_view, None);
    assert_eq!(annotation.three_d_activation, None);
    assert_eq!(annotation.three_d_interactive, None);
    assert_eq!(annotation.three_d_view_box, None);
}

#[test]
fn three_d_reference_dictionary_shares_run_time_instance() {
    let shared = Dict::new()
        .insert("Type", Value::Name("3DRef".into()))
        .insert("3DD", Value::Ref(200, 0));
    let a = base_annotation("3D").insert("3DD", Value::Ref(201, 0));
    let b = base_annotation("3D").insert("3DD", Value::Ref(201, 0));
    let c = base_annotation("3D").insert("3DD", Value::Ref(200, 0));
    let document = doc_with_annots(
        vec![Value::Ref(100, 0), Value::Ref(101, 0), Value::Ref(102, 0)],
        vec![
            stream_object(200, stream("U3D")),
            Object::new(201, 0, Value::Dict(shared)),
            Object::new(100, 0, Value::Dict(a)),
            Object::new(101, 0, Value::Dict(b)),
            Object::new(102, 0, Value::Dict(c)),
        ],
    );
    let annotations = document.page_annotations(0).expect("page annotations");
    assert_eq!(annotations.len(), 3);
    for index in [0, 1] {
        let artwork = annotations[index].three_d.as_ref().expect("/3DD");
        assert_eq!(artwork.reference, Some((201, 0)), "共享引用字典对象号");
        assert_eq!(artwork.source, Some((200, 0)), "实际流对象引用");
    }
    assert_eq!(
        annotations[0].three_d.as_ref().unwrap().reference,
        annotations[1].three_d.as_ref().unwrap().reference
    );
    let direct = annotations[2].three_d.as_ref().expect("/3DD");
    assert_eq!(direct.reference, None, "直接引用流不共享实例");
    assert_eq!(direct.source, Some((200, 0)));
}

#[test]
fn three_d_entries_are_read_on_non_3d_subtypes() {
    let document = doc(vec![catalog_object()]);
    let dict = base_annotation("Text")
        .insert("3DD", stream_value(stream("U3D")))
        .insert("3DI", Value::Bool(false));
    let annotation = parse_annotation(&document, 0, None, &dict).expect("tolerant /3DD");
    assert_eq!(annotation.subtype, AnnotationSubtype::Text);
    assert!(annotation.three_d.is_some());
    assert_eq!(annotation.three_d_interactive, Some(false));
}

#[test]
fn activation_view_box_and_interactive_entries() {
    let document = doc(vec![catalog_object()]);
    let dict = base_annotation("3D")
        .insert("3DD", stream_value(stream("U3D")))
        .insert(
            "3DA",
            Value::Dict(
                Dict::new()
                    .insert("A", Value::Name("PO".into()))
                    .insert("AIS", Value::Name("L".into()))
                    .insert("D", Value::Name("XD".into()))
                    .insert("DIS", Value::Name("U".into()))
                    .insert("TB", Value::Bool(true))
                    .insert("NP", Value::Bool(false)),
            ),
        )
        .insert("3DI", Value::Bool(false))
        .insert("3DB", numbers(&[-10.0, -20.0, 10.0, 20.0]));
    let annotation = parse_annotation(&document, 0, None, &dict).expect("3D entries");
    let activation = annotation.three_d_activation.expect("/3DA");
    assert_eq!(
        activation.activation,
        Some(ThreeDActivationPolicy::PageOpen)
    );
    assert_eq!(activation.activation_state, Some(ThreeDArtworkState::Live));
    assert_eq!(
        activation.deactivation,
        Some(ThreeDDeactivationPolicy::Explicit)
    );
    assert_eq!(
        activation.deactivation_state,
        Some(ThreeDArtworkState::Uninstantiated)
    );
    assert_eq!(activation.toolbar, Some(true));
    assert_eq!(activation.navigation_panel, Some(false));
    assert_eq!(annotation.three_d_interactive, Some(false));
    assert_eq!(
        annotation.three_d_view_box,
        Some([-10.0, -20.0, 10.0, 20.0])
    );

    // 缺省的激活字典不物化。
    let defaults = annotation_with_stream(base_annotation("3D"), stream("U3D"));
    assert!(defaults.three_d_activation.is_none());
    assert!(defaults.three_d_interactive.is_none());
    assert!(defaults.three_d_view_box.is_none());
}

#[test]
fn three_d_view_selection_forms() {
    let with_views = || {
        stream("U3D").insert(
            "VA",
            Value::Array(Array(vec![
                Value::Dict(view("Front")),
                Value::Dict(view("Back")),
            ])),
        )
    };
    let selections = [
        (Value::Integer(1), ThreeDViewSelection::Index(1)),
        (
            Value::String("Back".into()),
            ThreeDViewSelection::Name(Value::String("Back".into())),
        ),
        (Value::Name("F".into()), ThreeDViewSelection::First),
        (Value::Name("L".into()), ThreeDViewSelection::Last),
    ];
    for (value, expected) in selections {
        let annotation =
            annotation_with_stream(base_annotation("3D").insert("3DV", value), with_views());
        assert_eq!(annotation.three_d_view, Some(expected));
    }

    let inline = annotation_with_stream(
        base_annotation("3D").insert(
            "3DV",
            Value::Dict(view("Front").insert("IN", Value::String("front".into()))),
        ),
        with_views(),
    );
    match inline.three_d_view.expect("/3DV") {
        ThreeDViewSelection::Inline(view) => {
            assert_eq!(view.external_name, Value::String("Front".into()));
            assert_eq!(view.internal_name, Some(Value::String("front".into())));
        }
        other => panic!("unexpected selection: {other:?}"),
    }

    let unknown = annotation_with_stream(
        base_annotation("3D").insert("3DV", Value::Name("D".into())),
        with_views(),
    );
    assert_eq!(
        unknown.three_d_view,
        Some(ThreeDViewSelection::Other("D".into()))
    );
}

#[test]
fn three_d_view_selection_errors() {
    let with_views = || {
        stream("U3D").insert(
            "VA",
            Value::Array(Array(vec![
                Value::Dict(view("Front")),
                Value::Dict(view("Back")),
            ])),
        )
    };
    for value in [Value::Integer(2), Value::Integer(-1), Value::Float(0.5)] {
        let document = doc(vec![catalog_object()]);
        let dict = base_annotation("3D")
            .insert("3DD", stream_value(with_views()))
            .insert("3DV", value.clone());
        let error = parse_annotation(&document, 0, None, &dict).expect_err("bad /3DV");
        assert!(
            matches!(&error, AnnotationError::Malformed(message)
                if message.contains("/3DV")),
            "{value:?}: {error:?}"
        );
    }
}

#[test]
fn stream_views_projection_background_and_overlay_are_typed() {
    let c2w = numbers(&[1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0]);
    let projection = Dict::new()
        .insert("Subtype", Value::Name("P".into()))
        .insert("N", Value::Integer(2))
        .insert("FOV", Value::Integer(90))
        .insert("PS", Value::Name("Min".into()));
    let background = Dict::new()
        .insert("Type", Value::Name("3DBG".into()))
        .insert("C", numbers(&[1.0, 0.0, 0.0]))
        .insert("EA", Value::Bool(true));
    let stream = stream("U3D").insert(
        "VA",
        Value::Array(Array(vec![Value::Dict(
            view("Front")
                .insert("MS", Value::Name("M".into()))
                .insert("C2W", c2w)
                .insert("P", Value::Dict(projection))
                .insert("BG", Value::Dict(background))
                .insert("O", Value::Ref(90, 0))
                .insert(
                    "RM",
                    Value::Dict(
                        Dict::new()
                            .insert("Type", Value::Name("3DRenderMode".into()))
                            .insert("Subtype", Value::Name("Solid".into())),
                    ),
                )
                .insert("NR", Value::Bool(true)),
        )])),
    );
    let annotation = annotation_with_stream(base_annotation("3D"), stream);
    let view = &annotation.three_d.expect("/3DD").stream.views[0];
    assert_eq!(view.matrix_source, Some(ThreeDMatrixSource::M));
    assert_eq!(view.camera_to_world.map(|matrix| matrix[0]), Some(1.0));
    let projection = view.projection.as_ref().expect("/P");
    assert_eq!(projection.subtype, ThreeDProjectionType::Perspective);
    assert_eq!(projection.near, Some(2.0));
    assert_eq!(projection.field_of_view, Some(90.0));
    assert_eq!(projection.scale, Some(ThreeDProjectionScale::Min));
    let background = view.background.as_ref().expect("/BG");
    assert_eq!(background.color, Some(numbers(&[1.0, 0.0, 0.0])));
    assert_eq!(background.entire_annotation, Some(true));
    assert_eq!(view.overlay, Some(Value::Ref(90, 0)));
    assert_eq!(
        view.render_mode,
        Some(ThreeDRenderMode {
            subtype: ThreeDRenderModeType::Solid,
            auxiliary_color: None,
            face_color: None,
            opacity: None,
            crease_value: None,
        })
    );
    assert_eq!(view.restore_nodes, Some(true));
}

#[test]
fn malformed_three_d_structures_are_typed_errors() {
    let document = doc(vec![catalog_object()]);
    let cases: Vec<(Value, &str)> = vec![
        (Value::Integer(1), "/3DD"),
        (
            Value::Dict(
                Dict::new()
                    .insert("Type", Value::Name("Wrong".into()))
                    .insert("3DD", Value::Ref(200, 0)),
            ),
            "/Type",
        ),
        (
            Value::Dict(Dict::new().insert("Type", Value::Name("3DRef".into()))),
            "/3DD",
        ),
        (
            Value::Dict(
                Dict::new()
                    .insert("Type", Value::Name("3DRef".into()))
                    .insert("3DD", Value::Integer(200)),
            ),
            "not a 3D stream",
        ),
        (
            stream_value(
                Dict::new()
                    .insert("Type", Value::Name("Wrong".into()))
                    .insert("Subtype", Value::Name("U3D".into())),
            ),
            "/Type",
        ),
        (stream_value(Dict::new()), "/Subtype"),
        (
            stream_value(stream("U3D").insert(
                "VA",
                Value::Array(Array(vec![Value::Dict(
                    Dict::new().insert("Type", Value::Name("3DView".into())),
                )])),
            )),
            "/XN",
        ),
        (
            stream_value(stream("U3D").insert(
                "VA",
                Value::Array(Array(vec![Value::Dict(
                    view("Front")
                        .insert("MS", Value::Name("M".into()))
                        .insert("C2W", numbers(&[0.0; 11])),
                )])),
            )),
            "/C2W",
        ),
        (
            stream_value(stream("U3D").insert(
                "VA",
                Value::Array(Array(vec![Value::Dict(
                    view("Front").insert("MS", Value::Name("U3D".into())),
                )])),
            )),
            "/U3DPath",
        ),
        (
            stream_value(stream("U3D").insert(
                "VA",
                Value::Array(Array(vec![Value::Dict(
                    view("Front").insert(
                        "P",
                        Value::Dict(
                            Dict::new()
                                .insert("Subtype", Value::Name("P".into()))
                                .insert("N", Value::Integer(1))
                                .insert("FOV", Value::Integer(181)),
                        ),
                    ),
                )])),
            )),
            "/FOV",
        ),
        (
            stream_value(stream("U3D").insert(
                "VA",
                Value::Array(Array(vec![Value::Dict(view("Front").insert(
                    "BG",
                    Value::Dict(Dict::new().insert("Type", Value::Name("Bad".into()))),
                ))])),
            )),
            "/Type",
        ),
    ];
    for (value, fragment) in cases {
        let dict = base_annotation("3D").insert("3DD", value);
        let error = parse_annotation(&document, 0, None, &dict).expect_err("malformed 3D");
        assert!(
            matches!(&error, AnnotationError::Malformed(message)
                if message.contains("3D annotation") && message.contains(fragment)),
            "{fragment}: {error:?}"
        );
    }
}

#[test]
fn malformed_three_d_annotation_entries_are_typed_errors() {
    let document = doc(vec![catalog_object()]);
    for (dict, fragment) in [
        (
            base_annotation("3D")
                .insert("3DD", stream_value(stream("U3D")))
                .insert("3DI", Value::Integer(1)),
            "/3DI",
        ),
        (
            base_annotation("3D")
                .insert("3DD", stream_value(stream("U3D")))
                .insert("3DA", Value::Integer(1)),
            "/3DA",
        ),
        (
            base_annotation("3D")
                .insert("3DD", stream_value(stream("U3D")))
                .insert(
                    "3DA",
                    Value::Dict(Dict::new().insert("A", Value::Integer(1))),
                ),
            "/A",
        ),
        (
            base_annotation("3D")
                .insert("3DD", stream_value(stream("U3D")))
                .insert("3DB", numbers(&[0.0, 0.0, 1.0])),
            "/3DB",
        ),
    ] {
        let error = parse_annotation(&document, 0, None, &dict).expect_err("malformed entries");
        assert!(
            matches!(&error, AnnotationError::Malformed(message)
                if message.contains(fragment)),
            "{fragment}: {error:?}"
        );
    }
}

#[test]
fn unknown_stream_subtype_is_tolerated() {
    let annotation = annotation_with_stream(
        base_annotation("3D"),
        stream("PRC").insert("DV", Value::Name("L".into())),
    );
    let stream = annotation.three_d.expect("/3DD").stream;
    assert_eq!(stream.format, ThreeDStreamFormat::Other("PRC".into()));
    assert_eq!(stream.default_view, Some(ThreeDViewSelection::Last));
}

#[test]
fn three_d_limits_are_annotation_limit_errors() {
    let document = doc(vec![catalog_object()]);
    let views: Vec<Value> = (0..=1024)
        .map(|index| Value::Dict(view(&format!("View {index}"))))
        .collect();
    let dict = base_annotation("3D").insert(
        "3DD",
        stream_value(stream("U3D").insert("VA", Value::Array(Array(views)))),
    );
    let error = parse_annotation(&document, 0, None, &dict).expect_err("too many views");
    assert!(
        matches!(&error, AnnotationError::LimitExceeded(what) if what.contains("3D")),
        "{error:?}"
    );

    let path: Vec<Value> = (0..=64)
        .map(|index| Value::String(format!("Node {index}")))
        .collect();
    let dict = base_annotation("3D").insert(
        "3DD",
        stream_value(stream("U3D").insert(
            "VA",
            Value::Array(Array(vec![Value::Dict(
                view("Front")
                    .insert("MS", Value::Name("U3D".into()))
                    .insert("U3DPath", Value::Array(Array(path))),
            )])),
        )),
    );
    let error = parse_annotation(&document, 0, None, &dict).expect_err("too long path");
    assert!(
        matches!(&error, AnnotationError::LimitExceeded(what) if what.contains("3D")),
        "{error:?}"
    );
}
