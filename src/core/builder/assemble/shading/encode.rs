//! 网格数据的 `/Decode` 范围推导、量化与 MSB-first 位流编码工具。

use super::real_array;
use crate::core::builder::{ColorSpace, MeshFunction, MeshVertex, mesh_component_ranges};
use crate::core::types::Value;
use crate::core::writer::WriteError;

/// 网格片坐标解码范围：显式给出时原样使用，否则取控制点的紧包围盒。
pub(super) fn patch_decode(
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
pub(super) fn write_patch(
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
pub(super) fn mesh_decode(
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
pub(super) fn mesh_decode_array(decode: [f64; 4], color_ranges: &[[f64; 2]]) -> Value {
    let mut values = vec![decode[0], decode[1], decode[2], decode[3]];
    for [min, max] in color_ranges {
        values.push(*min);
        values.push(*max);
    }
    real_array(&values)
}

/// 按 MSB-first 连续位流编码顶点：flag（Type 4）、x、y、颜色分量。
pub(super) fn encode_mesh(
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
pub(super) struct BitWriter {
    bytes: Vec<u8>,
    buffer: u64,
    pending: u8,
}

impl BitWriter {
    pub(super) fn new() -> Self {
        Self {
            bytes: Vec::new(),
            buffer: 0,
            pending: 0,
        }
    }

    pub(super) fn write(&mut self, value: u32, width: u8) {
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

    pub(super) fn finish(mut self) -> Vec<u8> {
        if self.pending > 0 {
            self.bytes.push((self.buffer << (8 - self.pending)) as u8);
        }
        self.bytes
    }
}
