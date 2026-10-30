//! 3D 视图负载字典（Table 307–312）读端测试：渲染模式、光照方案、截面与
//! 节点，含缺省不物化与畸形输入。

use crate::core::types::{Array, Dict, Value};

use super::super::color::ThreeDColor;
use super::super::cross_section::parse_cross_section;
use super::super::lighting::{ThreeDLightingSchemeType, parse_lighting_scheme};
use super::super::node::parse_node;
use super::super::render_mode::{ThreeDFaceColor, ThreeDRenderModeType, parse_render_mode};
use super::super::view::parse_view;
use super::{document, numbers, view};

fn rgb(values: [f64; 3]) -> Value {
    let mut items = vec![Value::Name("DeviceRGB".into())];
    items.extend(values.iter().map(|value| Value::Float(*value)));
    Value::Array(Array(items))
}

#[test]
fn render_mode_reads_color_opacity_and_crease() {
    let parsed = parse_render_mode(
        &document(),
        &Dict::new()
            .insert("Type", Value::Name("3DRenderMode".into()))
            .insert("Subtype", Value::Name("ShadedIllustration".into()))
            .insert("AC", rgb([0.25, 0.5, 0.75]))
            .insert("FC", rgb([1.0, 0.5, 0.0]))
            .insert("O", Value::Float(0.7))
            .insert("CV", Value::Integer(30)),
        "render mode",
    )
    .expect("render mode");
    assert_eq!(parsed.subtype, ThreeDRenderModeType::ShadedIllustration);
    assert_eq!(
        parsed.auxiliary_color,
        Some(ThreeDColor::rgb([0.25, 0.5, 0.75]))
    );
    assert_eq!(
        parsed.face_color,
        Some(ThreeDFaceColor::DeviceRgb([1.0, 0.5, 0.0]))
    );
    assert_eq!(parsed.opacity, Some(0.7));
    assert_eq!(parsed.crease_value, Some(30.0));
}

#[test]
fn render_mode_preserves_unknown_subtype_and_colors() {
    let background = parse_render_mode(
        &document(),
        &Dict::new()
            .insert("Subtype", Value::Name("Illustration".into()))
            .insert("FC", Value::Name("BG".into())),
        "render mode",
    )
    .expect("background face color");
    assert_eq!(background.face_color, Some(ThreeDFaceColor::Background));
    assert_eq!(background.auxiliary_color, None);

    let unknown_color = Value::Array(Array(vec![
        Value::Name("CalRGB".into()),
        Value::Float(1.0),
        Value::Float(0.5),
        Value::Float(0.25),
    ]));
    let unknown = parse_render_mode(
        &document(),
        &Dict::new()
            .insert("Subtype", Value::Name("FutureMode".into()))
            .insert("AC", unknown_color.clone())
            .insert("FC", Value::Name("Spot".into())),
        "render mode",
    )
    .expect("future render mode");
    assert_eq!(
        unknown.subtype,
        ThreeDRenderModeType::Other("FutureMode".into())
    );
    assert_eq!(
        unknown.auxiliary_color,
        Some(ThreeDColor::Other(unknown_color))
    );
    assert_eq!(
        unknown.face_color,
        Some(ThreeDFaceColor::Other(Value::Name("Spot".into())))
    );
}

#[test]
fn lighting_scheme_reads_table310_names() {
    for (name, expected) in [
        ("Artwork", ThreeDLightingSchemeType::Artwork),
        ("None", ThreeDLightingSchemeType::None),
        ("White", ThreeDLightingSchemeType::White),
        ("Day", ThreeDLightingSchemeType::Day),
        ("Night", ThreeDLightingSchemeType::Night),
        ("Hard", ThreeDLightingSchemeType::Hard),
        ("Primary", ThreeDLightingSchemeType::Primary),
        ("Blue", ThreeDLightingSchemeType::Blue),
        ("Red", ThreeDLightingSchemeType::Red),
        ("Cube", ThreeDLightingSchemeType::Cube),
        ("CAD", ThreeDLightingSchemeType::Cad),
        ("Headlamp", ThreeDLightingSchemeType::Headlamp),
    ] {
        let parsed = parse_lighting_scheme(
            &document(),
            &Dict::new().insert("Subtype", Value::Name(name.into())),
            "lighting",
        )
        .expect("lighting scheme");
        assert_eq!(parsed.subtype, expected, "{name}");
    }

    let unknown = parse_lighting_scheme(
        &document(),
        &Dict::new()
            .insert("Type", Value::Name("3DLightingScheme".into()))
            .insert("Subtype", Value::Name("Studio".into())),
        "lighting",
    )
    .expect("unknown lighting");
    assert_eq!(
        unknown.subtype,
        ThreeDLightingSchemeType::Other("Studio".into())
    );
}

