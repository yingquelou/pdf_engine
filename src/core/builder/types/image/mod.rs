//! 原始像素图片数据与色彩空间。
//!
//! 按主题目录化拆分（逐行搬运自原单文件）：
//! - `color_space`：[`ColorSpace`] 联合与各 CIE-Based/ICCBased/Indexed/
//!   Separation/DeviceN 参数字典及其构造器；
//! - `channels`：色彩空间名称与每像素分量数；
//! - `color_validate`：色彩空间参数的构建期校验与共享校验助手；
//! - `image_data`：[`ImageData`]、[`ImageMask`]、[`AlternateImage`] 与
//!   [`ImageIntent`] 数据模型；
//! - `options`：[`ImageData`] 的链式可选条目设置（`with_*`）；
//! - `image_validate`：行对齐样本字节数计算与图像条目的构建期校验。

mod channels;
mod color_space;
mod color_validate;
mod image_data;
mod image_validate;
mod options;

pub use color_space::{CalGray, CalRGB, ColorSpace, DeviceN, ICCBased, Indexed, Lab, Separation};
pub use image_data::{AlternateImage, ImageData, ImageIntent, ImageMask};
