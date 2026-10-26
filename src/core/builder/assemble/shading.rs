//! 渐变（Shading）与着色图案（Shading Pattern）字典构造：
//! ISO 32000-1 §8.7.4.2–§8.7.4.5 与 §8.7.2。

use super::{color_space_value, real_array, write_function};
use crate::core::builder::{
    ColorSpace, CoonsPatchShading, FreeFormMeshShading, LatticeMeshShading, MeshFunction,
    MeshVertex, PaintType, Shading, ShadingOptions, ShadingPattern, TensorProductPatchShading,
    TilingPattern, mesh_component_ranges,
};
use crate::core::types::{Array, Dict, Document, Object, StreamData, Value};
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

fn free_form_mesh_stream(
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

fn lattice_mesh_stream(
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

fn coons_patch_stream(
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

fn tensor_product_patch_stream(
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

/// 网格片坐标解码范围：显式给出时原样使用，否则取控制点的紧包围盒。
fn patch_decode(
    role: &str,
    color_space: &ColorSpace,
    points: impl Iterator<Item = [f64; 2]>,
    function: Option<&MeshFunction>,
    explicit: Option<[f64; 4]>,
) -> Result<([f64; 4], Vec<[f64; 2]>), WriteError> {
    let decode = match explicit {
        Some(decode) => decode,
        None => {
            let mut min_x = f64::INFINITY;
            let mut max_x = f64::NEG_INFINITY;
            let mut min_y = f64::INFINITY;
            let mut max_y = f64::NEG_INFINITY;
            for [x, y] in points {
                min_x = min_x.min(x);
                max_x = max_x.max(x);
                min_y = min_y.min(y);
                max_y = max_y.max(y);
            }
            [min_x, max_x, min_y, max_y]
        }
    };
    if decode[0] >= decode[1] || decode[2] >= decode[3] {
        return Err(WriteError::InvalidObject(format!(
            "{role} Decode range must be non-degenerate; provide an explicit decode range"
        )));
    }
    Ok((
        decode,
        mesh_component_ranges(color_space, function.is_some()),
    ))
}

/// 写出一个网格片：12/16 个控制点坐标，再写 4 个角点颜色。
fn write_patch(
    writer: &mut BitWriter,
    points: &[[f64; 2]],
    colors: &[Vec<f64>],
    color_ranges: &[[f64; 2]],
    decode: [f64; 4],
    bits_per_coordinate: u8,
    bits_per_component: u8,
) {
    let coordinate_max = max_code(bits_per_coordinate);
    let component_max = max_code(bits_per_component);
    for [x, y] in points {
        writer.write(
            quantize(*x, decode[0], decode[1], coordinate_max),
            bits_per_coordinate,
        );
        writer.write(
            quantize(*y, decode[2], decode[3], coordinate_max),
            bits_per_coordinate,
        );
    }
    for color in colors {
        for (value, [min, max]) in color.iter().zip(color_ranges) {
            writer.write(
                quantize(*value, *min, *max, component_max),
                bits_per_component,
            );
        }
    }
}

/// 坐标解码范围：显式给出时原样使用，否则取顶点的紧包围盒。
fn mesh_decode(
    role: &str,
    color_space: &ColorSpace,
    vertices: &[MeshVertex],
    function: Option<&MeshFunction>,
    explicit: Option<[f64; 4]>,
) -> Result<([f64; 4], Vec<[f64; 2]>), WriteError> {
    let decode = match explicit {
        Some(decode) => decode,
        None => {
            let mut min_x = f64::INFINITY;
            let mut max_x = f64::NEG_INFINITY;
            let mut min_y = f64::INFINITY;
            let mut max_y = f64::NEG_INFINITY;
            for vertex in vertices {
                min_x = min_x.min(vertex.x);
                max_x = max_x.max(vertex.x);
                min_y = min_y.min(vertex.y);
                max_y = max_y.max(vertex.y);
            }
            [min_x, max_x, min_y, max_y]
        }
    };
    if decode[0] >= decode[1] || decode[2] >= decode[3] {
        return Err(WriteError::InvalidObject(format!(
            "{role} Decode range must be non-degenerate; provide an explicit decode range"
        )));
    }
    Ok((
        decode,
        mesh_component_ranges(color_space, function.is_some()),
    ))
}

/// `/Decode` 数组：先 x/y，再按分量顺序给出各颜色分量的范围。
fn mesh_decode_array(decode: [f64; 4], color_ranges: &[[f64; 2]]) -> Value {
    let mut values = vec![decode[0], decode[1], decode[2], decode[3]];
    for [min, max] in color_ranges {
        values.push(*min);
        values.push(*max);
    }
    real_array(&values)
}

/// 按 MSB-first 连续位流编码顶点：flag（Type 4）、x、y、颜色分量。
fn encode_mesh(
    vertices: &[MeshVertex],
    color_ranges: &[[f64; 2]],
    decode: [f64; 4],
    bits_per_coordinate: u8,
    bits_per_component: u8,
    bits_per_flag: Option<u8>,
) -> Vec<u8> {
    let mut writer = BitWriter::new();
    let coordinate_max = max_code(bits_per_coordinate);
    let component_max = max_code(bits_per_component);
    for vertex in vertices {
        if let Some(bits) = bits_per_flag {
            writer.write(0, bits);
        }
        writer.write(
            quantize(vertex.x, decode[0], decode[1], coordinate_max),
            bits_per_coordinate,
        );
        writer.write(
            quantize(vertex.y, decode[2], decode[3], coordinate_max),
            bits_per_coordinate,
        );
        for (value, [min, max]) in vertex.color.iter().zip(color_ranges) {
            writer.write(
                quantize(*value, *min, *max, component_max),
                bits_per_component,
            );
        }
    }
    writer.finish()
}

fn max_code(bits: u8) -> u32 {
    if bits == 32 {
        u32::MAX
    } else {
        (1u32 << bits) - 1
    }
}

fn quantize(value: f64, min: f64, max: f64, max_code: u32) -> u32 {
    let ratio = ((value - min) / (max - min)).clamp(0.0, 1.0);
    (ratio * max_code as f64).round() as u32
}

/// MSB-first 位打包器：写入宽度 1..=32 的整数，跨字节连续，末尾补零。
struct BitWriter {
    bytes: Vec<u8>,
    buffer: u64,
    pending: u8,
}

impl BitWriter {
    fn new() -> Self {
        Self {
            bytes: Vec::new(),
            buffer: 0,
            pending: 0,
        }
    }

    fn write(&mut self, value: u32, width: u8) {
        debug_assert!((1..=32).contains(&width));
        let mask = (1u64 << width) - 1;
        self.buffer = (self.buffer << width) | (value as u64 & mask);
        self.pending += width;
        while self.pending >= 8 {
            self.pending -= 8;
            self.bytes
                .push(((self.buffer >> self.pending) & 0xFF) as u8);
        }
        if self.pending == 0 {
            self.buffer = 0;
        } else {
            self.buffer &= (1u64 << self.pending) - 1;
        }
    }

    fn finish(mut self) -> Vec<u8> {
        if self.pending > 0 {
            self.bytes.push((self.buffer << (8 - self.pending)) as u8);
        }
        self.bytes
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

#[cfg(test)]
mod tests {
    use super::*;

    /// 非字节对齐的位流必须按 MSB-first 连续打包，末尾补零。
    #[test]
    fn mesh_payload_packs_bits_msb_first_across_byte_boundaries() {
        let vertices = vec![
            MeshVertex::new(0.0, 0.0, vec![0.0]),
            MeshVertex::new(15.0, 15.0, vec![1.0]),
        ];
        let payload = encode_mesh(
            &vertices,
            &[[0.0, 1.0]],
            [0.0, 15.0, 0.0, 15.0],
            4,
            4,
            Some(2),
        );
        // 每顶点 2 + 4 + 4 + 4 = 14 位；两顶点共 28 位，末字节补 4 个零位。
        assert_eq!(payload, vec![0x00, 0x00, 0xFF, 0xF0]);
    }

    /// 顶点值按 `/Decode` 范围线性量化并按位深取整。
    #[test]
    fn mesh_payload_quantizes_values_into_the_decode_range() {
        let vertices = vec![MeshVertex::new(5.0, 5.0, vec![0.5])];
        let payload = encode_mesh(&vertices, &[[0.0, 1.0]], [0.0, 10.0, 0.0, 10.0], 8, 8, None);
        // 0.5 × 255 = 127.5 → 128；无 flag 字段。
        assert_eq!(payload, vec![128, 128, 128]);
    }
}
