//! 渐变（Shading Type 1–7）与着色图案（PatternType 2）写端测试。

use super::*;
use crate::core::types::PdfVersion;

fn exponential(c0: Vec<f64>, c1: Vec<f64>) -> Function {
    Function::Exponential(
        ExponentialFunction::new(vec![0.0, 1.0], 1.0)
            .with_c0(c0)
            .with_c1(c1),
    )
}

fn axial_shading() -> Shading {
    Shading::Axial(
        AxialShading::new(
            ColorSpace::DeviceGray,
            [0.0, 0.0, 100.0, 0.0],
            exponential(vec![0.0], vec![1.0]),
        )
        .with_domain([0.0, 1.0])
        .with_extend([true, false]),
    )
}

fn shading_pattern() -> ShadingPattern {
    ShadingPattern::new(axial_shading()).with_matrix([2.0, 0.0, 0.0, 2.0, 5.0, 5.0])
}

fn render_with_shading(shading: Shading) -> String {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let h = b.shading(&shading);
    b.paint_shading(p, h);
    render(&mut b)
}

#[test]
fn function_shading_writes_type1_domain_matrix_and_sampled_function() {
    let function = Function::Sampled(SampledFunction::new(
        vec![0.0, 1.0, 0.0, 1.0],
        vec![0.0, 1.0, 0.0, 1.0, 0.0, 1.0],
        vec![2, 2],
        8,
        (0..12).map(|v| (v * 20) as f64).collect::<Vec<_>>(),
    ));
    let shading = Shading::Function(
        FunctionShading::new(ColorSpace::DeviceRGB, [0.0, 1.0, 0.0, 1.0], function)
            .with_matrix([1.0, 0.0, 0.0, 1.0, 10.0, 20.0]),
    );
    let s = render_with_shading(shading);

    assert!(s.contains("/Shading <<"), "{s}");
    assert!(s.contains("/Sh1 "), "{s}");
    assert!(s.contains("/ShadingType 1"), "{s}");
    assert!(s.contains("/ColorSpace /DeviceRGB"), "{s}");
    assert!(s.contains("/Domain [0.0 1.0 0.0 1.0]"), "{s}");
    assert!(s.contains("/Matrix [1.0 0.0 0.0 1.0 10.0 20.0]"), "{s}");
    assert!(s.contains("/FunctionType 0"), "{s}");
    assert!(s.contains("/Sh1 sh"), "{s}");
}

#[test]
fn axial_shading_writes_coords_domain_function_and_extend() {
    let s = render_with_shading(axial_shading());

    assert!(s.contains("/ShadingType 2"), "{s}");
    assert!(s.contains("/ColorSpace /DeviceGray"), "{s}");
    assert!(s.contains("/Coords [0.0 0.0 100.0 0.0]"), "{s}");
    assert!(s.contains("/Domain [0.0 1.0]"), "{s}");
    assert!(s.contains("/Extend [true false]"), "{s}");
    assert!(s.contains("/Function <<"), "{s}");
    assert!(s.contains("/FunctionType 2"), "{s}");
    assert!(s.contains("/C0 [0.0]"), "{s}");
    assert!(s.contains("/C1 [1.0]"), "{s}");
    assert!(s.contains("/Sh1 sh"), "{s}");
}

#[test]
fn radial_shading_writes_six_coords_and_omits_default_entries() {
    let function = exponential(vec![1.0, 0.0, 0.0], vec![0.0, 0.0, 1.0]);
    let shading = Shading::Radial(RadialShading::new(
        ColorSpace::DeviceRGB,
        [50.0, 50.0, 0.0, 50.0, 50.0, 80.0],
        function,
    ));
    let s = render_with_shading(shading);

    assert!(s.contains("/ShadingType 3"), "{s}");
    assert!(s.contains("/Coords [50.0 50.0 0.0 50.0 50.0 80.0]"), "{s}");
    assert!(s.contains("/ColorSpace /DeviceRGB"), "{s}");
    assert!(s.contains("/FunctionType 2"), "{s}");
    assert!(!s.contains("/Extend"), "省略 Extend 时不应写出: {s}");
    assert_eq!(
        s.matches("/Domain [0.0 1.0]").count(),
        1,
        "只应出现函数自身的 Domain，渐变 Domain 省略: {s}"
    );
}

