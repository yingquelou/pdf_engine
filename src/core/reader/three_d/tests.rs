//! 3D 读端低层测试：激活、流、视图、投影与背景字典（13.6 Table 299–306）。

use crate::core::types::{Array, Dict, Document, Value};

use super::activation::{ThreeDActivationPolicy, ThreeDArtworkState, ThreeDDeactivationPolicy};
use super::animation::{ThreeDAnimationStyle, ThreeDAnimationStyleType};
use super::background::ThreeDBackgroundType;
use super::projection::{
    ThreeDClipping, ThreeDProjectionBinding, ThreeDProjectionScale, ThreeDProjectionType,
};
use super::stream::ThreeDStreamFormat;
use super::view::ThreeDViewSelection;
use super::*;

mod views;

fn document() -> Document {
    Document::new()
}

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

fn parse_projection(dict: Dict) -> Result<ThreeDProjection, ThreeDError> {
    super::projection::parse_projection(&document(), &dict, "projection")
}

#[test]
fn projection_perspective_reads_entries_and_scale() {
    let projection = parse_projection(
        Dict::new()
            .insert("Subtype", Value::Name("P".into()))
            .insert("CS", Value::Name("XNF".into()))
            .insert("F", Value::Float(100.0))
            .insert("N", Value::Integer(2))
            .insert("FOV", Value::Float(45.0))
            .insert("PS", Value::Name("Min".into())),
    )
    .expect("perspective projection");
    assert_eq!(projection.subtype, ThreeDProjectionType::Perspective);
    assert_eq!(projection.clipping, Some(ThreeDClipping::Explicit));
    assert_eq!(projection.far, Some(100.0));
    assert_eq!(projection.near, Some(2.0));
    assert_eq!(projection.field_of_view, Some(45.0));
    assert_eq!(projection.scale, Some(ThreeDProjectionScale::Min));
    assert_eq!(projection.orthographic_scale, None);
    assert_eq!(projection.binding, None);

    let diameter = parse_projection(
        Dict::new()
            .insert("Subtype", Value::Name("P".into()))
            .insert("N", Value::Float(1.5))
            .insert("FOV", Value::Integer(180))
            .insert("PS", Value::Float(2.5)),
    )
    .expect("numeric scale");
    assert_eq!(diameter.scale, Some(ThreeDProjectionScale::Diameter(2.5)));
    assert_eq!(diameter.clipping, None, "`/CS` 缺省 ANF 不物化");

    let unknown = parse_projection(
        Dict::new()
            .insert("Subtype", Value::Name("P".into()))
            .insert("N", Value::Integer(1))
            .insert("FOV", Value::Integer(0))
            .insert("PS", Value::Name("Z".into())),
    )
    .expect("unknown scale name");
    assert_eq!(
        unknown.scale,
        Some(ThreeDProjectionScale::Other("Z".into()))
    );
}

#[test]
fn projection_perspective_requires_near_and_fov() {
    let base = |n: Option<Value>, fov: Option<Value>| {
        let mut dict = Dict::new().insert("Subtype", Value::Name("P".into()));
        if let Some(n) = n {
            dict = dict.insert("N", n);
        }
        if let Some(fov) = fov {
            dict = dict.insert("FOV", fov);
        }
        dict
    };
    for (dict, fragment) in [
        (base(None, Some(Value::Integer(90))), "/N"),
        (
            base(Some(Value::Integer(0)), Some(Value::Integer(90))),
            "/N",
        ),
        (base(Some(Value::Integer(2)), None), "/FOV"),
        (
            base(Some(Value::Integer(2)), Some(Value::Integer(181))),
            "/FOV",
        ),
        (
            base(Some(Value::Integer(2)), Some(Value::Integer(-1))),
            "/FOV",
        ),
        (
            base(Some(Value::Integer(2)), Some(Value::Float(f64::NAN))),
            "finite",
        ),
    ] {
        let error = parse_projection(dict).expect_err("invalid perspective projection");
        assert!(error.to_string().contains(fragment), "{error}");
    }
}

