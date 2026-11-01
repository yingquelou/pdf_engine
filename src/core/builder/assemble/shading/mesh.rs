//! Type 4–7 网格与网格片渐变流：位打包顶点/控制点数据与流字典
//! （§8.7.4.4 Table 84 / §8.7.4.5 Table 85）。

use super::encode::{
    BitWriter, encode_mesh, mesh_decode, mesh_decode_array, patch_decode, write_patch,
};
use super::{color_space_value, with_shading_options, write_function};
use crate::core::builder::{
    ColorSpace, CoonsPatchShading, FreeFormMeshShading, LatticeMeshShading, MeshFunction,
    ShadingOptions, TensorProductPatchShading,
};
use crate::core::types::{Array, Dict, Document, StreamData, Value};
use crate::core::writer::WriteError;

/// 网格 `/Function`：单函数直接写出；函数数组按颜色分量顺序写成 PDF 数组。
fn mesh_function_value(
    function: &MeshFunction,
    doc: &mut Document,
    num: &mut i64,
) -> Result<Value, WriteError> {
    match function {
        MeshFunction::Single(function) => write_function(doc, function, num),
        MeshFunction::Components(functions) => {
            let mut values = Vec::with_capacity(functions.len());
            for function in functions {
                values.push(write_function(doc, function, num)?);
            }
            Ok(Value::Array(Array(values)))
        }
    }
}

pub(super) fn free_form_mesh_stream(
    mesh: &FreeFormMeshShading,
    doc: &mut Document,
    num: &mut i64,
) -> Result<Value, WriteError> {
    let role = "type 4 shading";
    let (decode, ranges) = mesh_decode(
        role,
        &mesh.color_space,
        &mesh.vertices,
        mesh.function.as_ref(),
        mesh.decode,
    )?;
    let payload = encode_mesh(
        &mesh.vertices,
        &ranges,
        decode,
        mesh.bits_per_coordinate,
        mesh.bits_per_component,
        Some(mesh.bits_per_flag),
    );
    let mut dict = Dict::new()
        .insert("ShadingType", Value::Integer(4))
        .insert(
            "ColorSpace",
            color_space_value(&mesh.color_space, doc, num)?,
        )
        .insert(
            "BitsPerCoordinate",
            Value::Integer(mesh.bits_per_coordinate as i64),
        )
        .insert(
            "BitsPerComponent",
            Value::Integer(mesh.bits_per_component as i64),
        )
        .insert("BitsPerFlag", Value::Integer(mesh.bits_per_flag as i64))
        .insert("Decode", mesh_decode_array(decode, &ranges))
        .insert("Length", Value::Integer(payload.len() as i64));
    if let Some(function) = &mesh.function {
        dict = dict.insert("Function", mesh_function_value(function, doc, num)?);
    }
    Ok(Value::Stream(
        with_shading_options(dict, &mesh.options),
        StreamData::Owned(payload),
    ))
}

pub(super) fn lattice_mesh_stream(
    mesh: &LatticeMeshShading,
    doc: &mut Document,
    num: &mut i64,
) -> Result<Value, WriteError> {
    let role = "type 5 shading";
    let (decode, ranges) = mesh_decode(
        role,
        &mesh.color_space,
        &mesh.vertices,
        mesh.function.as_ref(),
        mesh.decode,
    )?;
    let payload = encode_mesh(
        &mesh.vertices,
        &ranges,
        decode,
        mesh.bits_per_coordinate,
        mesh.bits_per_component,
        None,
    );
    let mut dict = Dict::new()
        .insert("ShadingType", Value::Integer(5))
        .insert(
            "ColorSpace",
            color_space_value(&mesh.color_space, doc, num)?,
        )
        .insert(
            "BitsPerCoordinate",
            Value::Integer(mesh.bits_per_coordinate as i64),
        )
        .insert(
            "BitsPerComponent",
            Value::Integer(mesh.bits_per_component as i64),
        )
        .insert(
            "VerticesPerRow",
            Value::Integer(mesh.vertices_per_row as i64),
        )
        .insert("Decode", mesh_decode_array(decode, &ranges))
        .insert("Length", Value::Integer(payload.len() as i64));
    if let Some(function) = &mesh.function {
        dict = dict.insert("Function", mesh_function_value(function, doc, num)?);
    }
    Ok(Value::Stream(
        with_shading_options(dict, &mesh.options),
        StreamData::Owned(payload),
    ))
}