#[test]
fn shading_pattern_writes_pattern_type_two_and_reuses_page_resource() {
    let pattern = shading_pattern();
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let h = b.shading_pattern(&pattern);
    b.fill_rect_with_pattern(p, h, [70.0, 500.0, 120.0, 60.0]);
    b.fill_rect_with_pattern(p, h, [70.0, 400.0, 120.0, 60.0]);
    let s = render(&mut b);

    assert!(s.contains("/Pattern <<"), "{s}");
    assert!(s.contains("/P1 "), "{s}");
    assert!(!s.contains("/P2"), "同一图案应复用 /P1: {s}");
    assert_eq!(s.matches("/P1 scn").count(), 2, "{s}");
    assert!(s.contains("/Pattern cs"), "{s}");
    assert!(s.contains("/PatternType 2"), "{s}");
    assert!(s.contains("/Matrix [2.0 0.0 0.0 2.0 5.0 5.0]"), "{s}");
    assert!(s.contains("/Shading <<"), "图案应内联渐变字典: {s}");
    assert!(s.contains("/ShadingType 2"), "{s}");
    assert!(s.contains("70.0 500.0 120.0 60.0 re"), "{s}");
    assert!(s.contains("70.0 400.0 120.0 60.0 re"), "{s}");
}

#[test]
fn unused_shadings_and_patterns_are_not_written() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &Font::helvetica(12.0), 72.0, 700.0, "Plain");
    b.shading(&axial_shading());
    b.shading_pattern(&shading_pattern());
    let s = render(&mut b);

    assert!(!s.contains("/ShadingType"), "未绘制的渐变不应写出: {s}");
    assert!(!s.contains("/PatternType"), "未使用的图案不应写出: {s}");
    assert!(!s.contains("/Shading <<"), "{s}");
    assert!(!s.contains(" sh\n"), "{s}");
}

#[test]
fn shading_requires_pdf_1_3() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_2);
    let p = b.page(PageSize::A4);
    let h = b.shading(&axial_shading());
    b.paint_shading(p, h);
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("PDF 1.2 必须拒绝渐变");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(_)),
        "{error}"
    );
    assert!(out.is_empty(), "版本不足时不应写出任何字节");
}

#[test]
fn invalid_shading_and_pattern_parameters_fail_the_build() {
    fn assert_shading_rejected(shading: Shading, expected: &str) {
        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        let h = b.shading(&shading);
        b.paint_shading(p, h);
        let mut out = Vec::new();
        let error = b.build(&mut out).expect_err("非法渐变应被拒绝");
        assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
        assert!(error.to_string().contains(expected), "{error}");
        assert!(out.is_empty(), "构建失败时不应写出任何字节");
    }

    assert_shading_rejected(
        Shading::Function(FunctionShading::new(
            ColorSpace::DeviceGray,
            [0.0, f64::NAN, 0.0, 1.0],
            exponential(vec![0.0], vec![1.0]),
        )),
        "type 1 shading Domain",
    );
    assert_shading_rejected(
        Shading::Function(FunctionShading::new(
            ColorSpace::DeviceGray,
            [0.0, 1.0, 0.0, 1.0],
            exponential(vec![0.0], vec![1.0]),
        )),
        "must take 2 input(s)",
    );
    assert_shading_rejected(
        Shading::Axial(AxialShading::new(
            ColorSpace::DeviceRGB,
            [0.0, 0.0, 100.0, 0.0],
            exponential(vec![0.0], vec![1.0]),
        )),
        "must produce 3 output(s)",
    );
    assert_shading_rejected(
        Shading::Radial(RadialShading::new(
            ColorSpace::DeviceGray,
            [0.0, 0.0, -1.0, 10.0, 10.0, 5.0],
            exponential(vec![0.0], vec![1.0]),
        )),
        "radii must not be negative",
    );
    assert_shading_rejected(
        Shading::Axial(AxialShading::new(
            ColorSpace::Indexed(Indexed::new(ColorSpace::DeviceGray, 0, Vec::new())),
            [0.0, 0.0, 100.0, 0.0],
            exponential(vec![0.0], vec![1.0]),
        )),
        "color space",
    );

    let mut pattern = shading_pattern();
    pattern.matrix = Some([1.0, 0.0, 0.0, 1.0, f64::INFINITY, 0.0]);
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let h = b.shading_pattern(&pattern);
    b.fill_rect_with_pattern(p, h, [0.0, 0.0, 10.0, 10.0]);
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("非法图案矩阵应被拒绝");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(error.to_string().contains("Matrix"), "{error}");
    assert!(out.is_empty());
}