#[test]
fn projection_orthographic_defaults_and_binding() {
    let projection = parse_projection(
        Dict::new()
            .insert("Subtype", Value::Name("O".into()))
            .insert("N", Value::Integer(0))
            .insert("OS", Value::Integer(2))
            .insert("OB", Value::Name("Min".into()))
            // `/FOV` 与 `/PS` 对正交投影无意义，非法值也应被忽略。
            .insert("FOV", Value::Integer(999))
            .insert("PS", Value::Integer(0)),
    )
    .expect("orthographic projection");
    assert_eq!(projection.subtype, ThreeDProjectionType::Orthographic);
    assert_eq!(projection.near, Some(0.0));
    assert_eq!(projection.field_of_view, None);
    assert_eq!(projection.scale, None);
    assert_eq!(projection.orthographic_scale, Some(2.0));
    assert_eq!(projection.binding, Some(ThreeDProjectionBinding::Min));

    let defaults = parse_projection(Dict::new().insert("Subtype", Value::Name("O".into())))
        .expect("orthographic defaults");
    assert_eq!(defaults.near, None, "缺省 0 不物化");
    assert_eq!(defaults.orthographic_scale, None, "缺省 1 不物化");
    assert_eq!(defaults.binding, None, "缺省 Absolute 不物化");

    let unknown = parse_projection(
        Dict::new()
            .insert("Subtype", Value::Name("O".into()))
            .insert("OB", Value::Name("Z".into())),
    )
    .expect("unknown binding");
    assert_eq!(
        unknown.binding,
        Some(ThreeDProjectionBinding::Other("Z".into()))
    );

    for (dict, fragment) in [
        (
            Dict::new()
                .insert("Subtype", Value::Name("O".into()))
                .insert("N", Value::Integer(-1)),
            "/N",
        ),
        (
            Dict::new()
                .insert("Subtype", Value::Name("O".into()))
                .insert("OS", Value::Integer(0)),
            "/OS",
        ),
    ] {
        let error = parse_projection(dict).expect_err("invalid orthographic projection");
        assert!(error.to_string().contains(fragment), "{error}");
    }
}

#[test]
fn projection_rejects_invalid_subtype_and_cs() {
    for (dict, fragment) in [
        (Dict::new(), "/Subtype"),
        (
            Dict::new().insert("Subtype", Value::Name("Q".into())),
            "/Subtype",
        ),
        (
            Dict::new()
                .insert("Subtype", Value::Name("O".into()))
                .insert("CS", Value::Name("ZZ".into())),
            "/CS",
        ),
    ] {
        let error = parse_projection(dict).expect_err("invalid projection dictionary");
        assert!(error.to_string().contains(fragment), "{error}");
    }
}

#[test]
fn background_reads_and_preserves_values() {
    let background = super::background::parse_background(
        &document(),
        &Dict::new()
            .insert("Type", Value::Name("3DBG".into()))
            .insert("Subtype", Value::Name("SC".into()))
            .insert("CS", Value::Name("DeviceRGB".into()))
            .insert("C", numbers(&[0.2, 0.4, 0.6]))
            .insert("EA", Value::Bool(true)),
        "background",
    )
    .expect("background");
    assert_eq!(background.subtype, Some(ThreeDBackgroundType::SolidColor));
    assert_eq!(
        background.color_space,
        Some(Value::Name("DeviceRGB".into()))
    );
    assert_eq!(background.color, Some(numbers(&[0.2, 0.4, 0.6])));
    assert_eq!(background.entire_annotation, Some(true));

    // `/CS` 为数组或其他名字时宽容保留；`/Subtype` 未知名落 Other。
    let other = super::background::parse_background(
        &document(),
        &Dict::new()
            .insert("Subtype", Value::Name("FX".into()))
            .insert("CS", numbers(&[1.0]))
            .insert("C", Value::Name("Spot".into())),
        "background",
    )
    .expect("future background");
    assert_eq!(
        other.subtype,
        Some(ThreeDBackgroundType::Other("FX".into()))
    );
    assert_eq!(other.color_space, Some(numbers(&[1.0])));
    assert_eq!(other.color, Some(Value::Name("Spot".into())));
    assert_eq!(other.entire_annotation, None);
}

#[test]
fn background_defaults_and_errors() {
    let defaults = super::background::parse_background(&document(), &Dict::new(), "background")
        .expect("default background");
    assert_eq!(defaults, ThreeDBackground::default());

    for (dict, fragment) in [
        (
            Dict::new().insert("Type", Value::Name("Wrong".into())),
            "/Type",
        ),
        (Dict::new().insert("EA", Value::Integer(1)), "/EA"),
        (Dict::new().insert("Subtype", Value::Integer(1)), "/Subtype"),
    ] {
        let error = super::background::parse_background(&document(), &dict, "background")
            .expect_err("malformed background");
        assert!(error.to_string().contains(fragment), "{error}");
    }
}

