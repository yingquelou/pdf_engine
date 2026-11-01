//! 网格位流编码单元测试。

use super::encode::encode_mesh;
use crate::core::builder::MeshVertex;

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
