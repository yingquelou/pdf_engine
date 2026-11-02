//! 类型化错误：不支持的着色/图案类型、畸形结构的 `Malformed` 拒绝路径
//! 与错误 `Display`、访问器及色空间分量语义。

use super::super::*;
use super::support::{axial_dict, dict, doc_with_objects, name, reals, two_input_calculator};
use crate::core::reader::color_space::{ColorSpaceModel, IndexedSpace, SeparationSpace};
use crate::core::types::{Dict, Document, StreamData, Value};

#[test]
fn unsupported_shading_types_and_pattern_colour_spaces_are_typed_errors() {
    let doc = Document::new();
    assert!(matches!(
        doc.shading(&dict(&[
            ("ShadingType", Value::Integer(8)),
            ("ColorSpace", name("DeviceRGB")),
        ])),
        Err(ShadingError::Unsupported(message)) if message.contains("ShadingType 8")
    ));
    assert!(matches!(
        doc.shading(&dict(&[
            ("ShadingType", Value::Integer(2)),
            ("ColorSpace", name("Pattern")),
        ])),
        Err(ShadingError::Unsupported(message)) if message.contains("Pattern")
    ));
    assert!(matches!(
        doc.shading(&dict(&[("ColorSpace", name("DeviceRGB"))])),
        Err(ShadingError::Malformed(message)) if message.contains("ShadingType")
    ));
}
#[test]
fn malformed_shading_structures_return_typed_errors() {
    let doc = Document::new();
    // Type 4 modelled as a plain dictionary (no stream data).
    assert!(matches!(
        doc.shading(&dict(&[
            ("ShadingType", Value::Integer(4)),
            ("ColorSpace", name("DeviceGray")),
            ("BitsPerCoordinate", Value::Integer(8)),
            ("BitsPerComponent", Value::Integer(8)),
            ("BitsPerFlag", Value::Integer(8)),
            ("Decode", reals(&[0.0, 1.0, 0.0, 1.0, 0.0, 1.0])),
        ])),
        Err(ShadingError::Malformed(message)) if message.contains("stream data")
    ));
    assert!(matches!(
        doc.shading(&name("Shading")),
        Err(ShadingError::Malformed(message)) if message.contains("dictionary or stream")
    ));
    assert!(matches!(
        doc.shading(&axial_dict(&[("Function", name("bad"))])),
        Err(ShadingError::Malformed(message)) if message.contains("function must be")
    ));
    // Mesh function taking two inputs is rejected.
    let doc = doc_with_objects(vec![(6, two_input_calculator())]);
    let stream = Value::Stream(
        Dict::new()
            .insert("ShadingType", Value::Integer(5))
            .insert("ColorSpace", name("DeviceRGB"))
            .insert("BitsPerCoordinate", Value::Integer(8))
            .insert("BitsPerComponent", Value::Integer(8))
            .insert("VerticesPerRow", Value::Integer(2))
            .insert("Function", Value::Ref(6, 0))
            .insert("Decode", reals(&[0.0, 1.0, 0.0, 1.0, 0.0, 1.0])),
        StreamData::Owned(vec![0u8; 6]),
    );
    assert!(matches!(
        doc.shading(&stream),
        Err(ShadingError::Malformed(message)) if message.contains("single input")
    ));
}

#[test]
fn error_display_and_shading_accessors_are_available() {
    let doc = Document::new();
    let model = doc.shading(&axial_dict(&[])).unwrap();
    assert_eq!(model.color_space(), &ColorSpaceModel::DeviceRGB);
    let error = ShadingError::Unsupported("nope".into());
    assert_eq!(error.to_string(), "unsupported shading: nope");
    // Sanity-check the color space models referenced above still expose the
    // documented component semantics used by the mesh decoder.
    let indexed = ColorSpaceModel::Indexed(IndexedSpace {
        base: Box::new(ColorSpaceModel::DeviceRGB),
        hival: 1,
        lookup: vec![0, 0, 0, 255, 255, 255],
    });
    assert_eq!(indexed.component_count(), Some(1));
    let separation = ColorSpaceModel::Separation(SeparationSpace {
        colorant: "Spot".into(),
        alternate: Box::new(ColorSpaceModel::DeviceCMYK),
        tint: None,
    });
    assert_eq!(separation.component_count(), Some(1));
}