fn free_form_mesh() -> FreeFormMeshShading {
    FreeFormMeshShading::new(
        ColorSpace::DeviceRGB,
        vec![
            MeshVertex::new(10.0, 20.0, vec![0.0, 0.0, 0.0]),
            MeshVertex::new(90.0, 20.0, vec![1.0, 0.0, 0.0]),
            MeshVertex::new(50.0, 80.0, vec![0.0, 0.0, 1.0]),
        ],
    )
    .with_decode([0.0, 100.0, 0.0, 100.0])
    .with_bits(8, 8, 8)
}

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

fn coons_points() -> [[f64; 2]; 12] {
    [
        [0.0, 0.0],
        [10.0, 0.0],
        [20.0, 0.0],
        [30.0, 0.0],
        [30.0, 10.0],
        [30.0, 20.0],
        [30.0, 30.0],
        [20.0, 30.0],
        [10.0, 30.0],
        [0.0, 30.0],
        [0.0, 20.0],
        [0.0, 10.0],
    ]
}

fn tensor_points() -> [[f64; 2]; 16] {
    [
        [0.0, 0.0],
        [10.0, 0.0],
        [20.0, 0.0],
        [30.0, 0.0],
        [0.0, 10.0],
        [10.0, 10.0],
        [20.0, 10.0],
        [30.0, 10.0],
        [0.0, 20.0],
        [10.0, 20.0],
        [20.0, 20.0],
        [30.0, 20.0],
        [0.0, 30.0],
        [10.0, 30.0],
        [20.0, 30.0],
        [30.0, 30.0],
    ]
}

fn patch_colors() -> [Vec<f64>; 4] {
    [
        vec![0.0, 0.0, 0.0],
        vec![1.0, 0.0, 0.0],
        vec![0.0, 1.0, 0.0],
        vec![0.0, 0.0, 1.0],
    ]
}

fn coons_patch_shading() -> CoonsPatchShading {
    CoonsPatchShading::new(
        ColorSpace::DeviceRGB,
        vec![CoonsPatch::new(coons_points(), patch_colors())],
    )
    .with_bits(8, 8, 8)
}

