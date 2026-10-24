//! 原始像素图片数据与色彩空间。

/// 图片色彩空间。
///
/// 目前仅支持 DeviceGray（灰度）与 DeviceRGB（真彩），作为
/// [`ImageData::color_space`] 的取值，决定每个像素的通道数。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorSpace {
    /// 设备灰度，每个分量 1 个样本。
    DeviceGray,
    /// 设备 RGB，每个分量 3 个样本。
    DeviceRGB,
}

impl ColorSpace {
    pub(crate) fn as_name(&self) -> &'static str {
        match self {
            ColorSpace::DeviceGray => "DeviceGray",
            ColorSpace::DeviceRGB => "DeviceRGB",
        }
    }
}

/// 原始像素图片数据。
///
/// 流字节即未压缩的逐行像素样本，由 [`crate::core::builder::PdfBuilder::image`]
/// 消费并翻译为 XObject Image 流。关键字段见 [`ImageData::width`]、
/// [`ImageData::color_space`]、[`ImageData::data`]。
#[derive(Debug, Clone, PartialEq)]
pub struct ImageData {
    /// 图像宽度（像素）。
    pub width: u32,
    /// 图像高度（像素）。
    pub height: u32,
    /// 色彩空间（DeviceGray / DeviceRGB）。
    pub color_space: ColorSpace,
    /// 每个颜色分量的位数（通常 8）。
    pub bits: u8,
    /// 原始像素字节，长度应为
    /// `width * height * color_space.bytes_per_pixel() * (bits/8)`。
    pub data: Vec<u8>,
}
