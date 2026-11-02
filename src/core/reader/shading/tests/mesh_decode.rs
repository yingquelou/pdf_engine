//! Type 4–7 网格与网格片渐变：位流顶点记录解码、分组完整性、位宽
//! 规范枚举、`/Function` 单函数与分量数组维数校验及 Indexed 色空间语义。

use super::super::*;
use super::support::{array, exponential, name, reals};
use crate::core::types::{Dict, Document, StreamData, Value};

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
