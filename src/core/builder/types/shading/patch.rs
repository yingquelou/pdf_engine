//! Type 6–7 网格片渐变构造：Coons 网格与张量积网格及其控制点模型。

use crate::core::builder::types::{ColorSpace, Function};

use super::mesh::MeshFunction;
use super::options::ShadingOptions;
use super::validate::{validate_patch_colors, validate_patch_common, validate_patch_points};

/// Coons 网格片：12 个控制点与 4 个角点颜色。
///
/// 控制点沿四条三次贝塞尔边界排列（`p0`–`p3`、`p3`–`p6`、`p6`–`p9`、
/// `p9`–`p0`）；颜色依次对应 `p0`、`p3`、`p6`、`p9`。
#[derive(Debug, Clone, PartialEq)]
pub struct CoonsPatch {
    pub points: [[f64; 2]; 12],
    pub colors: [Vec<f64>; 4],
}

impl CoonsPatch {
    pub fn new(points: [[f64; 2]; 12], colors: [Vec<f64>; 4]) -> Self {
        Self { points, colors }
    }
}

/// 张量积网格片：16 个控制点（4×4）与 4 个角点颜色。
///
/// 控制点按行优先排列；颜色依次对应 `p0`、`p3`、`p12`、`p15`。
#[derive(Debug, Clone, PartialEq)]
pub struct TensorProductPatch {
    pub points: [[f64; 2]; 16],
    pub colors: [Vec<f64>; 4],
}

impl TensorProductPatch {
    pub fn new(points: [[f64; 2]; 16], colors: [Vec<f64>; 4]) -> Self {
        Self { points, colors }
    }
}

/// Type 6 Coons 网格渐变（`/ShadingType 6`）。
///
/// 每个网格片独立写出（标志恒为 0，不复用相邻网格片的共享边）。
#[derive(Debug, Clone, PartialEq)]
pub struct CoonsPatchShading {
    pub color_space: ColorSpace,
    pub patches: Vec<CoonsPatch>,
    /// 可选颜色映射函数；存在时每个角点只携带一个参数变量。
    pub function: Option<MeshFunction>,
    /// 坐标采样范围 `[xmin xmax ymin ymax]`；省略时取控制点紧包围盒。
    pub decode: Option<[f64; 4]>,
    pub bits_per_coordinate: u8,
    pub bits_per_component: u8,
    pub bits_per_flag: u8,
    pub options: ShadingOptions,
}

impl CoonsPatchShading {
    pub fn new(color_space: ColorSpace, patches: Vec<CoonsPatch>) -> Self {
        Self {
            color_space,
            patches,
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
        let role = "type 6 shading";
        if self.patches.is_empty() {
            return Err(format!("{role} requires at least one patch"));
        }
        if !matches!(self.bits_per_flag, 2 | 4 | 8) {
            return Err(format!("{role} BitsPerFlag must be 2, 4 or 8"));
        }
        validate_patch_common(
            role,
            &self.color_space,
            self.function.as_ref(),
            self.decode,
            self.bits_per_coordinate,
            self.bits_per_component,
        )?;
        for patch in &self.patches {
            validate_patch_points(role, &patch.points)?;
            validate_patch_colors(
                role,
                &self.color_space,
                &patch.colors,
                self.function.is_some(),
            )?;
        }
        self.options.validate(&self.color_space)
    }
}

/// Type 7 张量积网格渐变（`/ShadingType 7`）。
///
/// 每个网格片独立写出（标志恒为 0，不复用相邻网格片的共享边）。
#[derive(Debug, Clone, PartialEq)]
pub struct TensorProductPatchShading {
    pub color_space: ColorSpace,
    pub patches: Vec<TensorProductPatch>,
    /// 可选颜色映射函数；存在时每个角点只携带一个参数变量。
    pub function: Option<MeshFunction>,
    /// 坐标采样范围 `[xmin xmax ymin ymax]`；省略时取控制点紧包围盒。
    pub decode: Option<[f64; 4]>,
    pub bits_per_coordinate: u8,
    pub bits_per_component: u8,
    pub bits_per_flag: u8,
    pub options: ShadingOptions,
}

impl TensorProductPatchShading {
    pub fn new(color_space: ColorSpace, patches: Vec<TensorProductPatch>) -> Self {
        Self {
            color_space,
            patches,
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
        let role = "type 7 shading";
        if self.patches.is_empty() {
            return Err(format!("{role} requires at least one patch"));
        }
        if !matches!(self.bits_per_flag, 2 | 4 | 8) {
            return Err(format!("{role} BitsPerFlag must be 2, 4 or 8"));
        }
        validate_patch_common(
            role,
            &self.color_space,
            self.function.as_ref(),
            self.decode,
            self.bits_per_coordinate,
            self.bits_per_component,
        )?;
        for patch in &self.patches {
            validate_patch_points(role, &patch.points)?;
            validate_patch_colors(
                role,
                &self.color_space,
                &patch.colors,
                self.function.is_some(),
            )?;
        }
        self.options.validate(&self.color_space)
    }
}
