//! 色彩空间的名称访问器与每像素分量数计算。

use super::color_space::ColorSpace;

impl ColorSpace {
    /// 色彩空间名称（数组形式的 CIE-Based 空间取其首项名称）。
    pub(crate) fn name(&self) -> &'static str {
        match self {
            ColorSpace::DeviceGray => "DeviceGray",
            ColorSpace::DeviceRGB => "DeviceRGB",
            ColorSpace::DeviceCMYK => "DeviceCMYK",
            ColorSpace::CalGray(_) => "CalGray",
            ColorSpace::CalRGB(_) => "CalRGB",
            ColorSpace::Lab(_) => "Lab",
            ColorSpace::ICCBased(_) => "ICCBased",
            ColorSpace::Indexed(_) => "Indexed",
            ColorSpace::Separation(_) => "Separation",
            ColorSpace::DeviceN(_) => "DeviceN",
        }
    }

    /// 设备空间的名字；CIE-Based 空间返回 `None`。
    pub(crate) fn device_name(&self) -> Option<&'static str> {
        match self {
            ColorSpace::DeviceGray => Some("DeviceGray"),
            ColorSpace::DeviceRGB => Some("DeviceRGB"),
            ColorSpace::DeviceCMYK => Some("DeviceCMYK"),
            _ => None,
        }
    }

    /// 每个像素的颜色分量数。
    pub(crate) fn channels(&self) -> usize {
        match self {
            ColorSpace::DeviceGray | ColorSpace::CalGray(_) => 1,
            ColorSpace::DeviceRGB | ColorSpace::CalRGB(_) | ColorSpace::Lab(_) => 3,
            ColorSpace::DeviceCMYK => 4,
            ColorSpace::ICCBased(icc) => icc.n as usize,
            ColorSpace::Indexed(_) => 1,
            ColorSpace::Separation(_) => 1,
            ColorSpace::DeviceN(devicen) => devicen.colorants.len(),
        }
    }
}
