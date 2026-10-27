//! Shading/Pattern reader model tests.

use super::*;
use crate::core::reader::color_space::{ColorSpaceModel, IndexedSpace, SeparationSpace};
use crate::core::types::{Array, Object, StreamData, Trailer, Value};

fn doc_with_objects(objects: Vec<(i64, Value)>) -> Document {
    let doc = Document::new();
    for (num, value) in objects {
        doc.set_object(Object::new(num, 0, value));
    }
    doc
}

fn name(text: &str) -> Value {
    Value::Name(text.into())
}

fn array(items: Vec<Value>) -> Value {
    Value::Array(Array(items))
}

fn reals(values: &[f64]) -> Value {
    Value::Array(Array(
        values.iter().map(|v| Value::Float(*v)).collect::<Vec<_>>(),
    ))
}

fn dict(items: &[(&str, Value)]) -> Value {
    let mut dict = Dict::new();
    for (key, value) in items {
        dict = dict.insert(*key, value.clone());
    }
    Value::Dict(dict)
}

fn exponential(domain: &[f64], c0: &[f64], c1: &[f64]) -> Value {
    dict(&[
        ("FunctionType", Value::Integer(2)),
        ("Domain", reals(domain)),
        ("N", Value::Integer(1)),
        ("C0", reals(c0)),
        ("C1", reals(c1)),
    ])
}

fn two_input_calculator() -> Value {
    Value::Stream(
        Dict::new()
            .insert("FunctionType", Value::Integer(4))
            .insert("Domain", reals(&[0.0, 1.0, 0.0, 1.0]))
            .insert("Range", reals(&[0.0, 1.0, 0.0, 1.0, 0.0, 1.0])),
        StreamData::Owned(b"0.5 0.5 0.5".to_vec()),
    )
}

fn axial_dict(extra: &[(&str, Value)]) -> Value {
    let mut items = vec![
        ("ShadingType", Value::Integer(2)),
        ("ColorSpace", name("DeviceRGB")),
        ("Coords", reals(&[0.0, 0.0, 1.0, 1.0])),
        (
            "Function",
            exponential(&[0.0, 1.0], &[0.0, 0.0, 0.0], &[1.0, 1.0, 1.0]),
        ),
    ];
    items.extend(extra.iter().map(|(k, v)| (*k, v.clone())));
    dict(&items)
}

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
fn free_form_mesh_decodes_flag_coordinate_and_colour_records() {
    let doc = Document::new();
    // flag(8) x(16) y(16) rgb(8*3) = 64 bits per record.
    let mut data = Vec::new();
    data.extend_from_slice(&[0, 0xFF, 0xFF, 0x00, 0x00, 0xFF, 0x00, 0x80]);
    data.extend_from_slice(&[1, 0x00, 0x00, 0xFF, 0xFF, 0x00, 0x00, 0x00]);
    data.extend_from_slice(&[2, 0x00, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0xFF]);
    let stream = Value::Stream(
        Dict::new()
            .insert("ShadingType", Value::Integer(4))
            .insert("ColorSpace", name("DeviceRGB"))
            .insert("BitsPerCoordinate", Value::Integer(16))
            .insert("BitsPerComponent", Value::Integer(8))
            .insert("BitsPerFlag", Value::Integer(8))
            .insert(
                "Decode",
                reals(&[0.0, 100.0, 0.0, 200.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0]),
            ),
        StreamData::Owned(data),
    );
    let model = doc.shading(&stream).unwrap();
    let ShadingModel::FreeFormMesh(mesh) = model else {
        panic!("expected free form mesh");
    };
    assert_eq!(mesh.vertices.len(), 3);
    assert_eq!(mesh.bits_per_coordinate, 16);
    assert_eq!(mesh.bits_per_flag, 8);
    assert_eq!(mesh.vertices[0].flag, 0);
    assert_eq!(mesh.vertices[0].x, 100.0);
    assert_eq!(mesh.vertices[0].y, 0.0);
    assert_eq!(mesh.vertices[0].components, vec![1.0, 0.0, 128.0 / 255.0]);
    assert_eq!(mesh.vertices[1].flag, 1);
    assert_eq!(mesh.vertices[1].x, 0.0);
    assert_eq!(mesh.vertices[1].y, 200.0);
    assert_eq!(mesh.vertices[2].components, vec![1.0, 1.0, 1.0]);
}

