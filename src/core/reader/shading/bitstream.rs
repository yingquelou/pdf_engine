//! 网格片记录分组与网格位流（MSB-first）顶点记录解码。

use super::error::{MAX_MESH_RECORDS, Result, malformed};
use super::model::MeshVertex;

pub(super) fn group_patches(
    vertices: Vec<MeshVertex>,
    per_patch: usize,
) -> Result<Vec<[MeshVertex; 12]>> {
    if vertices.is_empty() || !vertices.len().is_multiple_of(per_patch) {
        return Err(malformed(format!(
            "patch vertex records must come in complete groups of {per_patch}"
        )));
    }
    let mut out = Vec::new();
    for chunk in vertices.chunks(per_patch) {
        let array: [MeshVertex; 12] = chunk
            .to_vec()
            .try_into()
            .map_err(|_| malformed("internal patch grouping mismatch"))?;
        out.push(array);
    }
    Ok(out)
}

pub(super) fn group_tensor_patches(vertices: Vec<MeshVertex>) -> Result<Vec<[MeshVertex; 16]>> {
    if vertices.is_empty() || !vertices.len().is_multiple_of(16) {
        return Err(malformed(
            "type 7 patch vertex records must come in complete groups of 16",
        ));
    }
    let mut out = Vec::new();
    for chunk in vertices.chunks(16) {
        let array: [MeshVertex; 16] = chunk
            .to_vec()
            .try_into()
            .map_err(|_| malformed("internal patch grouping mismatch"))?;
        out.push(array);
    }
    Ok(out)
}

/// 从位流按 MSB-first 读取 `bits` 位（≤32）。
fn read_bits(data: &[u8], offset: usize, bits: usize) -> u32 {
    let mut value = 0u32;
    for i in 0..bits {
        let bit = offset + i;
        let on = (data[bit / 8] >> (7 - bit % 8)) & 1 == 1;
        value = (value << 1) | on as u32;
    }
    value
}

fn map_code(code: u32, bits: usize, range: (f64, f64)) -> f64 {
    let max = if bits >= 32 {
        u32::MAX as f64
    } else {
        (1u64 << bits) as f64 - 1.0
    };
    let t = f64::from(code) / max;
    range.0 + (range.1 - range.0) * t
}

pub(super) fn decode_mesh(
    data: &[u8],
    record_bits: usize,
    flag_bits: usize,
    coord_bits: usize,
    component_bits: usize,
    decode: &[(f64, f64)],
) -> Result<Vec<MeshVertex>> {
    let component_count = decode.len() - 2;
    let total_bits = data.len() * 8;
    let mut vertices = Vec::new();
    let mut pos = 0usize;
    while pos + record_bits <= total_bits {
        let flag = if flag_bits == 0 {
            0
        } else {
            read_bits(data, pos, flag_bits) as u8
        };
        pos += flag_bits;
        let x = map_code(read_bits(data, pos, coord_bits), coord_bits, decode[0]);
        pos += coord_bits;
        let y = map_code(read_bits(data, pos, coord_bits), coord_bits, decode[1]);
        pos += coord_bits;
        let mut components = Vec::with_capacity(component_count);
        for pair in &decode[2..] {
            components.push(map_code(
                read_bits(data, pos, component_bits),
                component_bits,
                *pair,
            ));
            pos += component_bits;
        }
        vertices.push(MeshVertex {
            flag,
            x,
            y,
            components,
        });
        if vertices.len() > MAX_MESH_RECORDS {
            return Err(malformed("mesh vertex data exceeds the record limit"));
        }
    }
    if vertices.is_empty() {
        return Err(malformed("mesh shading stream contains no vertex records"));
    }
    Ok(vertices)
}
