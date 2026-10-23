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

/// 字体描述符度量（FontDescriptor metrics）。
///
/// 对应 PDF 标准 9.8 节的字体描述符字典，包含字体的度量与属性信息。
/// 标准 14 字体通常可省略描述符，但嵌入字体或需要精确度量时必须提供。
#[derive(Debug, Clone, PartialEq)]
pub struct FontDescriptor {
    /// `/FontFamily` 字体族名。
    pub font_family: Option<String>,
    /// `/FontStretch` 字体stretch 名称（如 Normal、Condensed）。
    pub font_stretch: Option<String>,
    /// `/FontWeight` 字体粗细（100-900）。
    pub font_weight: Option<f64>,
    /// `/Flags` 字体属性标志位。
    pub flags: Option<u32>,
    /// `/FontBBox` 字体边界框。
    pub font_bbox: Option<[f64; 4]>,
    /// `/ItalicAngle` 斜体角度（度，负值表示右倾）。
    pub italic_angle: Option<f64>,
    /// `/Ascent` 上升高度。
    pub ascent: Option<f64>,
    /// `/Descent` 下降深度（通常为负值）。
    pub descent: Option<f64>,
    /// `/Leading` 行间距。
    pub leading: Option<f64>,
    /// `/CapHeight` 大写字母高度。
    pub cap_height: Option<f64>,
    /// `/XHeight` 小写 x 高度。
    pub x_height: Option<f64>,
    /// `/StemV` 竖笔宽度。
    pub stem_v: Option<f64>,
    /// `/StemH` 横笔高度。
    pub stem_h: Option<f64>,
    /// `/AvgWidth` 平均宽度。
    pub avg_width: Option<f64>,
    /// `/MaxWidth` 最大宽度。
    pub max_width: Option<f64>,
    /// `/MissingWidth` 缺失字形宽度。
    pub missing_width: Option<f64>,
}

impl Default for FontDescriptor {
    fn default() -> Self {
        Self::new()
    }
}

impl FontDescriptor {
    /// 创建空的字体描述符。
    pub fn new() -> Self {
        Self {
            font_family: None,
            font_stretch: None,
            font_weight: None,
            flags: None,
            font_bbox: None,
            italic_angle: None,
            ascent: None,
            descent: None,
            leading: None,
            cap_height: None,
            x_height: None,
            stem_v: None,
            stem_h: None,
            avg_width: None,
            max_width: None,
            missing_width: None,
        }
    }

    /// 设置字体族名。
    pub fn with_family(mut self, family: impl Into<String>) -> Self {
        self.font_family = Some(family.into());
        self
    }

    /// 设置字体stretch。
    pub fn with_stretch(mut self, stretch: impl Into<String>) -> Self {
        self.font_stretch = Some(stretch.into());
        self
    }

    /// 设置字体粗细。
    pub fn with_weight(mut self, weight: f64) -> Self {
        self.font_weight = Some(weight);
        self
    }

    /// 设置属性标志位。
    pub fn with_flags(mut self, flags: u32) -> Self {
        self.flags = Some(flags);
        self
    }

    /// 设置字体边界框。
    pub fn with_bbox(mut self, bbox: [f64; 4]) -> Self {
        self.font_bbox = Some(bbox);
        self
    }

    /// 设置斜体角度。
    pub fn with_italic_angle(mut self, angle: f64) -> Self {
        self.italic_angle = Some(angle);
        self
    }

    /// 设置上升高度。
    pub fn with_ascent(mut self, ascent: f64) -> Self {
        self.ascent = Some(ascent);
        self
    }

    /// 设置下降深度。
    pub fn with_descent(mut self, descent: f64) -> Self {
        self.descent = Some(descent);
        self
    }

    /// 设置行间距。
    pub fn with_leading(mut self, leading: f64) -> Self {
        self.leading = Some(leading);
        self
    }

    /// 设置大写字母高度。
    pub fn with_cap_height(mut self, height: f64) -> Self {
        self.cap_height = Some(height);
        self
    }

    /// 设置小写 x 高度。
    pub fn with_x_height(mut self, height: f64) -> Self {
        self.x_height = Some(height);
        self
    }

    /// 设置竖笔宽度。
    pub fn with_stem_v(mut self, width: f64) -> Self {
        self.stem_v = Some(width);
        self
    }

    /// 设置横笔高度。
    pub fn with_stem_h(mut self, height: f64) -> Self {
        self.stem_h = Some(height);
        self
    }

