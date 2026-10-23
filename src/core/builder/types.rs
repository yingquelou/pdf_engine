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
/// CIDFont 描述符额外支持 `/FontName`、`/CharSet` 与 `/CIDSet`。
#[derive(Debug, Clone, PartialEq)]
pub struct FontDescriptor {
    /// `/FontName` 字体名称。
    pub font_name: Option<String>,
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
    /// `/CharSet` 字符集名称字符串（简单字体）。
    pub char_set: Option<String>,
    /// `/CIDSet` 位数组（MSB first），按 CID 索引标记哪些 CID 存在。
    /// 仅用于 CIDFont 描述符。
    pub cid_set: Option<Vec<u8>>,
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
            font_name: None,
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
            char_set: None,
            cid_set: None,
        }
    }

    /// 设置字体名称。
    pub fn with_font_name(mut self, name: impl Into<String>) -> Self {
        self.font_name = Some(name.into());
        self
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

    /// 设置字符集名称（简单字体 `/CharSet`）。
    pub fn with_char_set(mut self, char_set: impl Into<String>) -> Self {
        self.char_set = Some(char_set.into());
        self
    }

    /// 设置 CIDSet 位数组（CIDFont `/CIDSet`）。
    ///
    /// 位数组按 MSB-first 编码，第 `cid` 位表示 CID `cid` 是否存在。
    pub fn with_cid_set(mut self, cid_set: Vec<u8>) -> Self {
        self.cid_set = Some(cid_set);
        self
    }

    /// 转换为 PDF 字典值。
    pub(super) fn to_value(&self) -> Value {
        let mut dict = Dict::new().insert("Type", Value::Name("FontDescriptor".into()));
        if let Some(name) = &self.font_name {
            dict = dict.insert("FontName", Value::Name(name.clone()));
        }
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
        if let Some(char_set) = &self.char_set {
            dict = dict.insert("CharSet", Value::String(char_set.clone()));
        }
        // /CIDSet 作为流对象写出（此处仅返回字典项，流由 assemble 单独处理）
        // 注意：CIDSet 是流，不能直接嵌入字典值，由 assemble.rs 单独构造
        Value::Dict(dict)
    }

    /// 返回 `/CIDSet` 数据（如果存在）。
    pub(super) fn cid_set_data(&self) -> Option<&[u8]> {
        self.cid_set.as_deref()
    }
}

/// 嵌入的字体程序数据。
///
/// 对应 PDF 标准 5.8 节的字体程序嵌入，支持三种格式：
/// - Type 1（`/FontFile`）：PostScript Type 1 字体程序
/// - TrueType（`/FontFile2`）：TrueType 轮廓的 SFNT 字体程序
/// - CFF（`/FontFile3`）：Compact Font Format 字体程序
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FontProgram {
    /// Type 1 字体程序（PostScript Type 1，存储为 `/FontFile` 流）。
    Type1(Vec<u8>),
    /// TrueType 字体程序（SFNT 格式，存储为 `/FontFile2` 流）。
    TrueType(Vec<u8>),
    /// CFF 字体程序（Compact Font Format，存储为 `/FontFile3` 流）。
    CFF(Vec<u8>),
}

impl FontProgram {
    /// 返回字体程序的原始字节数据。
    pub(super) fn data(&self) -> &[u8] {
        match self {
            FontProgram::Type1(data) => data,
            FontProgram::TrueType(data) => data,
            FontProgram::CFF(data) => data,
        }
    }
}

/// 逻辑字体描述。
///
/// 本层支持 PDF 标准内置 Type1 字体（如 Helvetica）与嵌入字体程序。
/// 字段 [`Font::name`] 指定 BaseFont 名，[`Font::size`] 指定字号 pt，
/// [`Font::encoding`] 控制字体字典的 `/Encoding`（默认 WinAnsiEncoding），
/// [`Font::descriptor`] 可选地提供字体描述符度量，
/// [`Font::font_program`] 可选地嵌入字体程序数据。
/// 可通过 [`Font::helvetica`] 便捷构造常用 Helvetica、用
/// [`Font::with_encoding`] 替换编码配置、[`Font::with_descriptor`] 添加描述符、
/// [`Font::with_font_program`] 嵌入字体程序。
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
    /// 可选的嵌入字体程序。
    pub font_program: Option<FontProgram>,
}