fn tensor_patch_shading() -> TensorProductPatchShading {
    TensorProductPatchShading::new(
        ColorSpace::DeviceRGB,
        vec![TensorProductPatch::new(tensor_points(), patch_colors())],
    )
    .with_bits(8, 8, 8)
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
fn invalid_patch_shading_parameters_fail_the_build() {
    fn rejected(shading: Shading, expected: &str) {
        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        let h = b.shading(&shading);
        b.paint_shading(p, h);
        let mut out = Vec::new();
        let error = b.build(&mut out).expect_err("非法网格片渐变应被拒绝");
        assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
        assert!(error.to_string().contains(expected), "{error}");
        assert!(out.is_empty(), "构建失败时不应写出任何字节");
    }

    rejected(
        Shading::Coons(CoonsPatchShading::new(ColorSpace::DeviceRGB, Vec::new())),
        "requires at least one patch",
    );
    rejected(
        Shading::TensorProduct(TensorProductPatchShading::new(
            ColorSpace::DeviceRGB,
            Vec::new(),
        )),
        "requires at least one patch",
    );
    rejected(
        Shading::Coons(coons_patch_shading().with_bits(8, 8, 3)),
        "BitsPerFlag must be 2, 4 or 8",
    );
    rejected(
        Shading::Coons(coons_patch_shading().with_bits(17, 8, 8)),
        "BitsPerCoordinate must be one of 1/2/4/8/12/16/24/32",
    );
    rejected(
        Shading::Coons(CoonsPatchShading::new(
            ColorSpace::DeviceRGB,
            vec![CoonsPatch::new(
                coons_points(),
                [
                    vec![0.0, 0.0],
                    vec![1.0, 0.0],
                    vec![0.0, 1.0],
                    vec![0.0, 0.0],
                ],
            )],
        )),
        "corner colour must have 3 component(s)",
    );
    let mut points = coons_points();
    points[0] = [f64::NAN, 0.0];
    rejected(
        Shading::Coons(CoonsPatchShading::new(
            ColorSpace::DeviceRGB,
            vec![CoonsPatch::new(points, patch_colors())],
        )),
        "control point coordinates must be finite",
    );
    rejected(
        Shading::Coons(CoonsPatchShading::new(
            ColorSpace::DeviceRGB,
            vec![CoonsPatch::new([[5.0, 0.0]; 12], patch_colors())],
        )),
        "non-degenerate",
    );
    rejected(
        Shading::TensorProduct(tensor_patch_shading().with_bits(8, 8, 3)),
        "BitsPerFlag must be 2, 4 or 8",
    );
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

#[test]
fn invalid_mesh_shading_parameters_fail_the_build() {
    fn rejected(shading: Shading, expected: &str) {
        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        let h = b.shading(&shading);
        b.paint_shading(p, h);
        let mut out = Vec::new();
        let error = b.build(&mut out).expect_err("非法网格渐变应被拒绝");
        assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
        assert!(error.to_string().contains(expected), "{error}");
        assert!(out.is_empty(), "构建失败时不应写出任何字节");
    }

    rejected(
        Shading::FreeForm(FreeFormMeshShading::new(
            ColorSpace::DeviceRGB,
            vec![MeshVertex::new(0.0, 0.0, vec![0.0, 0.0, 0.0]); 4],
        )),
        "groups of 3",
    );
    rejected(
        Shading::FreeForm(free_form_mesh().with_bits(8, 8, 3)),
        "BitsPerFlag must be 2, 4 or 8",
    );
    rejected(
        Shading::FreeForm(free_form_mesh().with_bits(17, 8, 8)),
        "BitsPerCoordinate must be one of 1/2/4/8/12/16/24/32",
    );
    rejected(
        Shading::FreeForm(FreeFormMeshShading::new(
            ColorSpace::DeviceRGB,
            vec![
                MeshVertex::new(0.0, 0.0, vec![0.0, 0.0]),
                MeshVertex::new(10.0, 0.0, vec![0.0, 0.0]),
                MeshVertex::new(0.0, 10.0, vec![0.0, 0.0]),
            ],
        )),
        "must have 3 colour component(s)",
    );
    rejected(
        Shading::FreeForm(FreeFormMeshShading::new(
            ColorSpace::DeviceGray,
            vec![
                MeshVertex::new(0.0, 0.0, vec![1.5]),
                MeshVertex::new(10.0, 0.0, vec![0.0]),
                MeshVertex::new(0.0, 10.0, vec![0.0]),
            ],
        )),
        "outside [0, 1]",
    );
    rejected(
        Shading::FreeForm(FreeFormMeshShading::new(
            ColorSpace::DeviceGray,
            vec![
                MeshVertex::new(5.0, 0.0, vec![0.0]),
                MeshVertex::new(5.0, 10.0, vec![0.0]),
                MeshVertex::new(5.0, 20.0, vec![0.0]),
            ],
        )),
        "non-degenerate",
    );
    rejected(
        Shading::Lattice(LatticeMeshShading::new(
            ColorSpace::DeviceGray,
            1,
            vec![
                MeshVertex::new(0.0, 0.0, vec![0.0]),
                MeshVertex::new(10.0, 0.0, vec![0.0]),
            ],
        )),
        "VerticesPerRow must be at least 2",
    );
    rejected(
        Shading::Lattice(LatticeMeshShading::new(
            ColorSpace::DeviceGray,
            2,
            vec![
                MeshVertex::new(0.0, 0.0, vec![0.0]),
                MeshVertex::new(10.0, 0.0, vec![0.0]),
                MeshVertex::new(0.0, 10.0, vec![0.0]),
            ],
        )),
        "must fill whole rows",
    );
    rejected(
        Shading::Lattice(LatticeMeshShading::new(
            ColorSpace::DeviceGray,
            2,
            vec![
                MeshVertex::new(0.0, 0.0, vec![0.0]),
                MeshVertex::new(10.0, 0.0, vec![0.0]),
            ],
        )),
        "at least two rows",
    );
}

/// 1 输入 3 输出的颜色映射函数（用于带 `/Function` 的 RGB 网格）。
fn rgb_function() -> Function {
    Function::Exponential(
        ExponentialFunction::new(vec![0.0, 1.0], 1.0)
            .with_c0(vec![1.0, 0.0, 0.0])
            .with_c1(vec![0.0, 0.0, 1.0]),
    )
}

fn parametric_vertices() -> Vec<MeshVertex> {
    vec![
        MeshVertex::new(0.0, 0.0, vec![0.0]),
        MeshVertex::new(10.0, 0.0, vec![0.5]),
        MeshVertex::new(0.0, 10.0, vec![1.0]),
    ]
}

/// 带 `/Function` 的 Type 4 渐变：`/Decode` 只含一对颜色范围，
/// 每个顶点只编码一个参数变量。
#[test]
fn mesh_shading_with_function_writes_one_component_per_vertex() {
    let mesh = FreeFormMeshShading::new(ColorSpace::DeviceRGB, parametric_vertices())
        .with_function(rgb_function())
        .with_decode([0.0, 10.0, 0.0, 10.0])
        .with_bits(8, 8, 8);
    let s = render_with_shading(Shading::FreeForm(mesh));

    assert!(s.contains("/ShadingType 4"), "{s}");
    assert!(s.contains("/Function <<"), "{s}");
    assert!(s.contains("/FunctionType 2"), "{s}");
    assert!(s.contains("/C0 [1.0 0.0 0.0]"), "{s}");
    assert!(s.contains("/C1 [0.0 0.0 1.0]"), "{s}");
    assert!(
        s.contains("/Decode [0.0 10.0 0.0 10.0 0.0 1.0]"),
        "函数存在时只写一对颜色范围: {s}"
    );
    assert!(
        s.contains("/Length 12"),
        "3 顶点 × (8+8+8+8) 位 = 12 字节: {s}"
    );
}

/// 带 `/Function` 的 Type 6 网格片：每个角点只编码一个参数变量。
#[test]
fn patch_shading_with_function_writes_one_component_per_corner() {
    let coons = CoonsPatchShading::new(
        ColorSpace::DeviceRGB,
        vec![CoonsPatch::new(
            coons_points(),
            [vec![0.0], vec![0.25], vec![0.5], vec![1.0]],
        )],
    )
    .with_function(rgb_function())
    .with_bits(8, 8, 8);
    let s = render_with_shading(Shading::Coons(coons));

    assert!(s.contains("/ShadingType 6"), "{s}");
    assert!(s.contains("/Function <<"), "{s}");
    assert!(s.contains("/Decode [0.0 30.0 0.0 30.0 0.0 1.0]"), "{s}");
    assert!(
        s.contains("/Length 29"),
        "1 片 × (8 + 12×2×8 + 4×1×8) 位 = 29 字节: {s}"
    );
}

/// 函数是流对象（Type 0）时，网格字典以间接引用写 `/Function`。
#[test]
fn mesh_function_stream_is_written_as_indirect_object() {
    let function = Function::Sampled(SampledFunction::new(
        vec![0.0, 1.0],
        vec![0.0, 1.0, 0.0, 1.0, 0.0, 1.0],
        vec![2],
        8,
        vec![0.0, 0.0, 0.0, 255.0, 255.0, 255.0],
    ));
    let lattice = LatticeMeshShading::new(
        ColorSpace::DeviceRGB,
        2,
        vec![
            MeshVertex::new(0.0, 0.0, vec![0.0]),
            MeshVertex::new(10.0, 0.0, vec![0.0]),
            MeshVertex::new(0.0, 10.0, vec![1.0]),
            MeshVertex::new(10.0, 10.0, vec![1.0]),
        ],
    )
    .with_function(function)
    .with_decode([0.0, 10.0, 0.0, 10.0])
    .with_bits(8, 8);
    let s = render_with_shading(Shading::Lattice(lattice));

    assert!(s.contains("/Function 6 0 R"), "{s}");
    assert!(s.contains("6 0 obj"), "{s}");
    assert!(s.contains("/FunctionType 0"), "{s}");
    assert!(s.contains("/Size [2]"), "{s}");
    assert!(s.contains("/BitsPerSample 8"), "{s}");
    assert!(s.contains("/Decode [0.0 10.0 0.0 10.0 0.0 1.0]"), "{s}");
}

/// 函数数组形式：每个颜色分量一个 1 输入 1 输出函数。
#[test]
fn mesh_function_array_writes_one_function_per_component() {
    let mesh = FreeFormMeshShading::new(ColorSpace::DeviceRGB, parametric_vertices())
        .with_component_functions(vec![
            exponential(vec![0.0], vec![1.0]),
            exponential(vec![0.0], vec![0.5]),
            exponential(vec![1.0], vec![0.0]),
        ])
        .with_decode([0.0, 10.0, 0.0, 10.0])
        .with_bits(8, 8, 8);
    let s = render_with_shading(Shading::FreeForm(mesh));

    assert!(s.contains("/Function ["), "{s}");
    assert_eq!(s.matches("/FunctionType 2").count(), 3, "{s}");
    assert!(s.contains("/C0 [0.0]"), "{s}");
    assert!(s.contains("/C1 [0.5]"), "{s}");
    assert!(s.contains("/C0 [1.0]"), "{s}");
    assert!(
        s.contains("/Decode [0.0 10.0 0.0 10.0 0.0 1.0]"),
        "函数数组存在时只写一对颜色范围: {s}"
    );
    assert!(
        s.contains("/Length 12"),
        "3 顶点 × (8+8+8+8) 位 = 12 字节: {s}"
    );
}

/// Type 6 网格片同样接受函数数组，每个角点只编码一个参数变量。
#[test]
fn patch_function_array_writes_corner_functions() {
    let coons = CoonsPatchShading::new(
        ColorSpace::DeviceRGB,
        vec![CoonsPatch::new(
            coons_points(),
            [vec![0.0], vec![0.25], vec![0.5], vec![1.0]],
        )],
    )
    .with_component_functions(vec![
        exponential(vec![0.0], vec![1.0]),
        exponential(vec![0.0], vec![1.0]),
        exponential(vec![0.0], vec![1.0]),
    ])
    .with_bits(8, 8, 8);
    let s = render_with_shading(Shading::Coons(coons));

    assert!(s.contains("/ShadingType 6"), "{s}");
    assert!(s.contains("/Function ["), "{s}");
    assert_eq!(s.matches("/FunctionType 2").count(), 3, "{s}");
    assert!(s.contains("/Decode [0.0 30.0 0.0 30.0 0.0 1.0]"), "{s}");
    assert!(
        s.contains("/Length 29"),
        "1 片 × (8 + 12×2×8 + 4×1×8) 位 = 29 字节: {s}"
    );
}

/// 函数数组必须恰好包含颜色空间分量数个函数，且每项都是 1 进 1 出。
#[test]
fn invalid_mesh_function_arrays_fail_the_build() {
    fn rejected(shading: Shading, expected: &str) {
        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        let h = b.shading(&shading);
        b.paint_shading(p, h);
        let mut out = Vec::new();
        let error = b.build(&mut out).expect_err("非法函数数组应被拒绝");
        assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
        assert!(error.to_string().contains(expected), "{error}");
        assert!(out.is_empty(), "构建失败时不应写出任何字节");
    }

    let single = || exponential(vec![0.0], vec![1.0]);
    rejected(
        Shading::FreeForm(
            FreeFormMeshShading::new(ColorSpace::DeviceRGB, parametric_vertices())
                .with_component_functions(vec![single(), single()])
                .with_decode([0.0, 10.0, 0.0, 10.0]),
        ),
        "Function array must contain 3 function(s), got 2",
    );
    let two_input = Function::Exponential(
        ExponentialFunction::new(vec![0.0, 1.0, 0.0, 1.0], 1.0)
            .with_c0(vec![0.0])
            .with_c1(vec![1.0]),
    );
    rejected(
        Shading::FreeForm(
            FreeFormMeshShading::new(ColorSpace::DeviceGray, parametric_vertices())
                .with_component_functions(vec![two_input])
                .with_decode([0.0, 10.0, 0.0, 10.0]),
        ),
        "Function array entry 0 must take 1 input, got 2",
    );
    rejected(
        Shading::FreeForm(
            FreeFormMeshShading::new(ColorSpace::DeviceGray, parametric_vertices())
                .with_component_functions(vec![exponential(vec![0.0, 0.0], vec![1.0, 1.0])])
                .with_decode([0.0, 10.0, 0.0, 10.0]),
        ),
        "Function array entry 0 must produce 1 output, got 2",
    );
    rejected(
        Shading::FreeForm(
            FreeFormMeshShading::new(
                ColorSpace::Indexed(Indexed::new(ColorSpace::DeviceGray, 0, vec![0])),
                parametric_vertices(),
            )
            .with_component_functions(vec![single()])
            .with_decode([0.0, 10.0, 0.0, 10.0]),
        ),
        "must not be used with an Indexed colour space",
    );
}

/// `/Function` 必须单输入、输出数匹配颜色空间，且不得与 Indexed 组合；
/// 顶点/角点颜色在函数模式下必须恰好一个参数变量。
#[test]
fn invalid_mesh_function_parameters_fail_the_build() {
    fn rejected(shading: Shading, expected: &str) {
        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        let h = b.shading(&shading);
        b.paint_shading(p, h);
        let mut out = Vec::new();
        let error = b.build(&mut out).expect_err("非法网格函数应被拒绝");
        assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
        assert!(error.to_string().contains(expected), "{error}");
        assert!(out.is_empty(), "构建失败时不应写出任何字节");
    }

    rejected(
        Shading::FreeForm(
            FreeFormMeshShading::new(
                ColorSpace::Indexed(Indexed::new(ColorSpace::DeviceGray, 0, vec![0])),
                parametric_vertices(),
            )
            .with_function(rgb_function())
            .with_decode([0.0, 10.0, 0.0, 10.0]),
        ),
        "must not be used with an Indexed colour space",
    );
    let two_input = Function::Exponential(
        ExponentialFunction::new(vec![0.0, 1.0, 0.0, 1.0], 1.0)
            .with_c0(vec![0.0])
            .with_c1(vec![1.0]),
    );
    rejected(
        Shading::FreeForm(
            FreeFormMeshShading::new(ColorSpace::DeviceGray, parametric_vertices())
                .with_function(two_input)
                .with_decode([0.0, 10.0, 0.0, 10.0]),
        ),
        "Function must take 1 input",
    );
    rejected(
        Shading::FreeForm(
            FreeFormMeshShading::new(ColorSpace::DeviceGray, parametric_vertices())
                .with_function(rgb_function())
                .with_decode([0.0, 10.0, 0.0, 10.0]),
        ),
        "Function must produce 1 output(s)",
    );
    rejected(
        Shading::FreeForm(
            FreeFormMeshShading::new(
                ColorSpace::DeviceRGB,
                vec![
                    MeshVertex::new(0.0, 0.0, vec![0.0, 0.0, 0.0]),
                    MeshVertex::new(10.0, 0.0, vec![0.0, 0.0, 0.0]),
                    MeshVertex::new(0.0, 10.0, vec![0.0, 0.0, 0.0]),
                ],
            )
            .with_function(rgb_function())
            .with_decode([0.0, 10.0, 0.0, 10.0]),
        ),
        "vertex must have 1 colour component(s)",
    );
    rejected(
        Shading::FreeForm(
            FreeFormMeshShading::new(
                ColorSpace::DeviceGray,
                vec![
                    MeshVertex::new(0.0, 0.0, vec![0.0]),
                    MeshVertex::new(10.0, 0.0, vec![1.5]),
                    MeshVertex::new(0.0, 10.0, vec![0.0]),
                ],
            )
            .with_function(exponential(vec![0.0], vec![1.0]))
            .with_decode([0.0, 10.0, 0.0, 10.0]),
        ),
        "outside [0, 1]",
    );
    rejected(
        Shading::Coons(
            CoonsPatchShading::new(
                ColorSpace::DeviceRGB,
                vec![CoonsPatch::new(coons_points(), patch_colors())],
            )
            .with_function(rgb_function())
            .with_bits(8, 8, 8),
        ),
        "corner colour must have 1 component(s)",
    );
}

/// 位深必须取规范枚举值：坐标允许 24/32 位，分量上限 16 位。
#[test]
fn mesh_bit_depths_are_limited_to_specification_values() {
    let wide = FreeFormMeshShading::new(
        ColorSpace::DeviceGray,
        vec![
            MeshVertex::new(0.0, 0.0, vec![0.0]),
            MeshVertex::new(10.0, 0.0, vec![0.5]),
            MeshVertex::new(0.0, 10.0, vec![1.0]),
        ],
    )
    .with_decode([0.0, 10.0, 0.0, 10.0])
    .with_bits(24, 12, 8);
    let s = render_with_shading(Shading::FreeForm(wide));
    assert!(s.contains("/BitsPerCoordinate 24"), "{s}");
    assert!(s.contains("/BitsPerComponent 12"), "{s}");
    assert!(
        s.contains("/Length 26"),
        "3 顶点 × (8+24+24+12) 位 = 26 字节: {s}"
    );

    let widest = FreeFormMeshShading::new(
        ColorSpace::DeviceGray,
        vec![
            MeshVertex::new(0.0, 0.0, vec![0.0]),
            MeshVertex::new(10.0, 0.0, vec![0.5]),
            MeshVertex::new(0.0, 10.0, vec![1.0]),
        ],
    )
    .with_decode([0.0, 10.0, 0.0, 10.0])
    .with_bits(32, 8, 8);
    let s = render_with_shading(Shading::FreeForm(widest));
    assert!(s.contains("/BitsPerCoordinate 32"), "{s}");
    assert!(
        s.contains("/Length 30"),
        "3 顶点 × (8+32+32+8) 位 = 30 字节: {s}"
    );

    fn rejected(shading: Shading, expected: &str) {
        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        let h = b.shading(&shading);
        b.paint_shading(p, h);
        let mut out = Vec::new();
        let error = b.build(&mut out).expect_err("非法位深应被拒绝");
        assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
        assert!(error.to_string().contains(expected), "{error}");
        assert!(out.is_empty());
    }

    rejected(
        Shading::FreeForm(free_form_mesh().with_bits(3, 8, 8)),
        "BitsPerCoordinate must be one of 1/2/4/8/12/16/24/32",
    );
    rejected(
        Shading::FreeForm(free_form_mesh().with_bits(8, 24, 8)),
        "BitsPerComponent must be one of 1/2/4/8/12/16",
    );
}

/// 所有渐变共有的 `/Background`、`/BBox` 与 `/AntiAlias` 写入直接字典。
#[test]
fn shading_options_write_common_entries_to_direct_dictionary() {
    let shading = Shading::Axial(
        AxialShading::new(
            ColorSpace::DeviceRGB,
            [0.0, 0.0, 100.0, 0.0],
            exponential(vec![0.0, 0.0, 0.0], vec![1.0, 1.0, 1.0]),
        )
        .with_options(
            ShadingOptions::new()
                .with_background([0.25, 0.5, 0.75])
                .with_bbox([10.0, 20.0, 110.0, 70.0])
                .with_anti_alias(true),
        ),
    );
    let s = render_with_shading(shading);

    assert!(s.contains("/Background [0.25 0.5 0.75]"), "{s}");
    assert!(s.contains("/BBox [10.0 20.0 110.0 70.0]"), "{s}");
    assert!(s.contains("/AntiAlias true"), "{s}");
}

#[test]
fn shading_options_are_omitted_when_unset() {
    let s = render_with_shading(axial_shading());

    assert!(!s.contains("/Background"), "{s}");
    assert!(!s.contains("/BBox"), "{s}");
    assert!(!s.contains("/AntiAlias"), "{s}");
}

/// 网格渐变（Type 5）与网格片渐变（Type 6）的流字典同样携带公共条目。
#[test]
fn shading_options_apply_to_mesh_stream_dictionaries() {
    let options = ShadingOptions::new()
        .with_background([0.0, 0.0, 0.0])
        .with_bbox([0.0, 0.0, 100.0, 100.0])
        .with_anti_alias(false);
    let lattice = Shading::Lattice(
        LatticeMeshShading::new(
            ColorSpace::DeviceRGB,
            2,
            vec![
                MeshVertex::new(0.0, 0.0, vec![0.0, 0.0, 0.0]),
                MeshVertex::new(10.0, 0.0, vec![1.0, 0.0, 0.0]),
                MeshVertex::new(0.0, 10.0, vec![0.0, 1.0, 0.0]),
                MeshVertex::new(10.0, 10.0, vec![0.0, 0.0, 1.0]),
            ],
        )
        .with_bits(8, 8)
        .with_options(options.clone()),
    );
    let s = render_with_shading(lattice);
    assert!(s.contains("/ShadingType 5"), "{s}");
    assert!(s.contains("/Background [0.0 0.0 0.0]"), "{s}");
    assert!(s.contains("/BBox [0.0 0.0 100.0 100.0]"), "{s}");
    assert!(s.contains("/AntiAlias false"), "{s}");

    let coons = Shading::Coons(coons_patch_shading().with_options(options));
    let s = render_with_shading(coons);
    assert!(s.contains("/ShadingType 6"), "{s}");
    assert!(s.contains("/Background [0.0 0.0 0.0]"), "{s}");
    assert!(s.contains("/BBox [0.0 0.0 100.0 100.0]"), "{s}");
    assert!(s.contains("/AntiAlias false"), "{s}");
}

#[test]
fn invalid_shading_options_fail_the_build() {
    fn rejected(shading: Shading, expected: &str) {
        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        let h = b.shading(&shading);
        b.paint_shading(p, h);
        let mut out = Vec::new();
        let error = b.build(&mut out).expect_err("非法渐变公共条目应被拒绝");
        assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
        assert!(error.to_string().contains(expected), "{error}");
        assert!(out.is_empty(), "构建失败时不应写出任何字节");
    }

    let axial_with = |options: ShadingOptions| {
        Shading::Axial(
            AxialShading::new(
                ColorSpace::DeviceGray,
                [0.0, 0.0, 100.0, 0.0],
                exponential(vec![0.0], vec![1.0]),
            )
            .with_options(options),
        )
    };

    rejected(
        axial_with(ShadingOptions::new().with_background([0.0, 1.0])),
        "shading Background must have 1 component(s), got 2",
    );
    rejected(
        axial_with(ShadingOptions::new().with_background([f64::NAN])),
        "shading Background must contain finite numbers",
    );
    rejected(
        axial_with(ShadingOptions::new().with_bbox([10.0, 0.0, 10.0, 10.0])),
        "shading BBox minimum must be less than maximum",
    );
    rejected(
        Shading::Coons(
            coons_patch_shading().with_options(ShadingOptions::new().with_background([0.0, 0.0])),
        ),
        "shading Background must have 3 component(s), got 2",
    );
}