#[test]
fn cross_section_reads_orientation_opacity_and_colors() {
    let parsed = parse_cross_section(
        &document(),
        &Dict::new()
            .insert("Type", Value::Name("3DCrossSection".into()))
            .insert("C", numbers(&[1.0, 2.0, 3.0]))
            .insert(
                "O",
                Value::Array(Array(vec![
                    Value::Null,
                    Value::Integer(-30),
                    Value::Integer(30),
                ])),
            )
            .insert("PO", Value::Float(0.25))
            .insert("PC", rgb([0.5, 0.5, 0.5]))
            .insert("IV", Value::Bool(true))
            .insert("IC", rgb([0.0, 1.0, 0.0])),
        "cross section",
    )
    .expect("cross section");
    assert_eq!(parsed.center, Some([1.0, 2.0, 3.0]));
    assert_eq!(parsed.orientation, Some([None, Some(-30.0), Some(30.0)]));
    assert_eq!(parsed.plane_opacity, Some(0.25));
    assert_eq!(parsed.plane_color, Some(ThreeDColor::rgb([0.5, 0.5, 0.5])));
    assert_eq!(parsed.intersection_visible, Some(true));
    assert_eq!(
        parsed.intersection_color,
        Some(ThreeDColor::rgb([0.0, 1.0, 0.0]))
    );
}

#[test]
fn node_reads_name_opacity_visibility_and_matrix() {
    let parsed = parse_node(
        &document(),
        &Dict::new()
            .insert("Type", Value::Name("3DNode".into()))
            .insert("N", Value::String("Wheel".into()))
            .insert("O", Value::Float(0.8))
            .insert("V", Value::Bool(false))
            .insert(
                "M",
                Value::Array(Array((0..12).map(Value::Integer).collect())),
            ),
        "node",
    )
    .expect("node");
    assert_eq!(parsed.name, Value::String("Wheel".into()));
    assert_eq!(parsed.opacity, Some(0.8));
    assert_eq!(parsed.visible, Some(false));
    assert_eq!(
        parsed.matrix,
        Some([0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0])
    );
}

#[test]
fn payload_defaults_are_not_materialized() {
    let parsed = parse_view(
        &document(),
        &view("Front")
            .insert(
                "RM",
                Value::Dict(Dict::new().insert("Subtype", Value::Name("Solid".into()))),
            )
            .insert(
                "LS",
                Value::Dict(Dict::new().insert("Subtype", Value::Name("Day".into()))),
            )
            .insert("SA", Value::Array(Array(vec![Value::Dict(Dict::new())])))
            .insert(
                "NA",
                Value::Array(Array(vec![Value::Dict(
                    Dict::new().insert("N", Value::String("Root".into())),
                )])),
            ),
        "view",
    )
    .expect("defaults");
    let render_mode = parsed.render_mode.expect("/RM");
    assert_eq!(render_mode.subtype, ThreeDRenderModeType::Solid);
    assert_eq!(render_mode.auxiliary_color, None);
    assert_eq!(render_mode.face_color, None);
    assert_eq!(render_mode.opacity, None, "缺省 0.5 不物化");
    assert_eq!(render_mode.crease_value, None, "缺省 45 不物化");
    assert_eq!(
        parsed.lighting.map(|lighting| lighting.subtype),
        Some(ThreeDLightingSchemeType::Day)
    );
    let cross_sections = parsed.cross_sections.expect("/SA");
    assert_eq!(cross_sections.len(), 1);
    assert_eq!(cross_sections[0].center, None, "缺省 [0 0 0] 不物化");
    assert_eq!(
        cross_sections[0].orientation, None,
        "缺省 [null 0 0] 不物化"
    );
    assert_eq!(cross_sections[0].plane_opacity, None);
    assert_eq!(cross_sections[0].plane_color, None);
    assert_eq!(cross_sections[0].intersection_visible, None);
    assert_eq!(cross_sections[0].intersection_color, None);
    let nodes = parsed.nodes.expect("/NA");
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].name, Value::String("Root".into()));
    assert_eq!(nodes[0].opacity, None, "缺省表示继承");
    assert_eq!(nodes[0].visible, None, "缺省表示继承");
    assert_eq!(nodes[0].matrix, None);

    let empty = parse_view(
        &document(),
        &view("Front")
            .insert("SA", Value::Array(Array(Vec::new())))
            .insert("NA", Value::Array(Array(Vec::new()))),
        "view",
    )
    .expect("empty payload arrays");
    assert_eq!(empty.cross_sections, Some(Vec::new()));
    assert_eq!(empty.nodes, Some(Vec::new()));
}

