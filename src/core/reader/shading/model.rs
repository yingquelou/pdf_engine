//! Shading Type 1–7 与 Pattern（PatternType 1/2）的类型化数据模型。

use crate::core::functions::Function;
use crate::core::reader::color_space::ColorSpaceModel;
use crate::core::types::Dict;

/// 网格顶点记录：标志位、坐标与颜色分量（带 `/Function` 时为单个参数变量）。
#[derive(Debug, Clone, PartialEq)]
pub struct MeshVertex {
    pub flag: u8,
    pub x: f64,
    pub y: f64,
    pub components: Vec<f64>,
}

/// 网格渐变的可选 `/Function`（单函数或按分量的函数数组）。
#[derive(Debug, Clone, PartialEq)]
pub enum MeshFunction {
    /// 单个 1 输入、n 输出函数。
    Single(Function),
    /// n 个 1 输入 1 输出函数，按颜色分量排列。
    Components(Vec<Function>),
}

/// 渐变共有可选条目（Table 72 末三行）。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ShadingOptions {
    pub background: Option<Vec<f64>>,
    pub bbox: Option<[f64; 4]>,
    pub anti_alias: Option<bool>,
}

/// Shading Type 1：函数渐变。
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionShading {
    pub color_space: ColorSpaceModel,
    pub function: Function,
    pub matrix: Option<[f64; 6]>,
    pub options: ShadingOptions,
}

/// Shading Type 2：轴向渐变。
#[derive(Debug, Clone, PartialEq)]
pub struct AxialShading {
    pub color_space: ColorSpaceModel,
    pub function: Function,
    pub coords: [f64; 4],
    /// `/Domain`（缺省 `[(0, 1)]`）。
    pub domain: Vec<(f64, f64)>,
    pub extend: [bool; 2],
    pub options: ShadingOptions,
}

/// Shading Type 3：径向渐变。
#[derive(Debug, Clone, PartialEq)]
pub struct RadialShading {
    pub color_space: ColorSpaceModel,
    pub function: Function,
    pub coords: [f64; 6],
    pub domain: Vec<(f64, f64)>,
    pub extend: [bool; 2],
    pub options: ShadingOptions,
}

/// Shading Type 4：自由形态三角网格（流）。
#[derive(Debug, Clone, PartialEq)]
pub struct FreeFormMeshShading {
    pub color_space: ColorSpaceModel,
    pub function: Option<MeshFunction>,
    pub bits_per_coordinate: usize,
    pub bits_per_components: usize,
    pub bits_per_flag: usize,
    pub decode: Vec<(f64, f64)>,
    /// 原始顶点记录（每 3 个一组构成一个三角形；标志 1/2 表示复用前序顶点）。
    pub vertices: Vec<MeshVertex>,
    pub options: ShadingOptions,
}

/// Shading Type 5：四边形条带网格（流）。
#[derive(Debug, Clone, PartialEq)]
pub struct LatticeMeshShading {
    pub color_space: ColorSpaceModel,
    pub function: Option<MeshFunction>,
    pub bits_per_coordinate: usize,
    pub bits_per_components: usize,
    pub vertices_per_row: usize,
    pub decode: Vec<(f64, f64)>,
    pub vertices: Vec<MeshVertex>,
    pub options: ShadingOptions,
}

/// Shading Type 6：Coons 补丁网格（流，每片 12 条记录）。
#[derive(Debug, Clone, PartialEq)]
pub struct CoonsPatchShading {
    pub color_space: ColorSpaceModel,
    pub function: Option<MeshFunction>,
    pub bits_per_coordinate: usize,
    pub bits_per_components: usize,
    pub bits_per_flag: usize,
    pub decode: Vec<(f64, f64)>,
    pub patches: Vec<[MeshVertex; 12]>,
    pub options: ShadingOptions,
}