#[test]
fn mesh_vertex_groups_must_be_complete() {
    let doc = Document::new();
    // Type 4 with only two of three required records.
    let stream = Value::Stream(
        Dict::new()
            .insert("ShadingType", Value::Integer(4))
            .insert("ColorSpace", name("DeviceGray"))
            .insert("BitsPerCoordinate", Value::Integer(8))
            .insert("BitsPerComponent", Value::Integer(8))
            .insert("BitsPerFlag", Value::Integer(8))
            .insert("Decode", reals(&[0.0, 1.0, 0.0, 1.0, 0.0, 1.0])),
        StreamData::Owned(vec![0, 10, 20, 0, 30, 40]),
    );
    assert!(matches!(
        doc.shading(&stream),
        Err(ShadingError::Malformed(message)) if message.contains("groups of 3")
    ));

    // Type 5 rows must divide evenly.
    let lattice = Value::Stream(
        Dict::new()
            .insert("ShadingType", Value::Integer(5))
            .insert("ColorSpace", name("DeviceGray"))
            .insert("BitsPerCoordinate", Value::Integer(8))
            .insert("BitsPerComponent", Value::Integer(8))
            .insert("VerticesPerRow", Value::Integer(2))
            .insert("Decode", reals(&[0.0, 1.0, 0.0, 1.0, 0.0, 1.0])),
        StreamData::Owned(vec![0; 9]),
    );
    assert!(matches!(
        doc.shading(&lattice),
        Err(ShadingError::Malformed(message)) if message.contains("complete rows")
    ));
}

#[test]
fn lattice_mesh_decodes_rows_and_type_4_requires_decode() {
    let doc = Document::new();
    let lattice = Value::Stream(
        Dict::new()
            .insert("ShadingType", Value::Integer(5))
            .insert("ColorSpace", name("DeviceGray"))
            .insert("BitsPerCoordinate", Value::Integer(8))
            .insert("BitsPerComponent", Value::Integer(8))
            .insert("VerticesPerRow", Value::Integer(2))
            .insert("Decode", reals(&[0.0, 10.0, 0.0, 10.0, 0.0, 1.0])),
        StreamData::Owned(vec![10, 20, 30, 60, 30, 255]),
    );
    let model = doc.shading(&lattice).unwrap();
    let ShadingModel::LatticeMesh(mesh) = model else {
        panic!("expected lattice mesh");
    };
    assert_eq!(mesh.vertices_per_row, 2);
    assert_eq!(mesh.vertices.len(), 2);
    assert_eq!(mesh.vertices[0].flag, 0);
    assert_eq!(mesh.vertices[0].x, 10.0 / 255.0 * 10.0);
    assert_eq!(mesh.vertices[0].y, 20.0 / 255.0 * 10.0);
    assert_eq!(mesh.vertices[1].components, vec![1.0]);

    let missing_decode = Value::Stream(
        Dict::new()
            .insert("ShadingType", Value::Integer(4))
            .insert("ColorSpace", name("DeviceGray"))
            .insert("BitsPerCoordinate", Value::Integer(8))
            .insert("BitsPerComponent", Value::Integer(8))
            .insert("BitsPerFlag", Value::Integer(8)),
        StreamData::Owned(vec![0; 18]),
    );
    assert!(matches!(
        doc.shading(&missing_decode),
        Err(ShadingError::Malformed(message)) if message.contains("Decode")
    ));
}

#[test]
fn patch_meshes_default_decode_and_group_records() {
    let doc = Document::new();
    // Type 6: flag(8) x(16) y(16) rgb(24) = 64 bits, 12 records per patch.
    let coons = Value::Stream(
        Dict::new()
            .insert("ShadingType", Value::Integer(6))
            .insert("ColorSpace", name("DeviceRGB"))
            .insert("BitsPerCoordinate", Value::Integer(16))
            .insert("BitsPerComponent", Value::Integer(8))
            .insert("BitsPerFlag", Value::Integer(8)),
        StreamData::Owned(vec![0u8; 96]),
    );
    let model = doc.shading(&coons).unwrap();
    let ShadingModel::CoonsPatch(patch) = model else {
        panic!("expected coons patch mesh");
    };
    assert_eq!(patch.patches.len(), 1);
    assert_eq!(patch.patches[0].len(), 12);
    assert_eq!(patch.decode, vec![(0.0, 1.0); 5]);
    assert_eq!(patch.patches[0][0].x, 0.0);

    let tensor = Value::Stream(
        Dict::new()
            .insert("ShadingType", Value::Integer(7))
            .insert("ColorSpace", name("DeviceGray"))
            .insert("BitsPerCoordinate", Value::Integer(8))
            .insert("BitsPerComponent", Value::Integer(8))
            .insert("BitsPerFlag", Value::Integer(8)),
        StreamData::Owned(vec![0xFFu8; 64]),
    );
    let model = doc.shading(&tensor).unwrap();
    let ShadingModel::TensorProductPatch(patch) = model else {
        panic!("expected tensor product patch mesh");
    };
    assert_eq!(patch.patches.len(), 1);
    assert_eq!(patch.patches[0].len(), 16);
    // With the default [0 1] decode ranges every 0xFF code maps to the range maximum.
    assert_eq!(patch.patches[0][0].x, 1.0);
    assert_eq!(patch.patches[0][0].components, vec![1.0]);

    let ragged = Value::Stream(
        Dict::new()
            .insert("ShadingType", Value::Integer(6))
            .insert("ColorSpace", name("DeviceGray"))
            .insert("BitsPerCoordinate", Value::Integer(8))
            .insert("BitsPerComponent", Value::Integer(8))
            .insert("BitsPerFlag", Value::Integer(8)),
        StreamData::Owned(vec![0u8; 3 * 13]),
    );
    assert!(matches!(
        doc.shading(&ragged),
        Err(ShadingError::Malformed(message)) if message.contains("groups of 12")
    ));
}