#[test]
fn activation_reads_entries_and_unknown_names() {
    let activation = super::activation::parse_activation(
        &document(),
        &Dict::new()
            .insert("A", Value::Name("PV".into()))
            .insert("AIS", Value::Name("I".into()))
            .insert("D", Value::Name("PC".into()))
            .insert("DIS", Value::Name("L".into()))
            .insert("TB", Value::Bool(false))
            .insert("NP", Value::Bool(true)),
    )
    .expect("activation");
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
        Some(ThreeDDeactivationPolicy::PageClose)
    );
    assert_eq!(
        activation.deactivation_state,
        Some(ThreeDArtworkState::Live)
    );
    assert_eq!(activation.toolbar, Some(false));
    assert_eq!(activation.navigation_panel, Some(true));

    let unknown = super::activation::parse_activation(
        &document(),
        &Dict::new()
            .insert("A", Value::Name("X".into()))
            .insert("AIS", Value::Name("Z".into()))
            .insert("D", Value::Name("Q".into()))
            .insert("DIS", Value::Name("W".into())),
    )
    .expect("unknown names");
    assert_eq!(
        unknown.activation,
        Some(ThreeDActivationPolicy::Other("X".into()))
    );
    assert_eq!(
        unknown.activation_state,
        Some(ThreeDArtworkState::Other("Z".into()))
    );
    assert_eq!(
        unknown.deactivation,
        Some(ThreeDDeactivationPolicy::Other("Q".into()))
    );
    assert_eq!(
        unknown.deactivation_state,
        Some(ThreeDArtworkState::Other("W".into()))
    );
    assert_eq!(unknown.toolbar, None);
    assert_eq!(unknown.navigation_panel, None);
}

#[test]
fn activation_defaults_are_not_materialized() {
    let activation =
        super::activation::parse_activation(&document(), &Dict::new()).expect("empty activation");
    assert_eq!(activation, ThreeDActivation::default());
    assert!(activation.activation.is_none());
    assert!(activation.activation_state.is_none());
    assert!(activation.deactivation.is_none());
    assert!(activation.deactivation_state.is_none());
}

#[test]
fn activation_rejects_wrong_types() {
    for (dict, fragment) in [
        (Dict::new().insert("A", Value::Integer(1)), "/A"),
        (Dict::new().insert("TB", Value::Integer(1)), "/TB"),
        (Dict::new().insert("NP", Value::Name("true".into())), "/NP"),
    ] {
        let error = super::activation::parse_activation(&document(), &dict)
            .expect_err("malformed activation");
        assert!(error.to_string().contains(fragment), "{error}");
    }
}

#[test]
fn stream_reads_views_and_default_view_selection() {
    let parsed = super::stream::parse_stream(
        &document(),
        &stream("U3D")
            .insert(
                "VA",
                Value::Array(Array(vec![
                    Value::Dict(view("Front")),
                    Value::Dict(view("Back")),
                ])),
            )
            .insert("DV", Value::Integer(1))
            .insert("Resources", Value::Dict(Dict::new()))
            .insert("OnInstantiate", Value::Name("Script".into()))
            .insert(
                "AN",
                Value::Dict(
                    Dict::new()
                        .insert("Type", Value::Name("3DAnimationStyle".into()))
                        .insert("Subtype", Value::Name("Linear".into())),
                ),
            ),
        "3D stream",
    )
    .expect("stream");
    assert_eq!(parsed.view_count, Some(2));
    assert_eq!(parsed.stream.format, ThreeDStreamFormat::U3d);
    assert_eq!(parsed.stream.views.len(), 2);
    assert_eq!(
        parsed.stream.views[0].external_name,
        Value::String("Front".into())
    );
    assert_eq!(
        parsed.stream.default_view,
        Some(ThreeDViewSelection::Index(1))
    );
    assert_eq!(parsed.stream.resources, Some(Value::Dict(Dict::new())));
    assert_eq!(
        parsed.stream.on_instantiate,
        Some(Value::Name("Script".into()))
    );
    assert_eq!(
        parsed.stream.animation,
        Some(ThreeDAnimationStyle {
            subtype: Some(ThreeDAnimationStyleType::Linear),
            play_count: None,
            time_multiplier: None,
        })
    );

    let implicit = super::stream::parse_stream(
        &document(),
        &stream("U3D").insert(
            "VA",
            Value::Array(Array(vec![
                Value::Dict(view("Front")),
                Value::Dict(view("Back")),
            ])),
        ),
        "3D stream",
    )
    .expect("implicit default");
    assert_eq!(
        implicit.stream.default_view,
        Some(ThreeDViewSelection::Index(0)),
        "`/VA` 存在时 `/DV` 缺省为 0"
    );

    let no_va = super::stream::parse_stream(
        &document(),
        &stream("PRC").insert("DV", Value::Name("F".into())),
        "3D stream",
    )
    .expect("unknown format");
    assert_eq!(no_va.stream.format, ThreeDStreamFormat::Other("PRC".into()));
    assert_eq!(no_va.stream.default_view, Some(ThreeDViewSelection::First));
    assert_eq!(no_va.view_count, None);
}