impl Font {
    /// Helvetica 标准字体（默认 WinAnsiEncoding）。
    pub fn helvetica(size: f64) -> Self {
        Self {
            name: "Helvetica".to_string(),
            size,
            encoding: FontEncoding::default(),
            descriptor: None,
            font_program: None,
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

    /// 嵌入字体程序（链式构造）。
    pub fn with_font_program(mut self, program: FontProgram) -> Self {
        self.font_program = Some(program);
        self
    }
}

/// CID 字符集合信息（`/CIDSystemInfo`）。
///
/// 标识 CIDFont 的字符集合：注册机构、排序与补充版本号。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CidSystemInfo {
    /// `/Registry`（如 `"Adobe"`）。
    pub registry: String,
    /// `/Ordering`（如 `"Identity"`、`"GB1"`）。
    pub ordering: String,
    /// `/Supplement` 补充版本号。
    pub supplement: i64,
}

impl CidSystemInfo {
    /// 创建 `/CIDSystemInfo`。
    pub fn new(registry: impl Into<String>, ordering: impl Into<String>, supplement: i64) -> Self {
        Self {
            registry: registry.into(),
            ordering: ordering.into(),
            supplement,
        }
    }

    /// 转换为 PDF 字典值。
    pub(super) fn to_value(&self) -> Value {
        Value::Dict(
            Dict::new()
                .insert("Registry", Value::String(self.registry.clone()))
                .insert("Ordering", Value::String(self.ordering.clone()))
                .insert("Supplement", Value::Integer(self.supplement)),
        )
    }
}

/// CIDFont 子类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CidFontSubtype {
    /// CIDFontType0（CFF 轮廓）。
    CIDFontType0,
    /// CIDFontType2（TrueType 轮廓）。
    CIDFontType2,
}

impl CidFontSubtype {
    /// 返回 PDF 子类型名。
    pub(super) fn as_name(&self) -> &'static str {
        match self {
            CidFontSubtype::CIDFontType0 => "CIDFontType0",
            CidFontSubtype::CIDFontType2 => "CIDFontType2",
        }
    }
}

/// CIDFont 宽度条目：单个 CID 或 CID 区间共用一个宽度。
#[derive(Debug, Clone, PartialEq)]
pub enum CidWidth {
    /// 单个 CID 的宽度。
    Single(u16, f64),
    /// CID 区间 `[first, last]` 共用同一宽度。
    Range(u16, u16, f64),
}

/// CIDFont 逻辑字体描述。
///
/// 对应 PDF 标准 Type 0 复合字体中的 CIDFont 后代字体字典。
/// 包含 `/CIDSystemInfo`、`/FontDescriptor`（含 `/CIDSet`）、`/DW`/`/W` 宽度信息。
/// 通过 [`PdfBuilder::cid_font`] 注册到页面资源。
#[derive(Debug, Clone, PartialEq)]
pub struct CidFont {
    /// `/BaseFont` 名称。
    pub name: String,
    /// `/Subtype`（CIDFontType0 或 CIDFontType2）。
    pub subtype: CidFontSubtype,
    /// `/CIDSystemInfo`。
    pub system_info: CidSystemInfo,
    /// 字体描述符（含 `/CIDSet`）。
    pub descriptor: FontDescriptor,
    /// `/DW` 默认宽度（默认 1000）。
    pub default_width: f64,
    /// `/W` 宽度条目。
    pub widths: Vec<CidWidth>,
    /// 字号（pt），用于内容流 `Tf` 操作符。
    pub size: f64,
}

impl CidFont {
    /// 创建 CIDFont。
    pub fn new(
        name: impl Into<String>,
        subtype: CidFontSubtype,
        system_info: CidSystemInfo,
        descriptor: FontDescriptor,
        size: f64,
    ) -> Self {
        Self {
            name: name.into(),
            subtype,
            system_info,
            descriptor,
            default_width: 1000.0,
            widths: Vec::new(),
            size,
        }
    }

    /// 设置默认宽度 `/DW`。
    pub fn with_default_width(mut self, width: f64) -> Self {
        self.default_width = width;
        self
    }

    /// 添加宽度条目。
    pub fn with_widths(mut self, widths: Vec<CidWidth>) -> Self {
        self.widths = widths;
        self
    }

    /// 构建 `/W` 数组值。
    pub(super) fn widths_to_value(&self) -> Value {
        let mut arr = Array::new();
        for entry in &self.widths {
            match entry {
                CidWidth::Single(cid, width) => {
                    arr = arr
                        .push(Value::Integer(i64::from(*cid)))
                        .push(Value::Array(Array::new().push(Value::Float(*width))));
                }
                CidWidth::Range(first, last, width) => {
                    arr = arr
                        .push(Value::Integer(i64::from(*first)))
                        .push(Value::Integer(i64::from(*last)))
                        .push(Value::Float(*width));
                }
            }
        }
        Value::Array(arr)
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
