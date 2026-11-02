//! PatternType 1 平铺图案与 PatternType 2 着色图案：条目解析、无色图案
//! 基色空间约束、步进校验与内联/间接引用/命名 `/Shading` 形态。

use super::super::*;
use super::support::{array, axial_dict, dict, doc_with_objects, name, reals};
use crate::core::reader::color_space::ColorSpaceModel;
use crate::core::types::{Dict, Document, StreamData, Value};

#[test]
fn tiling_pattern_reads_steps_bbox_resources_and_content() {
    let doc = Document::new();
    let stream = Value::Stream(
        Dict::new()
            .insert("Type", name("Pattern"))
            .insert("PatternType", Value::Integer(1))
            .insert("PaintType", Value::Integer(1))
            .insert("TilingType", Value::Integer(3))
            .insert("BBox", reals(&[0.0, 0.0, 4.0, 4.0]))
            .insert("XStep", Value::Float(4.0))
            .insert("YStep", Value::Float(4.0))
            .insert("Matrix", reals(&[1.0, 0.0, 0.0, 1.0, 2.0, 3.0]))
            .insert("Resources", dict(&[("ProcSet", array(vec![name("PDF")]))])),
        StreamData::Owned(b"0.5 g S\n".to_vec()),
    );
    let PatternModel::Tiling(pattern) = doc.pattern(&stream).unwrap() else {
        panic!("expected tiling pattern");
    };
    assert_eq!(pattern.paint_type, PaintType::Colored);
    assert_eq!(pattern.tiling_type, TilingType::AdaptiveVariableSpacing);
    assert_eq!(pattern.bbox, [0.0, 0.0, 4.0, 4.0]);
    assert_eq!(pattern.x_step, 4.0);
    assert_eq!(pattern.matrix, Some([1.0, 0.0, 0.0, 1.0, 2.0, 3.0]));
    assert!(pattern.resources.is_some());
    assert_eq!(pattern.color_space, None);
    assert_eq!(pattern.content, b"0.5 g S\n");
}

#[test]
fn uncolored_tiling_pattern_requires_and_parses_base_colour_space() {
    let separation = array(vec![
        name("Separation"),
        name("Spot1"),
        name("DeviceCMYK"),
        Value::Null,
    ]);
    let doc = Document::new();
    let stream = Value::Stream(
        Dict::new()
            .insert("PatternType", Value::Integer(1))
            .insert("PaintType", Value::Integer(2))
            .insert("TilingType", Value::Integer(1))
            .insert("BBox", reals(&[0.0, 0.0, 1.0, 1.0]))
            .insert("XStep", Value::Float(1.0))
            .insert("YStep", Value::Float(1.0))
            .insert("ColorSpace", separation),
        StreamData::Owned(b"0 0 m 1 1 l S".to_vec()),
    );
    let PatternModel::Tiling(pattern) = doc.pattern(&stream).unwrap() else {
        panic!("expected tiling pattern");
    };
    assert_eq!(pattern.paint_type, PaintType::Uncolored);
    let Some(ColorSpaceModel::Separation(space)) = &pattern.color_space else {
        panic!("expected separation base color space");
    };
    assert_eq!(space.colorant, "Spot1");

    let without_base = Value::Stream(
        Dict::new()
            .insert("PaintType", Value::Integer(2))
            .insert("TilingType", Value::Integer(1))
            .insert("BBox", reals(&[0.0, 0.0, 1.0, 1.0]))
            .insert("XStep", Value::Float(1.0))
            .insert("YStep", Value::Float(1.0)),
        StreamData::Owned(b"x".to_vec()),
    );
    assert!(matches!(
        doc.pattern(&without_base),
        Err(ShadingError::Malformed(message)) if message.contains("ColorSpace")
    ));

    let zero_step = Value::Stream(
        Dict::new()
            .insert("PaintType", Value::Integer(1))
            .insert("TilingType", Value::Integer(1))
            .insert("BBox", reals(&[0.0, 0.0, 1.0, 1.0]))
            .insert("XStep", Value::Float(0.0))
            .insert("YStep", Value::Float(1.0)),
        StreamData::Owned(b"x".to_vec()),
    );
    assert!(matches!(
        doc.pattern(&zero_step),
        Err(ShadingError::Malformed(message)) if message.contains("XStep")
    ));
}

#[test]
fn shading_pattern_accepts_inline_and_referenced_shadings() {
    let doc = doc_with_objects(vec![(12, axial_dict(&[]))]);
    let inline = doc
        .pattern(&dict(&[
            ("Type", name("Pattern")),
            ("PatternType", Value::Integer(2)),
            ("Shading", axial_dict(&[])),
        ]))
        .unwrap();
    let PatternModel::Shading(pattern) = &inline else {
        panic!("expected shading pattern");
    };
    assert_eq!(pattern.shading.shading_type(), 2);

    let referenced = doc
        .pattern(&dict(&[
            ("PatternType", Value::Integer(2)),
            ("Shading", Value::Ref(12, 0)),
            ("Matrix", reals(&[2.0, 0.0, 0.0, 2.0, 0.0, 0.0])),
        ]))
        .unwrap();
    let PatternModel::Shading(pattern) = referenced else {
        panic!("expected shading pattern");
    };
    assert_eq!(pattern.matrix, Some([2.0, 0.0, 0.0, 2.0, 0.0, 0.0]));

    let named_without_resources = doc.pattern(&dict(&[
        ("PatternType", Value::Integer(2)),
        ("Shading", name("Sh1")),
    ]));
    assert!(matches!(
        named_without_resources,
        Err(ShadingError::Malformed(message)) if message.contains("resource dictionary")
    ));

    let bad_type = doc.pattern(&dict(&[("PatternType", Value::Integer(4))]));
    assert!(matches!(
        bad_type,
        Err(ShadingError::Unsupported(message)) if message.contains("PatternType 4")
    ));
}