#[test]
fn mesh_bit_depths_are_limited_to_specification_values() {
    let doc = Document::new();
    let stream = Value::Stream(
        Dict::new()
            .insert("ShadingType", Value::Integer(4))
            .insert("ColorSpace", name("DeviceGray"))
            .insert("BitsPerCoordinate", Value::Integer(3))
            .insert("BitsPerComponent", Value::Integer(8))
            .insert("BitsPerFlag", Value::Integer(8))
            .insert("Decode", reals(&[0.0, 1.0, 0.0, 1.0, 0.0, 1.0])),
        StreamData::Owned(vec![0u8; 9]),
    );
    assert!(matches!(
        doc.shading(&stream),
        Err(ShadingError::Malformed(message)) if message.contains("BitsPerCoordinate")
    ));
}

#[test]
fn mesh_function_single_and_component_arrays_validate_dimensions() {
    let doc = Document::new();
    let single = Value::Stream(
        Dict::new()
            .insert("ShadingType", Value::Integer(4))
            .insert("ColorSpace", name("DeviceRGB"))
            .insert("BitsPerCoordinate", Value::Integer(8))
            .insert("BitsPerComponent", Value::Integer(8))
            .insert("BitsPerFlag", Value::Integer(8))
            .insert(
                "Function",
                exponential(&[0.0, 1.0], &[0.0, 0.0, 0.0], &[1.0, 1.0, 1.0]),
            )
            .insert("Decode", reals(&[0.0, 1.0, 0.0, 1.0, 0.0, 1.0])),
        StreamData::Owned(vec![0u8; 12]),
    );
    let model = doc.shading(&single).unwrap();
    let ShadingModel::FreeFormMesh(mesh) = model else {
        panic!("expected free form mesh");
    };
    assert!(matches!(mesh.function, Some(MeshFunction::Single(_))));
    assert_eq!(mesh.vertices[0].components.len(), 1);

    let components = Value::Stream(
        Dict::new()
            .insert("ShadingType", Value::Integer(4))
            .insert("ColorSpace", name("DeviceRGB"))
            .insert("BitsPerCoordinate", Value::Integer(8))
            .insert("BitsPerComponent", Value::Integer(8))
            .insert("BitsPerFlag", Value::Integer(8))
            .insert(
                "Function",
                array(vec![
                    exponential(&[0.0, 1.0], &[0.0], &[1.0]),
                    exponential(&[0.0, 1.0], &[0.0], &[1.0]),
                    exponential(&[0.0, 1.0], &[0.0], &[1.0]),
                ]),
            )
            .insert("Decode", reals(&[0.0, 1.0, 0.0, 1.0, 0.0, 1.0])),
        StreamData::Owned(vec![0u8; 12]),
    );
    let model = doc.shading(&components).unwrap();
    let ShadingModel::FreeFormMesh(mesh) = model else {
        panic!("expected free form mesh");
    };
    let Some(MeshFunction::Components(functions)) = &mesh.function else {
        panic!("expected component functions");
    };
    assert_eq!(functions.len(), 3);

    let wrong_count = Value::Stream(
        Dict::new()
            .insert("ShadingType", Value::Integer(4))
            .insert("ColorSpace", name("DeviceRGB"))
            .insert("BitsPerCoordinate", Value::Integer(8))
            .insert("BitsPerComponent", Value::Integer(8))
            .insert("BitsPerFlag", Value::Integer(8))
            .insert(
                "Function",
                array(vec![
                    exponential(&[0.0, 1.0], &[0.0], &[1.0]),
                    exponential(&[0.0, 1.0], &[0.0], &[1.0]),
                ]),
            )
            .insert("Decode", reals(&[0.0, 1.0, 0.0, 1.0, 0.0, 1.0])),
        StreamData::Owned(vec![0u8; 9]),
    );
    assert!(matches!(
        doc.shading(&wrong_count),
        Err(ShadingError::Malformed(message)) if message.contains("got 2")
    ));
}