/// Shading Type 7：张量积补丁网格（流，每片 16 条记录）。
#[derive(Debug, Clone, PartialEq)]
pub struct TensorProductPatchShading {
    pub color_space: ColorSpaceModel,
    pub function: Option<MeshFunction>,
    pub bits_per_coordinate: usize,
    pub bits_per_components: usize,
    pub bits_per_flag: usize,
    pub decode: Vec<(f64, f64)>,
    pub patches: Vec<[MeshVertex; 16]>,
    pub options: ShadingOptions,
}

/// 类型化 Shading 模型（Type 1–7）。
#[derive(Debug, Clone, PartialEq)]
pub enum ShadingModel {
    Function(FunctionShading),
    Axial(AxialShading),
    Radial(RadialShading),
    FreeFormMesh(FreeFormMeshShading),
    LatticeMesh(LatticeMeshShading),
    CoonsPatch(CoonsPatchShading),
    TensorProductPatch(TensorProductPatchShading),
}

impl ShadingModel {
    /// 着色类型号（1–7）。
    pub fn shading_type(&self) -> u8 {
        match self {
            Self::Function(_) => 1,
            Self::Axial(_) => 2,
            Self::Radial(_) => 3,
            Self::FreeFormMesh(_) => 4,
            Self::LatticeMesh(_) => 5,
            Self::CoonsPatch(_) => 6,
            Self::TensorProductPatch(_) => 7,
        }
    }

    /// 渐变颜色空间。
    pub fn color_space(&self) -> &ColorSpaceModel {
        match self {
            Self::Function(s) => &s.color_space,
            Self::Axial(s) => &s.color_space,
            Self::Radial(s) => &s.color_space,
            Self::FreeFormMesh(s) => &s.color_space,
            Self::LatticeMesh(s) => &s.color_space,
            Self::CoonsPatch(s) => &s.color_space,
            Self::TensorProductPatch(s) => &s.color_space,
        }
    }

    /// 共有可选条目。
    pub fn options(&self) -> &ShadingOptions {
        match self {
            Self::Function(s) => &s.options,
            Self::Axial(s) => &s.options,
            Self::Radial(s) => &s.options,
            Self::FreeFormMesh(s) => &s.options,
            Self::LatticeMesh(s) => &s.options,
            Self::CoonsPatch(s) => &s.options,
            Self::TensorProductPatch(s) => &s.options,
        }
    }
}

/// 平铺图案 `/PaintType`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaintType {
    /// 1：图案内容流自设颜色。
    Colored,
    /// 2：无色，颜色取自 `scn`/`SCN` 分量。
    Uncolored,
}

/// 平铺图案 `/TilingType`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TilingType {
    /// 1：固定间距。
    ConstantSpacing,
    /// 2：非自适应可变间距。
    NonAdaptiveVariableSpacing,
    /// 3：自适应可变间距。
    AdaptiveVariableSpacing,
}

/// PatternType 1：平铺图案（流对象 + 图块内容）。
#[derive(Debug, Clone, PartialEq)]
pub struct TilingPattern {
    pub paint_type: PaintType,
    pub tiling_type: TilingType,
    pub bbox: [f64; 4],
    pub x_step: f64,
    pub y_step: f64,
    pub matrix: Option<[f64; 6]>,
    /// 图块自有 `/Resources`（原样保留的字典）。
    pub resources: Option<Dict>,
    /// 无色图案（PaintType 2）的基颜色空间；彩色图案为 `None`。
    pub color_space: Option<ColorSpaceModel>,
    /// 解码后的图块内容流字节。
    pub content: Vec<u8>,
}

/// PatternType 2：着色图案（字典）。
#[derive(Debug, Clone, PartialEq)]
pub struct ShadingPattern {
    pub matrix: Option<[f64; 6]>,
    pub shading: ShadingModel,
}

/// 类型化 Pattern 模型。
#[derive(Debug, Clone, PartialEq)]
pub enum PatternModel {
    Tiling(TilingPattern),
    Shading(ShadingPattern),
}
