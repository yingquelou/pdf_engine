//! 图像数据模型：`/Intent` 渲染意图、[`ImageData`] 原始像素条目、
//! [`ImageMask`] 的 `/Mask` 条目（显式蒙版与颜色键蒙版）。

use crate::core::builder::types::OpiProxy;

use super::color_space::ColorSpace;

/// 图像 `/Intent`（Table 89）：渲染意图，PDF 1.4 引入。
///
/// 规范只定义四个名称；省略时按 `RelativeColorimetric` 处理。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageIntent {
    /// 感知意图，适合照片类图像。
    Perceptual,
    /// 相对色度意图（缺省）。
    RelativeColorimetric,
    /// 饱和度意图。
    Saturation,
    /// 绝对色度意图。
    AbsoluteColorimetric,
}

impl ImageIntent {
    /// 字典中写出的名称。
    pub fn name(self) -> &'static str {
        match self {
            Self::Perceptual => "Perceptual",
            Self::RelativeColorimetric => "RelativeColorimetric",
            Self::Saturation => "Saturation",
            Self::AbsoluteColorimetric => "AbsoluteColorimetric",
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
    /// 色彩空间（设备空间、CIE-Based/ICCBased 或 Indexed）。
    pub color_space: ColorSpace,
    /// 每个颜色分量的位数（`1`、`2`、`4`、`8` 或 `16`）。
    pub bits: u8,
    /// 原始像素字节，按行对齐排列：每行
    /// `ceil(width * 通道数 * bits / 8)` 字节，总长度为该值乘以 `height`。
    /// 构建阶段校验长度，不匹配时返回 `WriteError::InvalidObject`。
    pub data: Vec<u8>,
    /// `/Decode` 数组（Table 89）：每个颜色分量一对 `[min max]`，
    /// 必须为 `2 × 通道数` 个有限数；反向区间表示反相。省略时按位深
    /// 使用规范默认值。
    pub decode: Option<Vec<f64>>,
    /// `/Interpolate`：是否对图像做插值采样，省略即 false。
    pub interpolate: bool,
    /// `/Intent`（PDF 1.4）：渲染意图；图像蒙版不得设置。
    pub intent: Option<ImageIntent>,
    /// `/Alternates`（PDF 1.3）：同一图像的低分辨率替代版本，按
    /// `<< /Image n 0 R … >>` 字典数组写出；不得嵌套、不得是图像蒙版。
    pub alternates: Vec<AlternateImage>,
    /// `/Matte`（PDF 1.6）：本图软蒙版的衬底颜色，按本图颜色空间给出；
    /// 必须与 [`ImageData::smask`] 同时设置，写出到 SMask 图像字典。
    pub matte: Option<Vec<f64>>,
    /// 显式软蒙版（`/SMask`，PDF 1.4）：独立的 DeviceGray 图像，
    /// 尺寸必须与本图一致，且自身不得再带软蒙版。
    pub smask: Option<Box<ImageData>>,
    /// `/ImageMask`（§8.9.6.2）：为 true 时本图是 1 位模板蒙版，
    /// 样本决定用当前非描边颜色绘制还是遮罩；`color_space` 仅作
    /// `DeviceGray` 占位，不会写出。
    pub image_mask: bool,
    /// `/Mask`（PDF 1.3）：显式蒙版图像或颜色键范围。
    pub mask: Option<ImageMask>,
    /// `/OPI`（§14.11.7 Table 368，PDF 1.2）：OPI 代理字典，指向
    /// 印刷工作流中的高分辨率原图。
    pub opi: Option<OpiProxy>,
    /// `/OC`（Table 89，PDF 1.5）：本图的可选内容组句柄。
    pub oc: Option<usize>,
    /// `/Metadata`（Table 89，PDF 1.4）：XMP 元数据包的原始 XML 字节，
    /// 写出为 `<< /Type /Metadata /Subtype /XML >>` 流。
    pub metadata: Option<Vec<u8>>,
}

/// 图像 `/Mask` 条目（Table 89，PDF 1.3）。
#[derive(Debug, Clone, PartialEq)]
pub enum ImageMask {
    /// 显式蒙版：独立图像蒙版 XObject（`/ImageMask true`，1 位样本）。
    /// 按 §8.9.6.3，分辨率可与基图不同。
    Explicit(Box<ImageData>),
    /// 颜色键蒙版：`2 × n` 个整数范围 `[min1 max1 … minn maxn]`，
    /// 落在范围内的样本不绘制（§8.9.6.4）。
    ColorKey(Vec<i64>),
}

impl ImageMask {
    /// 用一张图像蒙版构造显式蒙版。
    pub fn explicit(image: ImageData) -> Self {
        Self::Explicit(Box::new(image))
    }

    /// 用 `2 × 通道数` 个样本值范围构造颜色键蒙版。
    pub fn color_key(ranges: impl Into<Vec<i64>>) -> Self {
        Self::ColorKey(ranges.into())
    }
}

/// `/Alternates` 条目（Table 91）：图像及其打印缺省标记与 `/OC`。
#[derive(Debug, Clone, PartialEq)]
pub struct AlternateImage {
    /// `/Image`：交替图像本身。
    pub image: ImageData,
    /// `/DefaultForPrinting`：是否默认用于打印；同一基图至多一个为 true。
    pub default_for_printing: bool,
    /// `/OC`（PDF 1.5）：该交替图像的可选内容组句柄。
    pub oc: Option<usize>,
}

impl AlternateImage {
    /// 用一张图像构造交替图像条目，默认不用于打印、无 `/OC`。
    pub fn new(image: ImageData) -> Self {
        Self {
            image,
            default_for_printing: false,
            oc: None,
        }
    }

    /// 设置 `/DefaultForPrinting`。
    pub fn default_for_printing(mut self, flag: bool) -> Self {
        self.default_for_printing = flag;
        self
    }

    /// 设置 `/OC` 的可选内容组句柄。
    pub fn with_oc(mut self, handle: usize) -> Self {
        self.oc = Some(handle);
        self
    }
}

impl From<ImageData> for AlternateImage {
    fn from(image: ImageData) -> Self {
        Self::new(image)
    }
}
