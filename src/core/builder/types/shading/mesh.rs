//! 网格渐变（Type 4–7）的共享模型：顶点、颜色映射函数与 `/Decode`
//! 各分量的解码范围。

use crate::core::builder::types::{ColorSpace, Function};

/// 网格顶点：用户空间坐标与颜色分量。
///
/// `color` 按颜色空间分量顺序排列；多数空间的取值范围为 `0..1`，
/// Lab 为 `[0 100]` 与 `/Range` 指定的 a/b 范围，Indexed 为调色板下标。
#[derive(Debug, Clone, PartialEq)]
pub struct MeshVertex {
    pub x: f64,
    pub y: f64,
    pub color: Vec<f64>,
}

impl MeshVertex {
    pub fn new(x: f64, y: f64, color: impl Into<Vec<f64>>) -> Self {
        Self {
            x,
            y,
            color: color.into(),
        }
    }
}

/// 网格颜色分量的解码范围：对应 `/Decode` 中每个分量的 `[min max]`。
pub(crate) fn mesh_color_ranges(color_space: &ColorSpace) -> Vec<[f64; 2]> {
    match color_space {
        ColorSpace::Lab(lab) => {
            let [a_min, a_max, b_min, b_max] = lab.range.unwrap_or([-100.0, 100.0, -100.0, 100.0]);
            vec![[0.0, 100.0], [a_min, a_max], [b_min, b_max]]
        }
        ColorSpace::Indexed(indexed) => vec![[0.0, indexed.hival as f64]],
        ColorSpace::ICCBased(icc) => match &icc.range {
            Some(range) if range.len() == icc.n as usize * 2 => (0..icc.n as usize)
                .map(|i| [range[i * 2], range[i * 2 + 1]])
                .collect(),
            _ => vec![[0.0, 1.0]; icc.n as usize],
        },
        _ => vec![[0.0, 1.0]; color_space.channels()],
    }
}

/// 网格顶点的颜色解码范围：使用 `/Function` 时只解码一个参数变量。
pub(crate) fn mesh_component_ranges(color_space: &ColorSpace, has_function: bool) -> Vec<[f64; 2]> {
    if has_function {
        vec![[0.0, 1.0]]
    } else {
        mesh_color_ranges(color_space)
    }
}

/// 网格渐变的颜色映射函数（Table 82）：单个 1 输入 n 输出函数，
/// 或按颜色分量顺序排列的 n 个 1 输入 1 输出函数。
#[derive(Debug, Clone, PartialEq)]
pub enum MeshFunction {
    /// 单个函数：输入参数变量，输出各颜色分量。
    Single(Function),
    /// 函数数组：每个函数把参数变量映射到一个颜色分量。
    Components(Vec<Function>),
}
