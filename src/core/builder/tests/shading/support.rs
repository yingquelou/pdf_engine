//! 渐变写端测试的跨主题共享夹具：指数函数样本、轴向渐变、着色图案、
//! 渲染辅助与网格/网格片顶点数据。仅服务单一主题的夹具（如
//! `lattice_mesh`、`rgb_function`）随主题留在对应子文件。

use super::super::*;

pub(super) fn exponential(c0: Vec<f64>, c1: Vec<f64>) -> Function {
    Function::Exponential(
        ExponentialFunction::new(vec![0.0, 1.0], 1.0)
            .with_c0(c0)
            .with_c1(c1),
    )
}

pub(super) fn axial_shading() -> Shading {
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

pub(super) fn shading_pattern() -> ShadingPattern {
    ShadingPattern::new(axial_shading()).with_matrix([2.0, 0.0, 0.0, 2.0, 5.0, 5.0])
}

pub(super) fn render_with_shading(shading: Shading) -> String {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let h = b.shading(&shading);
    b.paint_shading(p, h);
    render(&mut b)
}

pub(super) fn free_form_mesh() -> FreeFormMeshShading {
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

pub(super) fn coons_points() -> [[f64; 2]; 12] {
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

pub(super) fn tensor_points() -> [[f64; 2]; 16] {
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

pub(super) fn patch_colors() -> [Vec<f64>; 4] {
    [
        vec![0.0, 0.0, 0.0],
        vec![1.0, 0.0, 0.0],
        vec![0.0, 1.0, 0.0],
        vec![0.0, 0.0, 1.0],
    ]
}

pub(super) fn coons_patch_shading() -> CoonsPatchShading {
    CoonsPatchShading::new(
        ColorSpace::DeviceRGB,
        vec![CoonsPatch::new(coons_points(), patch_colors())],
    )
    .with_bits(8, 8, 8)
}

pub(super) fn tensor_patch_shading() -> TensorProductPatchShading {
    TensorProductPatchShading::new(
        ColorSpace::DeviceRGB,
        vec![TensorProductPatch::new(tensor_points(), patch_colors())],
    )
    .with_bits(8, 8, 8)
}