    /// 设置平均宽度。
    pub fn with_avg_width(mut self, width: f64) -> Self {
        self.avg_width = Some(width);
        self
    }

    /// 设置最大宽度。
    pub fn with_max_width(mut self, width: f64) -> Self {
        self.max_width = Some(width);
        self
    }

    /// 设置缺失字形宽度。
    pub fn with_missing_width(mut self, width: f64) -> Self {
        self.missing_width = Some(width);
        self
    }

    /// 转换为 PDF 字典值。
    pub(super) fn to_value(&self) -> Value {
        let mut dict = Dict::new().insert("Type", Value::Name("FontDescriptor".into()));
        if let Some(family) = &self.font_family {
            dict = dict.insert("FontFamily", Value::String(family.clone()));
        }
        if let Some(stretch) = &self.font_stretch {
            dict = dict.insert("FontStretch", Value::Name(stretch.clone()));
        }
        if let Some(weight) = self.font_weight {
            dict = dict.insert("FontWeight", Value::Float(weight));
        }
        if let Some(flags) = self.flags {
            dict = dict.insert("Flags", Value::Integer(flags as i64));
        }
        if let Some(bbox) = &self.font_bbox {
            dict = dict.insert(
                "FontBBox",
                Value::Array(
                    Array::new()
                        .push(Value::Float(bbox[0]))
                        .push(Value::Float(bbox[1]))
                        .push(Value::Float(bbox[2]))
                        .push(Value::Float(bbox[3])),
                ),
            );
        }
        if let Some(angle) = self.italic_angle {
            dict = dict.insert("ItalicAngle", Value::Float(angle));
        }
        if let Some(ascent) = self.ascent {
            dict = dict.insert("Ascent", Value::Float(ascent));
        }
        if let Some(descent) = self.descent {
            dict = dict.insert("Descent", Value::Float(descent));
        }
        if let Some(leading) = self.leading {
            dict = dict.insert("Leading", Value::Float(leading));
        }
        if let Some(cap_height) = self.cap_height {
            dict = dict.insert("CapHeight", Value::Float(cap_height));
        }
        if let Some(x_height) = self.x_height {
            dict = dict.insert("XHeight", Value::Float(x_height));
        }
        if let Some(stem_v) = self.stem_v {
            dict = dict.insert("StemV", Value::Float(stem_v));
        }
        if let Some(stem_h) = self.stem_h {
            dict = dict.insert("StemH", Value::Float(stem_h));
        }
        if let Some(avg_width) = self.avg_width {
            dict = dict.insert("AvgWidth", Value::Float(avg_width));
        }
        if let Some(max_width) = self.max_width {
            dict = dict.insert("MaxWidth", Value::Float(max_width));
        }
        if let Some(missing_width) = self.missing_width {
            dict = dict.insert("MissingWidth", Value::Float(missing_width));
        }
        Value::Dict(dict)
    }
}

/// 逻辑字体描述。
///
/// 本层仅支持 PDF 标准内置 Type1 字体（如 Helvetica），无需子集嵌入。
/// 字段 [`Font::name`] 指定 BaseFont 名，[`Font::size`] 指定字号 pt，
/// [`Font::encoding`] 控制字体字典的 `/Encoding`（默认 WinAnsiEncoding），
/// [`Font::descriptor`] 可选地提供字体描述符度量。
/// 可通过 [`Font::helvetica`] 便捷构造常用 Helvetica、用
/// [`Font::with_encoding`] 替换编码配置、[`Font::with_descriptor`] 添加描述符。
#[derive(Debug, Clone, PartialEq)]
pub struct Font {
    /// BaseFont 名（不含前导 `/`），如 `Helvetica`。
    pub name: String,
    /// 字号（pt）。
    pub size: f64,
    /// `/Encoding` 写端配置。
    pub encoding: FontEncoding,
    /// 可选的字体描述符。
    pub descriptor: Option<FontDescriptor>,
}

impl Font {
    /// Helvetica 标准字体（默认 WinAnsiEncoding）。
    pub fn helvetica(size: f64) -> Self {
        Self {
            name: "Helvetica".to_string(),
            size,
            encoding: FontEncoding::default(),
            descriptor: None,
        }
    }

    /// 替换编码配置（链式构造）。
    pub fn with_encoding(mut self, encoding: FontEncoding) -> Self {
        self.encoding = encoding;
        self
    }

    /// 添加字体描述符（链式构造）。
    pub fn with_descriptor(mut self, descriptor: FontDescriptor) -> Self {
        self.descriptor = Some(descriptor);
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