#[test]
fn malformed_payloads_are_typed_errors() {
    for (dict, fragment) in [
        (
            view("Front").insert(
                "RM",
                Value::Dict(Dict::new().insert("Type", Value::Name("3DRenderMode".into()))),
            ),
            "/Subtype",
        ),
        (
            view("Front").insert("RM", Value::Name("Solid".into())),
            "/RM",
        ),
        (view("Front").insert("LS", Value::Integer(1)), "/LS"),
        (
            view("Front").insert("SA", Value::Array(Array(vec![Value::Integer(1)]))),
            "not a dictionary",
        ),
        (
            view("Front").insert(
                "NA",
                Value::Array(Array(vec![Value::Dict(
                    Dict::new().insert("Type", Value::Name("3DNode".into())),
                )])),
            ),
            "/N",
        ),
    ] {
        let error = parse_view(&document(), &dict, "view").expect_err("malformed view payload");
        assert!(error.to_string().contains(fragment), "{error}");
    }

    for (orientation, fragment) in [
        (
            Value::Array(Array(vec![
                Value::Integer(0),
                Value::Integer(1),
                Value::Integer(2),
            ])),
            "exactly one null",
        ),
        (
            Value::Array(Array(vec![Value::Null, Value::Null, Value::Integer(1)])),
            "exactly one null",
        ),
        (
            Value::Array(Array(vec![Value::Null, Value::Null])),
            "3 entries",
        ),
    ] {
        let error = parse_cross_section(
            &document(),
            &Dict::new().insert("O", orientation),
            "cross section",
        )
        .expect_err("invalid orientation");
        assert!(error.to_string().contains(fragment), "{error}");
    }

    for (dict, fragment) in [
        (Dict::new().insert("PO", Value::Float(1.5)), "/PO"),
        (Dict::new().insert("PO", Value::Integer(-1)), "/PO"),
        (
            Dict::new().insert("C", Value::Array(Array(vec![Value::Integer(1)]))),
            "3 numbers",
        ),
    ] {
        let error = parse_cross_section(&document(), &dict, "cross section")
            .expect_err("malformed cross section");
        assert!(error.to_string().contains(fragment), "{error}");
    }

    for (dict, fragment) in [
        (Dict::new(), "/N"),
        (Dict::new().insert("N", Value::Integer(1)), "/N"),
        (
            Dict::new()
                .insert("N", Value::String("Node".into()))
                .insert("O", Value::Float(1.5)),
            "/O",
        ),
        (
            Dict::new()
                .insert("N", Value::String("Node".into()))
                .insert("M", Value::Array(Array(vec![Value::Integer(0)]))),
            "12 numbers",
        ),
    ] {
        let error = parse_node(&document(), &dict, "node").expect_err("malformed node");
        assert!(error.to_string().contains(fragment), "{error}");
    }
}
