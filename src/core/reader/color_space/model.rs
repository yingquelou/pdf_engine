//! 类型化颜色空间模型：各 CIE / ICC / 索引 / 色名空间的参数字典结构、
//! [`ColorSpaceModel`] 判别枚举与页面默认颜色空间种类。

use crate::core::functions::Function;

/// CalGray（§8.6.5.2）。
#[derive(Debug, Clone, PartialEq)]
pub struct CalGraySpace {
    pub white_point: Vec<f64>,
    pub black_point: Vec<f64>,
    pub gamma: f64,
}

/// CalRGB（§8.6.5.3）。
#[derive(Debug, Clone, PartialEq)]
pub struct CalRGBSpace {
    pub white_point: Vec<f64>,
    pub black_point: Vec<f64>,
    pub gamma: Vec<f64>,
    pub matrix: Vec<f64>,
}

/// Lab（§8.6.5.4）。
#[derive(Debug, Clone, PartialEq)]
pub struct LabSpace {
    pub white_point: Vec<f64>,
    pub black_point: Vec<f64>,
    /// 原生值域，4 个数折叠为 2 个 `(min, max)`（a*、b*；L 固定 0..100）。
    pub range: Vec<(f64, f64)>,
}

/// ICCBased（§8.6.5.5）。
#[derive(Debug, Clone, PartialEq)]
pub struct IccBasedSpace {
    /// `/N`：通道数（1–4）。
    pub n: usize,
    /// 原生值域（缺 `/Range` 时为 n 个 `(0, 1)`）。
    pub range: Vec<(f64, f64)>,
    /// 备用设备空间（缺 `/Alternate` 时按 `/N` 取 DeviceGray/RGB/CMYK）。
    pub alternate: Box<ColorSpaceModel>,
    /// ICC 配置文件原始字节。
    pub profile: Vec<u8>,
}

/// Indexed（§8.6.6.1? 实为 §8.6.6 之前的 §8.6.6.1 索引色）：调色板颜色空间。
#[derive(Debug, Clone, PartialEq)]
pub struct IndexedSpace {
    pub base: Box<ColorSpaceModel>,
    /// 最大调色板索引（hival）。
    pub hival: usize,
    /// 调色板字节，每条基空间分量宽度。
    pub lookup: Vec<u8>,
}

/// Separation（§8.6.6.2）：单色名空间。
#[derive(Debug, Clone, PartialEq)]
pub struct SeparationSpace {
    /// 色名（`/All`、`/None` 或专色名）。
    pub colorant: String,
    pub alternate: Box<ColorSpaceModel>,
    /// tint 变换；规范上应为函数，缺失/为空时按恒等处理。
    pub tint: Option<Function>,
}

/// DeviceN（§8.6.6.3）：多色名空间。
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceNSpace {
    pub colorants: Vec<String>,
    pub alternate: Box<ColorSpaceModel>,
    pub tint: Option<Function>,
}

/// 类型化颜色空间模型。
#[derive(Debug, Clone, PartialEq)]
pub enum ColorSpaceModel {
    DeviceGray,
    DeviceRGB,
    DeviceCMYK,
    CalGray(CalGraySpace),
    CalRGB(CalRGBSpace),
    Lab(LabSpace),
    IccBased(IccBasedSpace),
    Indexed(IndexedSpace),
    Separation(SeparationSpace),
    DeviceN(DeviceNSpace),
    /// Pattern（§8.6.7）：`/Pattern` 可选基空间。
    Pattern {
        base: Option<Box<ColorSpaceModel>>,
    },
}

/// 页面默认颜色空间种类（资源字典 `/DefaultGray` 等）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefaultColorSpace {
    Gray,
    Rgb,
    Cmyk,
}

impl DefaultColorSpace {
    /// 资源字典中的键名。
    pub fn resource_key(self) -> &'static str {
        match self {
            Self::Gray => "DefaultGray",
            Self::Rgb => "DefaultRGB",
            Self::Cmyk => "DefaultCMYK",
        }
    }
}
