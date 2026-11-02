//! Type 1–7 渐变字典的联合类型与统一访问器。

use crate::core::builder::types::ColorSpace;

use super::gradient::{AxialShading, FunctionShading, RadialShading};
use super::mesh_grid::{FreeFormMeshShading, LatticeMeshShading};
use super::options::ShadingOptions;
use super::patch::{CoonsPatchShading, TensorProductPatchShading};

/// 渐变字典：Type 1–7 之一。
#[derive(Debug, Clone, PartialEq)]
pub enum Shading {
    Function(FunctionShading),
    Axial(AxialShading),
    Radial(RadialShading),
    FreeForm(FreeFormMeshShading),
    Lattice(LatticeMeshShading),
    Coons(CoonsPatchShading),
    TensorProduct(TensorProductPatchShading),
}

impl Shading {
    /// 渐变的 `/ShadingType` 编号。
    pub fn shading_type(&self) -> i64 {
        match self {
            Self::Function(_) => 1,
            Self::Axial(_) => 2,
            Self::Radial(_) => 3,
            Self::FreeForm(_) => 4,
            Self::Lattice(_) => 5,
            Self::Coons(_) => 6,
            Self::TensorProduct(_) => 7,
        }
    }

    pub(crate) fn color_space(&self) -> &ColorSpace {
        match self {
            Self::Function(shading) => &shading.color_space,
            Self::Axial(shading) => &shading.color_space,
            Self::Radial(shading) => &shading.color_space,
            Self::FreeForm(shading) => &shading.color_space,
            Self::Lattice(shading) => &shading.color_space,
            Self::Coons(shading) => &shading.color_space,
            Self::TensorProduct(shading) => &shading.color_space,
        }
    }

    /// 所有类型共有的可选条目。
    pub(crate) fn options(&self) -> &ShadingOptions {
        match self {
            Self::Function(shading) => &shading.options,
            Self::Axial(shading) => &shading.options,
            Self::Radial(shading) => &shading.options,
            Self::FreeForm(shading) => &shading.options,
            Self::Lattice(shading) => &shading.options,
            Self::Coons(shading) => &shading.options,
            Self::TensorProduct(shading) => &shading.options,
        }
    }

    /// 网格渐变（Type 4–7）只能写成流对象；其余类型是直接字典。
    pub(crate) fn is_stream(&self) -> bool {
        matches!(
            self,
            Self::FreeForm(_) | Self::Lattice(_) | Self::Coons(_) | Self::TensorProduct(_)
        )
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        match self {
            Self::Function(shading) => shading.validate(),
            Self::Axial(shading) => shading.validate(),
            Self::Radial(shading) => shading.validate(),
            Self::FreeForm(shading) => shading.validate(),
            Self::Lattice(shading) => shading.validate(),
            Self::Coons(shading) => shading.validate(),
            Self::TensorProduct(shading) => shading.validate(),
        }
    }
}
