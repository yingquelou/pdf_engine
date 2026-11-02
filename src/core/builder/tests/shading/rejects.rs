//! 非法参数在构建期以 `WriteError::InvalidObject` 拒绝且零字节输出：
//! 直接渐变/图案（Domain 非有限、函数进出数、半径为负、Indexed 色彩空间、
//! 矩阵非有限）、网格片（空片列表、位宽枚举、角点分量数、控制点非有限、
//! 退化）、网格（顶点组、颜色范围、VerticesPerRow 行数）与公共条目
//! （Background 分量数/非有限、BBox 最小值不小于最大值）。

use super::super::*;
use super::support::{
    coons_patch_shading, coons_points, exponential, free_form_mesh, patch_colors, shading_pattern,
    tensor_patch_shading,
};

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
