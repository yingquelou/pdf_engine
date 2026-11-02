//! 颜色空间读端模型（ISO 32000-1 §8.6.4–8.6.8）。
//!
//! 从 [`Document`] 解析设备、CIE-Based（CalGray / CalRGB / Lab / ICCBased）、
//! Indexed、Separation / DeviceN 与 Pattern 颜色空间，支持间接引用、
//! 流对象与（已解密上下文下的）过滤解码；Separation / DeviceN 的 tint
//! 变换复用 [`crate::core::functions::Function`] 求值器。
//!
//! [`ColorSpaceModel::decode_samples`] 把图像样本位流解码为规格化原生
//! 分量（Indexed 经调色板展开回基空间，Separation / DeviceN 经 tint
//! 函数映射到备用空间），不应用图像字典的 `/Decode` 矩阵——由调用方
//! 负责。非法颜色空间返回类型化 [`ColorSpaceError`]。

mod error;
mod model;
mod parse;
mod pdf;
mod sample;

pub use error::ColorSpaceError;
pub use model::{
    CalGraySpace, CalRGBSpace, ColorSpaceModel, DefaultColorSpace, DeviceNSpace, IccBasedSpace,
    IndexedSpace, LabSpace, SeparationSpace,
};

#[cfg(test)]
mod tests;
