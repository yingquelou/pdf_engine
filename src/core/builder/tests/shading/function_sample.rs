//! `/Function` 样本函数与函数数组的写出形态（每顶点/角点单参数变量、
//! 流函数间接引用、函数数组一对一分量）、位宽规范枚举取值，以及函数
//! 输入/输出数、Indexed 组合与参数变量个数不匹配时的构建期拒绝。

use super::super::*;
use super::support::{
    coons_points, exponential, free_form_mesh, patch_colors, render_with_shading,
};

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