struct PatchStreamConfig {
    shading_type: i64,
    bits_per_coordinate: u8,
    bits_per_component: u8,
    bits_per_flag: u8,
    decode: [f64; 4],
    color_ranges: Vec<[f64; 2]>,
}

fn patch_stream_dict(
    config: &PatchStreamConfig,
    color_space: &ColorSpace,
    function: Option<&MeshFunction>,
    options: &ShadingOptions,
    length: usize,
    doc: &mut Document,
    num: &mut i64,
) -> Result<Dict, WriteError> {
    let mut dict = Dict::new()
        .insert("ShadingType", Value::Integer(config.shading_type))
        .insert("ColorSpace", color_space_value(color_space, doc, num)?)
        .insert(
            "BitsPerCoordinate",
            Value::Integer(config.bits_per_coordinate as i64),
        )
        .insert(
            "BitsPerComponent",
            Value::Integer(config.bits_per_component as i64),
        )
        .insert("BitsPerFlag", Value::Integer(config.bits_per_flag as i64))
        .insert(
            "Decode",
            mesh_decode_array(config.decode, &config.color_ranges),
        )
        .insert("Length", Value::Integer(length as i64));
    if let Some(function) = function {
        dict = dict.insert("Function", mesh_function_value(function, doc, num)?);
    }
    Ok(with_shading_options(dict, options))
}

pub(super) fn coons_patch_stream(
    shading: &CoonsPatchShading,
    doc: &mut Document,
    num: &mut i64,
) -> Result<Value, WriteError> {
    let role = "type 6 shading";
    let (decode, color_ranges) = patch_decode(
        role,
        &shading.color_space,
        shading.patches.iter().flat_map(|patch| patch.points),
        shading.function.as_ref(),
        shading.decode,
    )?;
    let mut writer = BitWriter::new();
    for patch in &shading.patches {
        writer.write(0, shading.bits_per_flag);
        write_patch(
            &mut writer,
            &patch.points,
            &patch.colors,
            &color_ranges,
            decode,
            shading.bits_per_coordinate,
            shading.bits_per_component,
        );
    }
    let payload = writer.finish();
    let config = PatchStreamConfig {
        shading_type: 6,
        bits_per_coordinate: shading.bits_per_coordinate,
        bits_per_component: shading.bits_per_component,
        bits_per_flag: shading.bits_per_flag,
        decode,
        color_ranges,
    };
    let dict = patch_stream_dict(
        &config,
        &shading.color_space,
        shading.function.as_ref(),
        &shading.options,
        payload.len(),
        doc,
        num,
    )?;
    Ok(Value::Stream(dict, StreamData::Owned(payload)))
}

pub(super) fn tensor_product_patch_stream(
    shading: &TensorProductPatchShading,
    doc: &mut Document,
    num: &mut i64,
) -> Result<Value, WriteError> {
    let role = "type 7 shading";
    let (decode, color_ranges) = patch_decode(
        role,
        &shading.color_space,
        shading.patches.iter().flat_map(|patch| patch.points),
        shading.function.as_ref(),
        shading.decode,
    )?;
    let mut writer = BitWriter::new();
    for patch in &shading.patches {
        writer.write(0, shading.bits_per_flag);
        write_patch(
            &mut writer,
            &patch.points,
            &patch.colors,
            &color_ranges,
            decode,
            shading.bits_per_coordinate,
            shading.bits_per_component,
        );
    }
    let payload = writer.finish();
    let config = PatchStreamConfig {
        shading_type: 7,
        bits_per_coordinate: shading.bits_per_coordinate,
        bits_per_component: shading.bits_per_component,
        bits_per_flag: shading.bits_per_flag,
        decode,
        color_ranges,
    };
    let dict = patch_stream_dict(
        &config,
        &shading.color_space,
        shading.function.as_ref(),
        &shading.options,
        payload.len(),
        doc,
        num,
    )?;
    Ok(Value::Stream(dict, StreamData::Owned(payload)))
}
