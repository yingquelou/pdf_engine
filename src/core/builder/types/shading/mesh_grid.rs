//! Type 4–5 三角网格渐变构造：自由三角形网格与栅格三角网格。

use crate::core::builder::types::{ColorSpace, Function};

use super::mesh::{MeshFunction, MeshVertex};
use super::options::ShadingOptions;
use super::validate::validate_mesh;

/// Type 4 自由三角网格（Gouraud 着色，`/ShadingType 4`）。
///
/// 顶点按三角形分组（每 3 个一组）。每个顶点前写出 `BitsPerFlag` 位
/// 标志；本实现一律写 0，即每个三角形都独立给出三个顶点，不复用
/// 相邻三角形的共享边。
#[derive(Debug, Clone, PartialEq)]
pub struct FreeFormMeshShading {
    pub color_space: ColorSpace,
    pub vertices: Vec<MeshVertex>,
    /// 可选颜色映射函数；存在时每个顶点只携带一个参数变量。
    pub function: Option<MeshFunction>,
    /// 坐标采样范围 `[xmin xmax ymin ymax]`；省略时取顶点紧包围盒。
    pub decode: Option<[f64; 4]>,
    pub bits_per_coordinate: u8,
    pub bits_per_component: u8,
    pub bits_per_flag: u8,
    pub options: ShadingOptions,
}

impl FreeFormMeshShading {
    pub fn new(color_space: ColorSpace, vertices: Vec<MeshVertex>) -> Self {
        Self {
            color_space,
            vertices,
            function: None,
            decode: None,
            bits_per_coordinate: 16,
            bits_per_component: 8,
            bits_per_flag: 8,
            options: ShadingOptions::new(),
        }
    }

    pub fn with_function(mut self, function: Function) -> Self {
        self.function = Some(MeshFunction::Single(function));
        self
    }

    /// 使用 n 个单输入单输出函数作为颜色映射，每个函数对应一个颜色分量。
    pub fn with_component_functions(mut self, functions: Vec<Function>) -> Self {
        self.function = Some(MeshFunction::Components(functions));
        self
    }

    pub fn with_decode(mut self, decode: [f64; 4]) -> Self {
        self.decode = Some(decode);
        self
    }

    pub fn with_bits(mut self, coordinate: u8, component: u8, flag: u8) -> Self {
        self.bits_per_coordinate = coordinate;
        self.bits_per_component = component;
        self.bits_per_flag = flag;
        self
    }

    pub fn with_options(mut self, options: ShadingOptions) -> Self {
        self.options = options;
        self
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        if !self.vertices.len().is_multiple_of(3) {
            return Err("type 4 shading vertices must come in groups of 3".into());
        }
        if !matches!(self.bits_per_flag, 2 | 4 | 8) {
            return Err("type 4 shading BitsPerFlag must be 2, 4 or 8".into());
        }
        validate_mesh(
            "type 4 shading",
            &self.color_space,
            &self.vertices,
            self.function.as_ref(),
            self.decode,
            self.bits_per_coordinate,
            self.bits_per_component,
        )?;
        self.options.validate(&self.color_space)
    }
}

/// Type 5 栅格三角网格（`/ShadingType 5`）：顶点按等长行排列。
///
/// 每行顶点数由 `vertices_per_row` 指定（至少 2），行数由顶点总数决定
/// （至少 2）；相邻两行之间按规范连线形成三角网格。
#[derive(Debug, Clone, PartialEq)]
pub struct LatticeMeshShading {
    pub color_space: ColorSpace,
    pub vertices_per_row: u16,
    /// 按行优先顺序展开的顶点。
    pub vertices: Vec<MeshVertex>,
    /// 可选颜色映射函数；存在时每个顶点只携带一个参数变量。
    pub function: Option<MeshFunction>,
    /// 坐标采样范围 `[xmin xmax ymin ymax]`；省略时取顶点紧包围盒。
    pub decode: Option<[f64; 4]>,
    pub bits_per_coordinate: u8,
    pub bits_per_component: u8,
    pub options: ShadingOptions,
}

impl LatticeMeshShading {
    pub fn new(color_space: ColorSpace, vertices_per_row: u16, vertices: Vec<MeshVertex>) -> Self {
        Self {
            color_space,
            vertices_per_row,
            vertices,
            function: None,
            decode: None,
            bits_per_coordinate: 16,
            bits_per_component: 8,
            options: ShadingOptions::new(),
        }
    }

    pub fn with_function(mut self, function: Function) -> Self {
        self.function = Some(MeshFunction::Single(function));
        self
    }

    /// 使用 n 个单输入单输出函数作为颜色映射，每个函数对应一个颜色分量。
    pub fn with_component_functions(mut self, functions: Vec<Function>) -> Self {
        self.function = Some(MeshFunction::Components(functions));
        self
    }

    pub fn with_decode(mut self, decode: [f64; 4]) -> Self {
        self.decode = Some(decode);
        self
    }

    pub fn with_bits(mut self, coordinate: u8, component: u8) -> Self {
        self.bits_per_coordinate = coordinate;
        self.bits_per_component = component;
        self
    }

    pub fn with_options(mut self, options: ShadingOptions) -> Self {
        self.options = options;
        self
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.vertices_per_row < 2 {
            return Err("type 5 shading VerticesPerRow must be at least 2".into());
        }
        if !self
            .vertices
            .len()
            .is_multiple_of(self.vertices_per_row as usize)
        {
            return Err("type 5 shading vertices must fill whole rows".into());
        }
        let rows = self.vertices.len() / self.vertices_per_row as usize;
        if rows < 2 {
            return Err("type 5 shading requires at least two rows".into());
        }
        validate_mesh(
            "type 5 shading",
            &self.color_space,
            &self.vertices,
            self.function.as_ref(),
            self.decode,
            self.bits_per_coordinate,
            self.bits_per_component,
        )?;
        self.options.validate(&self.color_space)
    }
}