#[test]
fn mesh_indexed_space_stores_single_component_and_rejects_functions() {
    let doc = Document::new();
    let indexed = array(vec![
        name("Indexed"),
        name("DeviceRGB"),
        Value::Integer(1),
        Value::HexString("FFFFFF000000".into()),
    ]);
    let lattice = Value::Stream(
        Dict::new()
            .insert("ShadingType", Value::Integer(5))
            .insert("ColorSpace", indexed.clone())
            .insert("BitsPerCoordinate", Value::Integer(8))
            .insert("BitsPerComponent", Value::Integer(8))
            .insert("VerticesPerRow", Value::Integer(2))
            .insert("Decode", reals(&[0.0, 10.0, 0.0, 10.0, 0.0, 1.0])),
        StreamData::Owned(vec![5, 6, 1, 0, 0, 128]),
    );
    let model = doc.shading(&lattice).unwrap();
    let ShadingModel::LatticeMesh(mesh) = model else {
        panic!("expected lattice mesh");
    };
    assert_eq!(mesh.vertices[1].components, vec![128.0 / 255.0]);

    let with_function = Value::Stream(
        Dict::new()
            .insert("ShadingType", Value::Integer(5))
            .insert("ColorSpace", indexed)
            .insert("BitsPerCoordinate", Value::Integer(8))
            .insert("BitsPerComponent", Value::Integer(8))
            .insert("VerticesPerRow", Value::Integer(2))
            .insert(
                "Function",
                exponential(&[0.0, 1.0], &[0.0, 0.0, 0.0], &[1.0, 1.0, 1.0]),
            )
            .insert("Decode", reals(&[0.0, 1.0, 0.0, 1.0, 0.0, 1.0])),
        StreamData::Owned(vec![0u8; 6]),
    );
    assert!(matches!(
        doc.shading(&with_function),
        Err(ShadingError::Malformed(message)) if message.contains("Indexed")
    ));
}

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

fn make_page_doc(resources: Dict) -> Document {
    let mut doc = Document::new();
    doc.set_object(Object::new(
        1,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Catalog"))
                .insert("Pages", Value::Ref(2, 0)),
        ),
    ));
    doc.set_object(Object::new(
        2,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Pages"))
                .insert("Kids", Value::Array(Array::new().push(Value::Ref(3, 0))))
                .insert("Count", Value::Integer(1)),
        ),
    ));
    doc.set_object(Object::new(
        3,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Page"))
                .insert("Parent", Value::Ref(2, 0))
                .insert("Resources", Value::Dict(resources)),
        ),
    ));
    doc.trailer = Trailer {
        dict: Dict::new()
            .insert("Root", Value::Ref(1, 0))
            .insert("Size", Value::Integer(4)),
    };
    doc
}

#[test]
fn page_shading_and_pattern_resources_resolve_named_entries() {
    let doc = make_page_doc(
        Dict::new()
            .insert(
                "Shading",
                Value::Dict(Dict::new().insert("Sh1", axial_dict(&[]))),
            )
            .insert(
                "Pattern",
                Value::Dict(
                    Dict::new()
                        .insert(
                            "P1",
                            dict(&[("PatternType", Value::Integer(2)), ("Shading", name("Sh1"))]),
                        )
                        .insert(
                            "P2",
                            Value::Stream(
                                Dict::new()
                                    .insert("PaintType", Value::Integer(1))
                                    .insert("TilingType", Value::Integer(2))
                                    .insert("BBox", reals(&[0.0, 0.0, 2.0, 2.0]))
                                    .insert("XStep", Value::Float(2.0))
                                    .insert("YStep", Value::Float(2.0)),
                                StreamData::Owned(b"n".to_vec()),
                            ),
                        ),
                ),
            ),
    );
    let model = doc.page_shading(0, "Sh1").unwrap();
    assert!(matches!(model, Some(ShadingModel::Axial(_))));
    assert!(doc.page_shading(0, "Missing").unwrap().is_none());

    let named_pattern = doc.page_pattern(0, "P1").unwrap();
    let Some(PatternModel::Shading(pattern)) = named_pattern else {
        panic!("expected shading pattern resolved through page resources");
    };
    assert_eq!(pattern.shading.shading_type(), 2);

    let tiling = doc.page_pattern(0, "P2").unwrap();
    let Some(PatternModel::Tiling(tiling)) = tiling else {
        panic!("expected tiling pattern");
    };
    assert_eq!(tiling.tiling_type, TilingType::NonAdaptiveVariableSpacing);
    assert!(doc.page_pattern(0, "Absent").unwrap().is_none());
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