#[test]
fn stream_rejects_malformed_views_and_defaults() {
    for (dict, fragment) in [
        (
            Dict::new().insert("Type", Value::Name("3D".into())),
            "/Subtype",
        ),
        (
            Dict::new()
                .insert("Type", Value::Name("Wrong".into()))
                .insert("Subtype", Value::Name("U3D".into())),
            "/Type",
        ),
        (stream("U3D").insert("VA", Value::Integer(1)), "/VA"),
        (
            stream("U3D").insert("VA", Value::Array(Array(vec![Value::Integer(1)]))),
            "/VA",
        ),
        (
            stream("U3D")
                .insert("VA", Value::Array(Array(vec![Value::Dict(view("Front"))])))
                .insert("DV", Value::Integer(1)),
            "/VA",
        ),
        (stream("U3D").insert("DV", Value::Float(1.0)), "/DV"),
    ] {
        let error = super::stream::parse_stream(&document(), &dict, "3D stream")
            .expect_err("malformed stream");
        assert!(error.to_string().contains(fragment), "{error}");
    }
}

#[test]
fn stream_view_limit_is_enforced() {
    let views: Vec<Value> = (0..=1024)
        .map(|index| Value::Dict(view(&format!("View {index}"))))
        .collect();
    let error = super::stream::parse_stream(
        &document(),
        &stream("U3D").insert("VA", Value::Array(Array(views))),
        "3D stream",
    )
    .expect_err("too many views");
    assert_eq!(
        error,
        ThreeDError::LimitExceeded("3D stream /VA views"),
        "{error}"
    );
}

#[test]
fn animation_style_reads_table301_entries() {
    let parsed = super::stream::parse_stream(
        &document(),
        &stream("U3D").insert(
            "AN",
            Value::Dict(
                Dict::new()
                    .insert("Type", Value::Name("3DAnimationStyle".into()))
                    .insert("Subtype", Value::Name("Oscillating".into()))
                    .insert("PC", Value::Integer(-1))
                    .insert("TM", Value::Float(2.5)),
            ),
        ),
        "3D stream",
    )
    .expect("animation style");
    let animation = parsed.stream.animation.expect("/AN");
    assert_eq!(
        animation.subtype,
        Some(ThreeDAnimationStyleType::Oscillating)
    );
    assert_eq!(animation.play_count, Some(-1), "负数表示无限重复");
    assert_eq!(animation.time_multiplier, Some(2.5));
}

#[test]
fn animation_style_defaults_and_unknown_subtype() {
    let empty = super::stream::parse_stream(
        &document(),
        &stream("U3D").insert("AN", Value::Dict(Dict::new())),
        "3D stream",
    )
    .expect("empty animation style");
    assert_eq!(
        empty.stream.animation,
        Some(ThreeDAnimationStyle::default()),
        "缺省 None/0/1 不物化"
    );

    let absent =
        super::stream::parse_stream(&document(), &stream("U3D"), "3D stream").expect("absent /AN");
    assert_eq!(absent.stream.animation, None);

    let unknown = super::stream::parse_stream(
        &document(),
        &stream("U3D").insert(
            "AN",
            Value::Dict(Dict::new().insert("Subtype", Value::Name("Bounce".into()))),
        ),
        "3D stream",
    )
    .expect("unknown style");
    assert_eq!(
        unknown.stream.animation.and_then(|style| style.subtype),
        Some(ThreeDAnimationStyleType::Other("Bounce".into())),
        "未识别样式原样保留，规范要求按 None 处理"
    );
}

#[test]
fn malformed_animation_styles_are_typed_errors() {
    for (value, fragment) in [
        (Value::Integer(1), "is not a dictionary"),
        (
            Value::Dict(Dict::new().insert("Type", Value::Name("Other".into()))),
            "/Type",
        ),
        (
            Value::Dict(Dict::new().insert("Subtype", Value::Integer(1))),
            "/Subtype",
        ),
        (
            Value::Dict(Dict::new().insert("PC", Value::Float(1.0))),
            "/PC",
        ),
        (
            Value::Dict(Dict::new().insert("PC", Value::String("3".into()))),
            "/PC",
        ),
        (
            Value::Dict(Dict::new().insert("TM", Value::Integer(0))),
            "/TM",
        ),
        (
            Value::Dict(Dict::new().insert("TM", Value::Float(-1.0))),
            "/TM",
        ),
    ] {
        let error = super::stream::parse_stream(
            &document(),
            &stream("U3D").insert("AN", value),
            "3D stream",
        )
        .expect_err("malformed animation style");
        assert!(error.to_string().contains(fragment), "{error}");
    }
}
