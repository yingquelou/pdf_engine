//! 色彩空间联合与其参数字典：设备空间以名字引用，CIE-Based、ICCBased、
//! Indexed、Separation 与 DeviceN 以类型化字典建模并提供构造器。

use crate::core::builder::types::Function;

/// 图片色彩空间。
///
/// 设备空间（DeviceGray/DeviceRGB/DeviceCMYK）以名字引用；CIE-Based
/// 空间按 ISO 32000-1 §8.6.5 写成 `[名称 字典]` 数组；ICCBased 写成
/// `[/ICCBased 流引用]`，配置文件作为独立流对象输出；Indexed 写成
/// `[/Indexed base hival 调色板]`；Separation 与 DeviceN（§8.6.6）
/// 写成 `[名称 … 备用空间 tint变换]`。
/// 作为 [`ImageData::color_space`] 的取值，决定每个像素的通道数。
#[derive(Debug, Clone, PartialEq)]
pub enum ColorSpace {
    /// 设备灰度，每个像素 1 个样本。
    DeviceGray,
    /// 设备 RGB，每个像素 3 个样本。
    DeviceRGB,
    /// 设备 CMYK，每个像素 4 个样本。
    DeviceCMYK,
    /// 校准灰度（CalGray，Table 63）。
    CalGray(CalGray),
    /// 校准 RGB（CalRGB，Table 64）。
    CalRGB(CalRGB),
    /// CIE L*a*b*（Lab，Table 65）。
    Lab(Lab),
    /// ICC 配置文件（ICCBased，Table 66）。
    ICCBased(ICCBased),
    /// 索引色（Indexed，§8.6.6.3）：像素为调色板下标，每像素 1 个样本。
    Indexed(Indexed),
    /// 分色（Separation，§8.6.6.4）：单一 tint 分量，每像素 1 个样本。
    Separation(Separation),
    /// 多分量专色（DeviceN，§8.6.6.5）：每色料一个 tint 分量。
    DeviceN(DeviceN),
}

/// CalGray 颜色空间（Table 63）。
///
/// `white_point` 必填；`black_point` 与 `gamma` 省略时使用规范默认值
/// （`[0 0 0]` 与 `1.0`）。
#[derive(Debug, Clone, PartialEq)]
pub struct CalGray {
    /// CIE XYZ 白点，三个分量。
    pub white_point: [f64; 3],
    /// 黑点，省略时为 `[0 0 0]`。
    pub black_point: Option<[f64; 3]>,
    /// gamma 值，省略时为 `1.0`；必须为有限正数。
    pub gamma: Option<f64>,
}

impl CalGray {
    pub fn new(white_point: [f64; 3]) -> Self {
        Self {
            white_point,
            black_point: None,
            gamma: None,
        }
    }
}

/// CalRGB 颜色空间（Table 64）。
///
/// 省略的条目使用规范默认值：黑点 `[0 0 0]`、gamma `[1 1 1]`、
/// 矩阵为单位矩阵。
#[derive(Debug, Clone, PartialEq)]
pub struct CalRGB {
    /// CIE XYZ 白点，三个分量。
    pub white_point: [f64; 3],
    /// 黑点，省略时为 `[0 0 0]`。
    pub black_point: Option<[f64; 3]>,
    /// 三个分量的 gamma，省略时为 `[1 1 1]`；各分量必须为有限正数。
    pub gamma: Option<[f64; 3]>,
    /// 3×3 变换矩阵，按行展开 9 个数，省略时为单位矩阵。
    pub matrix: Option<[f64; 9]>,
}

impl CalRGB {
    pub fn new(white_point: [f64; 3]) -> Self {
        Self {
            white_point,
            black_point: None,
            gamma: None,
            matrix: None,
        }
    }
}

/// Lab 颜色空间（Table 65）。
///
/// 省略的条目使用规范默认值：黑点 `[0 0 0]`、范围
/// `[-100 100 -100 100]`。
#[derive(Debug, Clone, PartialEq)]
pub struct Lab {
    /// CIE XYZ 白点，三个分量。
    pub white_point: [f64; 3],
    /// 黑点，省略时为 `[0 0 0]`。
    pub black_point: Option<[f64; 3]>,
    /// `[a_min a_max b_min b_max]`，省略时为 `[-100 100 -100 100]`。
    pub range: Option<[f64; 4]>,
}

