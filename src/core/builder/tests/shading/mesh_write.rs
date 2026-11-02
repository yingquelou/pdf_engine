//! Type 4–7 网格/网格片渐变的流字典写出：`/BitsPer*`、`/Decode`、
//! `/Length` 位打包契约、缺省紧凑包围盒、Lab 分量范围与图案内间接流形态。

use super::super::*;
use super::support::{
    coons_patch_shading, free_form_mesh, render_with_shading, tensor_patch_shading,
};

fn lattice_mesh() -> LatticeMeshShading {
    LatticeMeshShading::new(
        ColorSpace::DeviceGray,
        2,
        vec![
            MeshVertex::new(0.0, 0.0, vec![0.0]),
            MeshVertex::new(10.0, 0.0, vec![1.0]),
            MeshVertex::new(0.0, 10.0, vec![0.5]),
            MeshVertex::new(10.0, 10.0, vec![0.0]),
        ],
    )
    .with_decode([0.0, 10.0, 0.0, 10.0])
    .with_bits(8, 8)
}

#[test]
fn coons_patch_shading_writes_type6_stream_dictionary() {
    let s = render_with_shading(Shading::Coons(coons_patch_shading()));

    assert!(s.contains("/ShadingType 6"), "{s}");
    assert!(s.contains("/ColorSpace /DeviceRGB"), "{s}");
    assert!(s.contains("/BitsPerCoordinate 8"), "{s}");
    assert!(s.contains("/BitsPerComponent 8"), "{s}");
    assert!(s.contains("/BitsPerFlag 8"), "{s}");
    assert!(
        s.contains("/Decode [0.0 30.0 0.0 30.0 0.0 1.0 0.0 1.0 0.0 1.0]"),
        "{s}"
    );
    assert!(
        s.contains("/Length 37"),
        "1 片 × (8 + 12×2×8 + 4×3×8) 位 = 37 字节: {s}"
    );
    assert!(!s.contains("/Sh1 <<"), "Coons 网格必须写成流对象: {s}");
    assert!(s.contains("/Sh1 sh"), "{s}");
}

#[test]
fn tensor_product_patch_shading_writes_type7_stream_dictionary() {
    let s = render_with_shading(Shading::TensorProduct(tensor_patch_shading()));

    assert!(s.contains("/ShadingType 7"), "{s}");
    assert!(s.contains("/ColorSpace /DeviceRGB"), "{s}");
    assert!(s.contains("/BitsPerCoordinate 8"), "{s}");
    assert!(s.contains("/BitsPerComponent 8"), "{s}");
    assert!(s.contains("/BitsPerFlag 8"), "{s}");
    assert!(!s.contains("/VerticesPerRow"), "Type 7 无该条目: {s}");
    assert!(
        s.contains("/Decode [0.0 30.0 0.0 30.0 0.0 1.0 0.0 1.0 0.0 1.0]"),
        "{s}"
    );
    assert!(
        s.contains("/Length 45"),
        "1 片 × (8 + 16×2×8 + 4×3×8) 位 = 45 字节: {s}"
    );
    assert!(s.contains("/Sh1 sh"), "{s}");
}

#[test]
fn free_form_mesh_shading_writes_type4_stream_dictionary() {
    let s = render_with_shading(Shading::FreeForm(free_form_mesh()));

    assert!(s.contains("/ShadingType 4"), "{s}");
    assert!(s.contains("/ColorSpace /DeviceRGB"), "{s}");
    assert!(s.contains("/BitsPerCoordinate 8"), "{s}");
    assert!(s.contains("/BitsPerComponent 8"), "{s}");
    assert!(s.contains("/BitsPerFlag 8"), "{s}");
    assert!(
        s.contains("/Decode [0.0 100.0 0.0 100.0 0.0 1.0 0.0 1.0 0.0 1.0]"),
        "{s}"
    );
    assert!(
        s.contains("/Length 18"),
        "3 顶点 × (8+8+8+24) 位 = 18 字节: {s}"
    );
    assert!(!s.contains("/Sh1 <<"), "网格渐变必须写成流对象: {s}");
    assert!(s.contains("/Sh1 sh"), "{s}");
}

#[test]
fn free_form_mesh_shading_defaults_to_tight_decode_bounds() {
    let mesh = FreeFormMeshShading::new(
        ColorSpace::DeviceGray,
        vec![
            MeshVertex::new(4.0, 6.0, vec![0.25]),
            MeshVertex::new(24.0, 6.0, vec![0.5]),
            MeshVertex::new(12.0, 18.0, vec![1.0]),
        ],
    );
    let s = render_with_shading(Shading::FreeForm(mesh));

    assert!(s.contains("/BitsPerCoordinate 16"), "{s}");
    assert!(s.contains("/BitsPerComponent 8"), "{s}");
    assert!(s.contains("/Decode [4.0 24.0 6.0 18.0 0.0 1.0]"), "{s}");
    assert!(
        s.contains("/Length 18"),
        "3 顶点 × (8+16+16+8) 位 = 18 字节: {s}"
    );
}

#[test]
fn lattice_mesh_shading_writes_type5_vertices_per_row_and_omits_flag() {
    let s = render_with_shading(Shading::Lattice(lattice_mesh()));

    assert!(s.contains("/ShadingType 5"), "{s}");
    assert!(s.contains("/ColorSpace /DeviceGray"), "{s}");
    assert!(s.contains("/VerticesPerRow 2"), "{s}");
    assert!(!s.contains("/BitsPerFlag"), "Type 5 无 flag 字段: {s}");
    assert!(s.contains("/Decode [0.0 10.0 0.0 10.0 0.0 1.0]"), "{s}");
    assert!(
        s.contains("/Length 12"),
        "4 顶点 × (8+8+8) 位 = 12 字节: {s}"
    );
}

#[test]
fn mesh_shading_uses_lab_component_decode_ranges() {
    let mesh = FreeFormMeshShading::new(
        ColorSpace::Lab(Lab::new([0.9505, 1.0, 1.089])),
        vec![
            MeshVertex::new(0.0, 0.0, vec![50.0, -100.0, 100.0]),
            MeshVertex::new(10.0, 0.0, vec![50.0, 0.0, 0.0]),
            MeshVertex::new(0.0, 10.0, vec![50.0, 100.0, -100.0]),
        ],
    );
    let s = render_with_shading(Shading::FreeForm(mesh));

    assert!(
        s.contains("/Decode [0.0 10.0 0.0 10.0 0.0 100.0 -100.0 100.0 -100.0 100.0]"),
        "{s}"
    );
}

#[test]
fn mesh_shading_pattern_writes_mesh_as_indirect_stream() {
    let pattern = ShadingPattern::new(Shading::FreeForm(free_form_mesh()));
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let h = b.shading_pattern(&pattern);
    b.fill_rect_with_pattern(p, h, [0.0, 0.0, 100.0, 100.0]);
    let s = render(&mut b);

    assert!(s.contains("/PatternType 2"), "{s}");
    assert!(!s.contains("/Shading <<"), "流渐变不能内联在图案字典: {s}");
    assert!(s.contains("/Shading 6 0 R"), "{s}");
    assert!(s.contains("6 0 obj"), "{s}");
    assert!(s.contains("/ShadingType 4"), "{s}");
}
