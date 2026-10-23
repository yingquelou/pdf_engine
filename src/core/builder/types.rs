//! 逻辑层公共数据类型：页面尺寸、字体与编码配置、图片数据。

use crate::core::reader::font::{
    PredefinedEncoding, code_for_char, default_encoding, predefined_encoding,
};
use crate::core::types::{Array, Dict, Value};
use std::collections::BTreeMap;

/// 页面尺寸预设。
///
/// 提供 A4、Letter 两种常用规格（单位 pt），以及可选的自定义宽高。
/// 用于 [`crate::core::builder::PdfBuilder::page`] 声明新页的 MediaBox。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PageSize {
    /// A4 纵向：595 × 842 pt。
    A4,
    /// Letter 纵向：612 × 792 pt。
    Letter,
    /// 自定义宽高（pt）。
    Custom(f64, f64),
}

impl PageSize {
    /// 返回 `(width, height)`，单位 pt。
    pub(super) fn dimensions(&self) -> (f64, f64) {
        match self {
            PageSize::A4 => (595.0, 842.0),
            PageSize::Letter => (612.0, 792.0),
            PageSize::Custom(w, h) => (*w, *h),
        }
    }
}

/// 简单字体 `/Encoding` 的写端配置。
///
/// 默认取 [`FontEncoding::Named`] 的 `WinAnsiEncoding`，与既有输出一致。
/// [`FontEncoding::BuiltIn`] 省略 `/Encoding`，让字体程序的内建编码生效；
/// [`FontEncoding::Custom`] 写入 `/BaseEncoding` 与 `/Differences` 覆盖。
///
/// `text` 会按此配置把文本转成单字节字符码：五个预定义编码名可识别，
/// 其余名称无法转码时退回原始 UTF-8 字节。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FontEncoding {
    /// 省略 `/Encoding`（使用字体内建编码，如 Standard 14 的 Standard）。
    BuiltIn,
    /// 预定义编码名（不含前导 `/`），如 `MacRomanEncoding`。
    Named(String),
    /// 编码字典。
    Custom {
        /// `/BaseEncoding` 名（不含前导 `/`）；`None` 表示省略。
        base: Option<String>,
        /// 字符码 → glyph 名覆盖项，写出时按字符码升序排列。
        differences: BTreeMap<u8, String>,
    },
}

impl Default for FontEncoding {
    fn default() -> Self {
        Self::Named("WinAnsiEncoding".to_string())
    }
}

impl FontEncoding {
    /// 指定预定义编码名。
    pub fn named(name: impl Into<String>) -> Self {
        Self::Named(name.into())
    }

    /// 带 `/Differences` 的编码字典；`base` 为 `None` 时省略 `/BaseEncoding`。
    ///
    /// 同一字符码重复出现时，后定义者覆盖先定义者。
    pub fn with_differences(base: Option<&str>, differences: Vec<(u8, String)>) -> Self {
        Self::Custom {
            base: base.map(str::to_string),
            differences: differences.into_iter().collect(),
        }
    }

    /// 解析基础预定义编码；编码名无法识别或 `/BaseEncoding` 无效时返回 `None`。
    pub(super) fn base_encoding(&self, base_font: &str) -> Option<PredefinedEncoding> {
        match self {
            Self::BuiltIn => Some(default_encoding(base_font)),
            Self::Named(name) => predefined_encoding(name).ok(),
            Self::Custom {
                base: Some(name), ..
            } => predefined_encoding(name).ok(),
            Self::Custom { base: None, .. } => Some(default_encoding(base_font)),
        }
    }

    /// 把文本按编码转成单字节字符码。
    ///
    /// 无法表示的字符替换为 `?`（0x3F）；基础编码无法识别时返回 `None`，
    /// 调用方回退为原始 UTF-8 字节。
    pub(super) fn encode_text(&self, base_font: &str, text: &str) -> Option<Vec<u8>> {
        let base = self.base_encoding(base_font)?;
        let empty = BTreeMap::new();
        let differences = match self {
            Self::Custom { differences, .. } => differences,
            _ => &empty,
        };
        Some(
            text.chars()
                .map(|ch| code_for_char(base, differences, ch).unwrap_or(b'?'))
                .collect(),
        )
    }

    /// 转换为字体字典的 `/Encoding` 值；`BuiltIn` 无对应值。
    pub(super) fn to_value(&self) -> Option<Value> {
        match self {
            Self::BuiltIn => None,
            Self::Named(name) => Some(Value::Name(name.clone())),
            Self::Custom { base, differences } => {
                let mut dict = Dict::new();
                if let Some(base) = base {
                    dict = dict.insert("BaseEncoding", Value::Name(base.clone()));
                }
                if !differences.is_empty() {
                    let mut arr = Array::new();
                    for (code, name) in differences {
                        arr = arr
                            .push(Value::Integer(i64::from(*code)))
                            .push(Value::Name(name.clone()));
                    }
                    dict = dict.insert("Differences", Value::Array(arr));
                }
                Some(Value::Dict(dict))
            }
        }
    }
}

/// 逻辑字体描述。
///
/// 本层仅支持 PDF 标准内置 Type1 字体（如 Helvetica），无需子集嵌入。
/// 字段 [`Font::name`] 指定 BaseFont 名，[`Font::size`] 指定字号 pt，
/// [`Font::encoding`] 控制字体字典的 `/Encoding`（默认 WinAnsiEncoding），
/// 可通过 [`Font::helvetica`] 便捷构造常用 Helvetica、用
/// [`Font::with_encoding`] 替换编码配置。
#[derive(Debug, Clone, PartialEq)]
pub struct Font {
    /// BaseFont 名（不含前导 `/`），如 `Helvetica`。
    pub name: String,
    /// 字号（pt）。
    pub size: f64,
    /// `/Encoding` 写端配置。
    pub encoding: FontEncoding,
}

impl Font {
    /// Helvetica 标准字体（默认 WinAnsiEncoding）。
    pub fn helvetica(size: f64) -> Self {
        Self {
            name: "Helvetica".to_string(),
            size,
            encoding: FontEncoding::default(),
        }
    }

    /// 替换编码配置（链式构造）。
    pub fn with_encoding(mut self, encoding: FontEncoding) -> Self {
        self.encoding = encoding;
        self
    }
}

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
    pub(super) fn as_name(&self) -> &'static str {
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
