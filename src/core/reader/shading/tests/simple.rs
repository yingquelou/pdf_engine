//! Type 1–3 简单渐变：`/Coords`、`/Domain`、`/Extend`、`/Matrix` 与函数
//! 维数校验及共有可选条目的规范缺省与拒绝路径。

use super::super::*;
use super::support::{
    axial_dict, dict, doc_with_objects, exponential, name, reals, two_input_calculator,
};
use crate::core::reader::color_space::ColorSpaceModel;
use crate::core::types::{Array, Document, Value};

#[test]
fn axial_shading_parses_coords_with_spec_defaults() {
    let doc = Document::new();
    let model = doc.shading(&axial_dict(&[])).unwrap();
    let ShadingModel::Axial(axial) = &model else {
        panic!("expected axial shading");
    };
    assert_eq!(model.shading_type(), 2);
    assert_eq!(axial.coords, [0.0, 0.0, 1.0, 1.0]);
    assert_eq!(axial.domain, vec![(0.0, 1.0)]);
    assert_eq!(axial.extend, [false, false]);
    assert_eq!(axial.color_space, ColorSpaceModel::DeviceRGB);
    assert_eq!(axial.function.function_type(), 2);
    assert_eq!(axial.options.background, None);
}

#[test]
fn radial_shading_reads_six_coords_and_rejects_negative_radii() {
    let doc = Document::new();
    let model = doc
        .shading(&dict(&[
            ("ShadingType", Value::Integer(3)),
            ("ColorSpace", name("DeviceGray")),
            ("Coords", reals(&[0.0, 0.0, 1.0, 0.0, 0.0, 2.0])),
            ("Function", exponential(&[0.0, 1.0], &[0.0], &[1.0])),
            (
                "Extend",
                Value::Array(Array(vec![Value::Bool(true), Value::Bool(false)])),
            ),
            ("Domain", reals(&[0.25, 0.75])),
        ]))
        .unwrap();
    let ShadingModel::Radial(radial) = model else {
        panic!("expected radial shading");
    };
    assert_eq!(radial.coords, [0.0, 0.0, 1.0, 0.0, 0.0, 2.0]);
    assert_eq!(radial.extend, [true, false]);
    assert_eq!(radial.domain, vec![(0.25, 0.75)]);

    let negative = doc.shading(&dict(&[
        ("ShadingType", Value::Integer(3)),
        ("ColorSpace", name("DeviceGray")),
        ("Coords", reals(&[0.0, 0.0, -1.0, 0.0, 0.0, 2.0])),
        ("Function", exponential(&[0.0, 1.0], &[0.0], &[1.0])),
    ]));
    assert!(matches!(
        negative,
        Err(ShadingError::Malformed(message)) if message.contains("radii")
    ));
}

#[test]
fn function_shading_requires_two_input_colour_function() {
    let doc = doc_with_objects(vec![(5, two_input_calculator())]);
    let model = doc
        .shading(&dict(&[
            ("ShadingType", Value::Integer(1)),
            ("ColorSpace", name("DeviceRGB")),
            ("Function", Value::Ref(5, 0)),
            ("Matrix", reals(&[2.0, 0.0, 0.0, 2.0, 0.0, 0.0])),
        ]))
        .unwrap();
    let ShadingModel::Function(function) = model else {
        panic!("expected function shading");
    };
    assert_eq!(function.matrix, Some([2.0, 0.0, 0.0, 2.0, 0.0, 0.0]));
    assert_eq!(function.function.function_type(), 4);

    let wrong_inputs = doc.shading(&dict(&[
        ("ShadingType", Value::Integer(1)),
        ("ColorSpace", name("DeviceRGB")),
        (
            "Function",
            exponential(&[0.0, 1.0], &[0.0, 0.0, 0.0], &[1.0, 1.0, 1.0]),
        ),
    ]));
    assert!(matches!(
        wrong_inputs,
        Err(ShadingError::Malformed(message)) if message.contains("2 input")
    ));
}

#[test]
fn shading_function_output_count_must_match_colour_components() {
    let doc = Document::new();
    let result = doc.shading(&dict(&[
        ("ShadingType", Value::Integer(2)),
        ("ColorSpace", name("DeviceRGB")),
        ("Coords", reals(&[0.0, 0.0, 1.0, 0.0])),
        (
            "Function",
            exponential(&[0.0, 1.0], &[0.0, 0.0], &[1.0, 1.0]),
        ),
    ]));
    assert!(matches!(
        result,
        Err(ShadingError::Malformed(message)) if message.contains("outputs")
    ));
}

#[test]
fn shading_common_options_are_validated() {
    let doc = Document::new();
    let model = doc
        .shading(&axial_dict(&[
            ("Background", reals(&[0.1, 0.2, 0.3])),
            ("BBox", reals(&[0.0, 0.0, 10.0, 20.0])),
            ("AntiAlias", Value::Bool(true)),
        ]))
        .unwrap();
    let options = model.options();
    assert_eq!(options.background, Some(vec![0.1, 0.2, 0.3]));
    assert_eq!(options.bbox, Some([0.0, 0.0, 10.0, 20.0]));
    assert_eq!(options.anti_alias, Some(true));

    let bad_background = doc.shading(&axial_dict(&[("Background", reals(&[0.1, 0.2]))]));
    assert!(matches!(
        bad_background,
        Err(ShadingError::Malformed(message)) if message.contains("Background")
    ));
    let bad_bbox = doc.shading(&axial_dict(&[("BBox", reals(&[5.0, 0.0, 4.0, 1.0]))]));
    assert!(matches!(
        bad_bbox,
        Err(ShadingError::Malformed(message)) if message.contains("BBox")
    ));
}
