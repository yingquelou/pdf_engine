//! 渐变（Shading）与着色图案（Shading Pattern）字典构造：
//! ISO 32000-1 §8.7.4.2–§8.7.4.5 与 §8.7.2。
//!
//! 子模块：
//! - `mesh`：Type 4–7 网格/网格片渐变的流对象构造（位打包顶点数据与片流字典）；
//! - `encode`：`/Decode` 范围推导与 MSB-first 位流编码工具。

mod encode;
mod mesh;
#[cfg(test)]
mod tests;

use super::{color_space_value, real_array, write_function};
use crate::core::builder::{PaintType, Shading, ShadingOptions, ShadingPattern, TilingPattern};
use crate::core::types::{Array, Dict, Document, Object, StreamData, Value};
use crate::core::writer::WriteError;
use mesh::{
    coons_patch_stream, free_form_mesh_stream, lattice_mesh_stream, tensor_product_patch_stream,
};

/// 构造渐变对象值：Type 1–3 为直接字典，Type 4–7 为带位打包顶点/网格片
/// 数据的流对象（Table 78 / Table 84 / Table 85）。
pub(super) fn shading_object_value(
    shading: &Shading,
    doc: &mut Document,
    num: &mut i64,
) -> Result<Value, WriteError> {
    match shading {
        Shading::FreeForm(mesh) => free_form_mesh_stream(mesh, doc, num),
        Shading::Lattice(mesh) => lattice_mesh_stream(mesh, doc, num),
        Shading::Coons(mesh) => coons_patch_stream(mesh, doc, num),
        Shading::TensorProduct(mesh) => tensor_product_patch_stream(mesh, doc, num),
        _ => Ok(Value::Dict(shading_dict(shading, doc, num)?)),
    }
}

/// 构造渐变字典（Table 78）：Type 1/2/3；函数经 `write_function` 写出。
pub(super) fn shading_dict(
    shading: &Shading,
    doc: &mut Document,
    num: &mut i64,
) -> Result<Dict, WriteError> {
    if shading.is_stream() {
        return Err(WriteError::InvalidObject(
            "mesh shading must be written as a stream".into(),
        ));
    }
    shading
        .validate()
        .map_err(|message| WriteError::InvalidObject(format!("shading: {message}")))?;
    let mut dict = Dict::new()
        .insert("ShadingType", Value::Integer(shading.shading_type()))
        .insert(
            "ColorSpace",
            color_space_value(shading.color_space(), doc, num)?,
        );
    match shading {
        Shading::Function(function) => {
            dict = dict.insert("Domain", real_array(&function.domain));
            if let Some(matrix) = function.matrix {
                dict = dict.insert("Matrix", real_array(&matrix));
            }
            dict = dict.insert("Function", write_function(doc, &function.function, num)?);
        }
        Shading::Axial(axial) => {
            dict = dict.insert("Coords", real_array(&axial.coords));
            if let Some(domain) = axial.domain {
                dict = dict.insert("Domain", real_array(&domain));
            }
            dict = dict.insert("Function", write_function(doc, &axial.function, num)?);
            if let Some(extend) = axial.extend {
                dict = dict.insert("Extend", bool_array(extend));
            }
        }
        Shading::Radial(radial) => {
            dict = dict.insert("Coords", real_array(&radial.coords));
            if let Some(domain) = radial.domain {
                dict = dict.insert("Domain", real_array(&domain));
            }
            dict = dict.insert("Function", write_function(doc, &radial.function, num)?);
            if let Some(extend) = radial.extend {
                dict = dict.insert("Extend", bool_array(extend));
            }
        }
        Shading::FreeForm(_)
        | Shading::Lattice(_)
        | Shading::Coons(_)
        | Shading::TensorProduct(_) => {
            return Err(WriteError::InvalidObject(
                "mesh shading must be written as a stream".into(),
            ));
        }
    }
    Ok(with_shading_options(dict, shading.options()))
}

/// 写入所有渐变共有的可选条目：`/Background`、`/BBox` 与 `/AntiAlias`。
fn with_shading_options(mut dict: Dict, options: &ShadingOptions) -> Dict {
    if let Some(background) = &options.background {
        dict = dict.insert("Background", real_array(background));
    }
    if let Some(bbox) = options.bbox {
        dict = dict.insert("BBox", real_array(&bbox));
    }
    if let Some(anti_alias) = options.anti_alias {
        dict = dict.insert("AntiAlias", Value::Bool(anti_alias));
    }
    dict
}

/// 构造 PatternType 2 图案字典（Table 75）：Type 1–3 渐变内联为直接
/// 字典；Type 4/5 渐变是流，需分配独立对象并以间接引用挂入 `/Shading`。
pub(super) fn pattern_dict(
    pattern: &ShadingPattern,
    doc: &mut Document,
    num: &mut i64,
) -> Result<Dict, WriteError> {
    pattern
        .validate()
        .map_err(|message| WriteError::InvalidObject(format!("shading pattern: {message}")))?;
    let mut dict = Dict::new()
        .insert("Type", Value::Name("Pattern".into()))
        .insert("PatternType", Value::Integer(2));
    if let Some(matrix) = pattern.matrix {
        dict = dict.insert("Matrix", real_array(&matrix));
    }
    let shading = if pattern.shading.is_stream() {
        let value = shading_object_value(&pattern.shading, doc, num)?;
        let shading_num = *num;
        *num += 1;
        doc.set_object(Object::new(shading_num, 0, value));
        Value::Ref(shading_num, 0)
    } else {
        Value::Dict(shading_dict(&pattern.shading, doc, num)?)
    };
    dict = dict.insert("Shading", shading);
    Ok(dict)
}

fn bool_array(values: [bool; 2]) -> Value {
    Value::Array(Array(vec![Value::Bool(values[0]), Value::Bool(values[1])]))
}

/// 构造 PatternType 1 平铺图案流（Table 75）：彩色图案内容流原样写出；
/// 无色图案额外写出 `/ColorSpace`，颜色由使用图案时的 `scn` 操作数给出。
pub(super) fn tiling_pattern_stream(
    pattern: &mut TilingPattern,
    doc: &mut Document,
    num: &mut i64,
) -> Result<Value, WriteError> {
    let content = pattern.content.to_bytes();
    let len = content.len();
    let resources = match &pattern.resources {
        Some(resources) => resources.clone(),
        None => Dict::new(),
    };
    let mut dict = Dict::new()
        .insert("Type", Value::Name("Pattern".into()))
        .insert("PatternType", Value::Integer(1))
        .insert("PaintType", Value::Integer(pattern.paint_type.as_i64()))
        .insert("TilingType", Value::Integer(pattern.tiling_type.as_i64()))
        .insert("BBox", real_array(&pattern.bbox))
        .insert("XStep", Value::Float(pattern.x_step))
        .insert("YStep", Value::Float(pattern.y_step))
        .insert("Resources", Value::Dict(resources))
        .insert("Length", Value::Integer(len as i64));
    if let Some(matrix) = pattern.matrix {
        dict = dict.insert("Matrix", real_array(&matrix));
    }
    if pattern.paint_type == PaintType::Uncolored
        && let Some(color_space) = &pattern.color_space
    {
        dict = dict.insert("ColorSpace", color_space_value(color_space, doc, num)?);
    }
    Ok(Value::Stream(dict, StreamData::Owned(content)))
}