impl Lab {
    pub fn new(white_point: [f64; 3]) -> Self {
        Self {
            white_point,
            black_point: None,
            range: None,
        }
    }
}

/// ICCBased 颜色空间（Table 66）。
///
/// 配置文件字节作为独立流对象写出；`n` 必须为 1、3 或 4，并与图片
/// 像素样本的通道数一致。
#[derive(Debug, Clone, PartialEq)]
pub struct ICCBased {
    /// ICC 配置文件的颜色分量数：1（灰度）、3（RGB）或 4（CMYK）。
    pub n: u8,
    /// ICC 配置文件字节（不压缩写出）。
    pub data: Vec<u8>,
    /// 备用颜色空间，分量数必须与 `n` 相同。
    pub alternate: Option<Box<ColorSpace>>,
    /// 各分量的有效范围（2 × `n` 个数），省略时由规范隐含默认值。
    pub range: Option<Vec<f64>>,
}

impl ICCBased {
    pub fn new(n: u8, data: impl Into<Vec<u8>>) -> Self {
        Self {
            n,
            data: data.into(),
            alternate: None,
            range: None,
        }
    }
}

/// 索引色颜色空间（§8.6.6.3）。
///
/// 像素样本是调色板下标，范围为 `0..=hival`；调色板为按 `base`
/// 分量连续排列的字节序列，长度必须为 `(hival + 1) × base 通道数`。
#[derive(Debug, Clone, PartialEq)]
pub struct Indexed {
    /// 调色板条目的基础颜色空间，不能是另一个 Indexed。
    pub base: Box<ColorSpace>,
    /// 调色板最大下标，取值 `0..=255`。
    pub hival: u8,
    /// 调色板字节：每个条目按基础空间的通道顺序排列。
    pub lookup: Vec<u8>,
}

impl Indexed {
    pub fn new(base: ColorSpace, hival: u8, lookup: impl Into<Vec<u8>>) -> Self {
        Self {
            base: Box::new(base),
            hival,
            lookup: lookup.into(),
        }
    }
}

/// Separation 颜色空间（§8.6.6.4）。
///
/// 单一 tint 分量控制给定色料；`All` 与 `None` 是规范保留的特殊色料名。
/// `tint_transform` 必须接受 1 个输入，并输出备用颜色空间的通道数。
#[derive(Debug, Clone, PartialEq)]
pub struct Separation {
    /// 色料名称（非空；`All`/`None` 允许）。
    pub colorant: String,
    /// 备用颜色空间，须为设备或 CIE-Based 空间。
    pub alternate: Box<ColorSpace>,
    /// tint 变换函数。
    pub tint_transform: Function,
}

impl Separation {
    pub fn new(
        colorant: impl Into<String>,
        alternate: ColorSpace,
        tint_transform: Function,
    ) -> Self {
        Self {
            colorant: colorant.into(),
            alternate: Box::new(alternate),
            tint_transform,
        }
    }
}

/// DeviceN 颜色空间（§8.6.6.5）。
///
/// 分量数由 `colorants` 长度决定；除 `None` 外分量名不得重复，且不得
/// 使用保留名 `All`。`tint_transform` 必须接受 `colorants.len()` 个输入，
/// 并输出备用颜色空间的通道数。
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceN {
    /// 色料名称数组，决定分量数（至少 1 个）。
    pub colorants: Vec<String>,
    /// 备用颜色空间，须为设备或 CIE-Based 空间。
    pub alternate: Box<ColorSpace>,
    /// tint 变换函数。
    pub tint_transform: Function,
}

impl DeviceN {
    pub fn new(
        colorants: impl Into<Vec<String>>,
        alternate: ColorSpace,
        tint_transform: Function,
    ) -> Self {
        Self {
            colorants: colorants.into(),
            alternate: Box::new(alternate),
            tint_transform,
        }
    }
}
