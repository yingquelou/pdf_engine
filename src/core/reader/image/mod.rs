//! 图像 XObject 与内联图像的类型化读端模型（ISO 32000-1 §8.9.5–8.9.7）。
//!
//! [`Document::image`] 把图像 XObject（流或间接引用）解析为 [`ImageModel`]：
//! `/Width` `/Height` `/ColorSpace` `/BitsPerComponent` `/Decode`
//! `/Interpolate` `/Intent` `/ImageMask` `/Mask` `/SMask` `/SMaskInData`
//! `/Alternates` 全部建模，其余条目保留在 `other`。`/Mask` 区分显式蒙版
//! （图像蒙版 XObject）与颜色键范围；`/Alternates` 递归解析为图像列表。
//! [`Document::inline_image`] 接受内容流里的 [`InlineImage`]，按 Table 94
//! 把缩写键（`/W` `/H` `/BPC` `/CS` `/D` `/F` `/DP` `/IM` `/I`）还原为
//! 标准键，`/CS` 的颜色空间名按 Table 95 还原（`/G` `/RGB` `/CMYK` `/I`），
//! 并先经过滤器解码数据；[`Document::page_inline_image`] 额外用页面
//! `/Resources` 解析非内建的命名颜色空间。
//!
//! [`ImageModel::samples`] 是样本读取 API（不做合成/渲染）：按行对齐解码
//! 位流、校验数据长度（截断/多余字节报错），对设备/CIE/ICCBased 空间应用
//! `/Decode`（缺省时 Lab 用其 `/Range`，其余用 [0 1]），Indexed 展开为
//! 基空间分量，Separation/DeviceN 经 tint 函数求值。对 Indexed /
//! Separation / DeviceN 的显式 `/Decode`（需在查色板/tint 之前映射）
//! 暂不支持，返回类型化错误而不是猜测。
//!
//! **JPXDecode（§7.4.9）只建模到语法层**：`/Filter` 含 JPX 时过滤链的
//! JPX 步骤原样透传（其余步骤照常解码），原始 JPX 字节交给
//! [`parse_jpx_header`] 解析头部语法，结果挂在 [`ImageModel::jpx`]；
//! 据此按 Table 89 忽略声明的 `/BitsPerComponent`、在 `/ColorSpace`
//! 缺失时给出设备空间提示。按设计**不解码 JPX 像素**（无小波/熵解码，
//! ICC 剖面只记长度），[`ImageModel::samples`] 对 JPX 图像返回类型化
//! `Unsupported`。边界详见 [`jpx`] 模块文档。

use std::collections::BTreeMap;
use std::fmt;

use crate::core::types::{Array, Dict, Document, Value};

use super::ParseError;
use super::color_space::{ColorSpaceError, ColorSpaceModel};
use super::content::InlineImage;
use super::opi::{OpiProxy, parse_opi_entry};
use super::page_content::PageContentError;

#[cfg(test)]
mod tests;

mod jpx;

pub use jpx::{JpxColorSpace, JpxComponent, JpxContainer, JpxHeader, parse_jpx_header};

type Result<T> = std::result::Result<T, ImageError>;

/// 图像嵌套（`/Mask`、`/SMask`、`/Alternates`）最大深度，防止引用环。
const MAX_IMAGE_DEPTH: usize = 8;

/// 已建模的图像字典键；其余键进入 [`ImageModel::other`]。
const IMAGE_ENTRIES: [&str; 22] = [
    "Type",
    "Subtype",
    "Width",
    "Height",
    "ColorSpace",
    "BitsPerComponent",
    "Decode",
    "Interpolate",
    "Intent",
    "ImageMask",
    "Mask",
    "SMask",
    "SMaskInData",
    "Alternates",
    "Filter",
    "DecodeParms",
    "Length",
    "DL",
    "Metadata",
    "OC",
    "Matte",
    "OPI",
];

/// 图像解析或样本读取的失败原因。
#[derive(Debug)]
pub enum ImageError {
    /// 间接引用无法解析。
    Parse(ParseError),
    /// 页面定位或资源解析失败。
    PageContent(PageContentError),
    /// `/ColorSpace` 解析失败。
    ColorSpace(ColorSpaceError),
    /// 图像字典或样本数据不满足规范。
    Invalid(String),
    /// 暂不支持的图像特性（如 Indexed 的 `/Decode`、DCT 原始样本）。
    Unsupported(String),
    /// 流过滤链解码失败。
    Filter(String),
}

impl fmt::Display for ImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(error) => write!(f, "{error}"),
            Self::PageContent(error) => write!(f, "{error}"),
            Self::ColorSpace(error) => write!(f, "invalid image color space: {error}"),
            Self::Invalid(message) => write!(f, "invalid image: {message}"),
            Self::Unsupported(message) => write!(f, "unsupported image feature: {message}"),
            Self::Filter(message) => write!(f, "image stream decode failed: {message}"),
        }
    }
}

impl std::error::Error for ImageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Parse(error) => Some(error),
            Self::PageContent(error) => Some(error),
            Self::ColorSpace(error) => Some(error),
            Self::Invalid(_) | Self::Unsupported(_) | Self::Filter(_) => None,
        }
    }
}

impl From<ParseError> for ImageError {
    fn from(error: ParseError) -> Self {
        Self::Parse(error)
    }
}

impl From<PageContentError> for ImageError {
    fn from(error: PageContentError) -> Self {
        Self::PageContent(error)
    }
}

impl From<ColorSpaceError> for ImageError {
    fn from(error: ColorSpaceError) -> Self {
        Self::ColorSpace(error)
    }
}

fn invalid(message: impl Into<String>) -> ImageError {
    ImageError::Invalid(message.into())
}

fn unsupported(message: impl Into<String>) -> ImageError {
    ImageError::Unsupported(message.into())
}

/// 图像 `/Mask` 条目（Table 89，PDF 1.3）。
#[derive(Debug, Clone, PartialEq)]
pub enum ImageMaskModel {
    /// 显式蒙版：独立的图像蒙版 XObject（`/ImageMask true`，1 位样本）。
    Explicit(Box<ImageModel>),
    /// 颜色键蒙版：`2 × n` 个整数范围，落在范围内的样本不绘制。
    ColorKey(Vec<i64>),
}

/// 解析后的图像 XObject / 内联图像。
#[derive(Debug, Clone, PartialEq)]
pub struct ImageModel {
    pub width: u32,
    pub height: u32,
    /// 颜色空间；图像蒙版（`/ImageMask true`）没有颜色空间。
    pub color_space: Option<ColorSpaceModel>,
    /// 每分量位数（1/2/4/8/16）；图像蒙版固定为 1。
    pub bits_per_component: u8,
    /// 显式 `/Decode` 数组；省略时为 `None`（按颜色空间使用规范默认值）。
    pub decode: Option<Vec<f64>>,
    /// `/Interpolate`，省略即 `false`。
    pub interpolate: bool,
    /// `/Intent` 渲染意图名（`AbsoluteColorimetric` 等）。
    pub intent: Option<String>,
    /// `/Matte`：软蒙版图像的衬底颜色（PDF 1.6），分量数按**基图**
    /// 颜色空间解释；仅在作为 `/SMask` 时校验长度。
    pub matte: Option<Vec<f64>>,
    /// `/ImageMask`。
    pub image_mask: bool,
    /// `/Mask`：显式蒙版或颜色键范围。
    pub mask: Option<ImageMaskModel>,
    /// `/SMask`：DeviceGray 软蒙版图像。
    pub smask: Option<Box<ImageModel>>,
    /// `/SMaskInData`：软蒙版是否来自图像数据（0–2）。
    pub smask_in_data: Option<u8>,
    /// `/Alternates`：交替图像列表。
    pub alternates: Vec<ImageModel>,
    /// `/OPI`：OPI 代理版本字典（§14.11.7 Table 368–370，PDF 1.2）。
    pub opi: Option<OpiProxy>,
    /// JPXDecode 图像的头部语法解析结果（§7.4.9，仅语法层，不解码
    /// 像素）；`/Filter` 不含 JPX 或载荷无法解析时为 `None`。
    pub jpx: Option<JpxHeader>,
    /// 已应用的过滤器规范名（`FlateDecode` 等），供样本读取判定。
    pub filters: Vec<String>,
    /// 未建模的其余字典条目。
    pub other: BTreeMap<String, Value>,
    /// 过滤链解码后的流字节（内联图像为 `ID…EI` 之间的数据）。
    pub data: Vec<u8>,
}

type DecodedSamples = (Vec<f64>, Vec<(f64, f64)>);

impl ImageModel {
    /// 每像素的样本分量数（图像蒙版为 1；Pattern 空间无样本）。
    pub fn component_count(&self) -> Option<usize> {
        if self.image_mask {
            return Some(1);
        }
        self.color_space
            .as_ref()
            .and_then(|space| space.component_count())
    }

    /// 把图像数据解码为行优先、展平的样本值，并应用 `/Decode`。
    ///
    /// 返回的 `ranges` 是输出分量的取值范围；`values` 长度恒为
    /// `width × height × components`。流过滤器输出不是原始样本（DCT/JPEG）
    /// 或需要先于查色板/tint 应用 `/Decode` 的空间返回类型化错误。
    pub fn samples(&self) -> Result<ImageSamples> {
        if jpx::is_jpx_filter(&self.filters) {
            return Err(unsupported(
                "JPXDecode image data is a JPEG 2000 codestream; this crate parses the JPX \
                 header syntax only and does not decode pixels",
            ));
        }
        let bits = self.bits_per_component as usize;
        let width = self.width as usize;
        let height = self.height as usize;

        if self.image_mask {
            let expected = width
                .div_ceil(8)
                .checked_mul(height)
                .ok_or_else(|| invalid("image mask dimensions overflow sample length"))?;
            if self.data.len() != expected {
                return Err(invalid(format!(
                    "image mask data length {} does not match expected {expected} bytes for \
                     {width}x{height} at 1 bit per component",
                    self.data.len()
                )));
            }
            let values = ColorSpaceModel::DeviceGray
                .decode_samples(bits, width, &self.data)
                .map_err(ImageError::ColorSpace)?;
            let decode = match &self.decode {
                Some(decode) => {
                    if decode.len() != 2
                        || !((decode[0] == 0.0 && decode[1] == 1.0)
                            || (decode[0] == 1.0 && decode[1] == 0.0))
                    {
                        return Err(invalid("image mask /Decode must be [0 1] or [1 0]"));
                    }
                    decode.clone()
                }
                None => vec![0.0, 1.0],
            };
            let values = values
                .into_iter()
                .map(|value| decode[0] + value * (decode[1] - decode[0]))
                .collect();
            return Ok(ImageSamples {
                width,
                height,
                components: 1,
                ranges: vec![(decode[0], decode[1])],
                values,
            });
        }

        let space = self
            .color_space
            .as_ref()
            .ok_or_else(|| invalid("image has no color space"))?;
        let components = space
            .component_count()
            .ok_or_else(|| unsupported("Pattern color space carries no image samples"))?;

        let row_bytes = (width)
            .checked_mul(components)
            .and_then(|bits_total| bits_total.checked_mul(bits))
            .map(|bits_total| bits_total.div_ceil(8))
            .ok_or_else(|| invalid("image dimensions overflow sample length"))?;
        let expected = row_bytes
            .checked_mul(height)
            .ok_or_else(|| invalid("image dimensions overflow sample length"))?;
        if self.data.len() != expected {
            return Err(invalid(format!(
                "image data length {} does not match expected {expected} bytes for \
                 {width}x{height} at {bits} bits per component (truncated or corrupt data)",
                self.data.len()
            )));
        }

        let values = space
            .decode_samples(bits, width, &self.data)
            .map_err(ImageError::ColorSpace)?;
        let output_components = space
            .native_component_count()
            .ok_or_else(|| unsupported("color space carries no native components"))?;
        let (values, ranges) = self.apply_decode(space, values)?;
        Ok(ImageSamples {
            width,
            height,
            components: output_components,
            ranges,
            values,
        })
    }

    /// 应用 `/Decode`（或规范默认值），返回映射后的值与输出范围。
    fn apply_decode(&self, space: &ColorSpaceModel, values: Vec<f64>) -> Result<DecodedSamples> {
        match space {
            ColorSpaceModel::Indexed(_) => {
                if self.decode.is_some() {
                    return Err(unsupported(
                        "/Decode on Indexed images must be applied to palette indices",
                    ));
                }
                let components = space.native_component_count().unwrap_or(1);
                let ranges = space.native_ranges();
                let ranges = if ranges.len() == components {
                    ranges
                } else {
                    vec![(0.0, 1.0); components]
                };
                Ok((values, ranges))
            }
            ColorSpaceModel::Separation(_) | ColorSpaceModel::DeviceN(_) => {
                if self.decode.is_some() {
                    return Err(unsupported(
                        "/Decode on Separation/DeviceN images must be applied to tint inputs",
                    ));
                }
                let components = space.native_component_count().unwrap_or(1);
                let ranges = space.native_ranges();
                let ranges = if ranges.len() == components {
                    ranges
                } else {
                    vec![(0.0, 1.0); components]
                };
                Ok((values, ranges))
            }
            _ => {
                let components = space
                    .native_component_count()
                    .ok_or_else(|| unsupported("color space carries no sample components"))?;
                let decode = match &self.decode {
                    Some(decode) => {
                        if decode.len() != components * 2 {
                            return Err(invalid(format!(
                                "/Decode must contain {} numbers for a {components}-component \
                                 image, got {}",
                                components * 2,
                                decode.len()
                            )));
                        }
                        if decode.iter().any(|value| !value.is_finite()) {
                            return Err(invalid("/Decode must contain finite numbers"));
                        }
                        decode.clone()
                    }
                    None => default_decode(space, components),
                };
                let ranges = decode
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|pair| (pair[0], pair[1]))
                    .collect::<Vec<_>>();
                let values = values
                    .into_iter()
                    .enumerate()
                    .map(|(index, value)| {
                        let pair = &decode[(index % components) * 2..(index % components) * 2 + 2];
                        pair[0] + value * (pair[1] - pair[0])
                    })
                    .collect();
                Ok((values, ranges))
            }
        }
    }
}

/// 规范默认 `/Decode`：Lab 用 `/Range`（L 固定 0–100），其余 [0 1]。
fn default_decode(space: &ColorSpaceModel, components: usize) -> Vec<f64> {
    if let ColorSpaceModel::Lab(lab) = space {
        let mut decode = vec![0.0, 100.0];
        for &(low, high) in &lab.range {
            decode.push(low);
            decode.push(high);
        }
        if decode.len() == components * 2 {
            return decode;
        }
    }
    [0.0, 1.0].repeat(components)
}

/// 解码后的样本：行优先展平的规格化/映射后分量。
#[derive(Debug, Clone, PartialEq)]
pub struct ImageSamples {
    pub width: usize,
    pub height: usize,
    /// 每像素分量数（Indexed 展开后为基空间分量数）。
    pub components: usize,
    /// 输出分量的取值范围（已应用 `/Decode`）。
    pub ranges: Vec<(f64, f64)>,
    pub values: Vec<f64>,
}

impl ImageSamples {
    /// 取 `(x, y)` 像素的分量切片；越界返回 `None`。
    pub fn pixel(&self, x: usize, y: usize) -> Option<&[f64]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let start = (y * self.width + x) * self.components;
        self.values.get(start..start + self.components)
    }
}

/// 字典来源，决定省略键的默认值。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ImageSource {
    /// 图像 XObject：`/ColorSpace` 与 `/BitsPerComponent` 必填（蒙版除外）。
    XObject,
    /// 内联图像（Table 94）：缺省 `/CS` 为 DeviceGray、缺省 `/BPC` 为 8。
    Inline,
}

impl Document {
    /// 解析一个图像 XObject（流或指向流的间接引用）。
    pub fn image(&self, value: &Value) -> Result<ImageModel> {
        self.image_depth(value, 0)
    }

    /// 解析页面资源字典中命名的图像 XObject；资源不存在时返回 `None`。
    pub fn page_image(&self, page_index: usize, name: &str) -> Result<Option<ImageModel>> {
        match self.page_resource(page_index, "XObject", name)? {
            Some(value) => self.image(&value).map(Some),
            None => Ok(None),
        }
    }

    /// 解析内容流中的内联图像（`BI … ID … EI`）。
    ///
    /// `/CS` 只解析内建颜色空间；命名资源需用
    /// [`Document::page_inline_image`]。
    pub fn inline_image(&self, image: &InlineImage) -> Result<ImageModel> {
        self.parse_inline_image(image, None)
    }

    /// 解析页面内容流中的内联图像，命名 `/CS` 经页面 `/Resources` 解析。
    pub fn page_inline_image(&self, page_index: usize, image: &InlineImage) -> Result<ImageModel> {
        self.parse_inline_image(image, Some(page_index))
    }

    fn image_depth(&self, value: &Value, depth: usize) -> Result<ImageModel> {
        if depth > MAX_IMAGE_DEPTH {
            return Err(unsupported(
                "image nesting exceeds 8 levels (possible reference cycle)",
            ));
        }
        let resolved = self.resolve_image_value(value)?;
        let Value::Stream(dict, _) = &resolved else {
            return Err(invalid(format!(
                "image must be a stream, got {}",
                resolved.type_name()
            )));
        };
        if let Some(value) = dict.get("Subtype") {
            let subtype = self.resolve_image_value(value)?;
            match subtype.as_name() {
                Some("Image") => {}
                Some(other) => {
                    return Err(invalid(format!(
                        "image XObject /Subtype must be /Image, got /{other}"
                    )));
                }
                None => return Err(invalid("/Subtype must be a name")),
            }
        }
        let filters = self.image_filters(dict)?;
        let data = if jpx::is_jpx_filter(&filters) {
            // §7.4.9：JPX 步骤透传，保留原始 JPX 字节给语法层解析；
            // 链上其余步骤（Flate 等）与 /Crypt 处理照常。
            self.decode_stream_bytes_except(&resolved, &|name| name == "JPXDecode" || name == "JPX")
        } else {
            self.decode_stream_bytes(&resolved)
        }
        .map_err(ImageError::Filter)?;
        self.parse_image_dict(dict, data, depth, ImageSource::XObject, None)
    }

    fn parse_inline_image(
        &self,
        image: &InlineImage,
        page_index: Option<usize>,
    ) -> Result<ImageModel> {
        let mut dict = Dict::new();
        for (key, value) in &image.dict {
            let key = inline_key(key);
            let value = if key == "ColorSpace" {
                inline_color_space_value(value)
            } else {
                value.clone()
            };
            dict = dict.insert(key, value);
        }
        let filter_dict = Dict::new()
            .insert("Filter", dict.get("Filter").cloned().unwrap_or(Value::Null))
            .insert(
                "DecodeParms",
                dict.get("DecodeParms").cloned().unwrap_or(Value::Null),
            );
        let data = crate::core::filters::decode_stream(&filter_dict, &image.data)
            .map_err(|error| ImageError::Filter(error.to_string()))?;
        let space = match page_index {
            Some(page_index) => self.inline_resource_color_space(&dict, page_index)?,
            None => None,
        };
        self.parse_image_dict(&dict, data, 0, ImageSource::Inline, space)
    }

    /// 内联图像的非内建命名 `/CS` 经页面资源解析；其余交给 `color_space`。
    fn inline_resource_color_space(
        &self,
        dict: &Dict,
        page_index: usize,
    ) -> Result<Option<ColorSpaceModel>> {
        let Some(Value::Name(name)) = dict.get("ColorSpace") else {
            return Ok(None);
        };
        if matches!(
            name.as_str(),
            "DeviceGray" | "DeviceRGB" | "DeviceCMYK" | "Pattern"
        ) {
            return Ok(None);
        }
        self.page_color_space(page_index, name)
            .map_err(ImageError::ColorSpace)
    }

    fn parse_image_dict(
        &self,
        dict: &Dict,
        data: Vec<u8>,
        depth: usize,
        source: ImageSource,
        resolved_space: Option<ColorSpaceModel>,
    ) -> Result<ImageModel> {
        let filters = self.image_filters(dict)?;
        let jpx = if jpx::is_jpx_filter(&filters) {
            jpx::parse_jpx_header(&data)
        } else {
            None
        };
        let image_mask = match dict.get("ImageMask") {
            None => false,
            Some(value) => match self.resolve_image_value(value)? {
                Value::Bool(flag) => flag,
                _ => return Err(invalid("/ImageMask must be a boolean")),
            },
        };
        let width = self.image_dimension(dict, "Width")?;
        let height = self.image_dimension(dict, "Height")?;
        let (color_space, bits_per_component) = if image_mask {
            if let Some(value) = dict.get("BitsPerComponent") {
                let bits = self.image_integer(value, "BitsPerComponent")?;
                if bits != 1 {
                    return Err(invalid(format!(
                        "image mask requires /BitsPerComponent 1, got {bits}"
                    )));
                }
            }
            (None, 1)
        } else {
            // Table 89：JPXDecode 图像的 `/BitsPerComponent`「可选，存在时
            // 必须忽略」——头部可解析时一律以解析出的位深为准。
            let bits = match (
                jpx.as_ref().and_then(JpxHeader::bits_per_component),
                dict.get("BitsPerComponent"),
            ) {
                (Some(bits), _) => bits,
                (None, Some(value)) => {
                    let bits = self.image_integer(value, "BitsPerComponent")?;
                    if !matches!(bits, 1 | 2 | 4 | 8 | 16) {
                        return Err(invalid(format!(
                            "unsupported /BitsPerComponent {bits} (expected 1, 2, 4, 8 or 16)"
                        )));
                    }
                    bits as u8
                }
                (None, None) if source == ImageSource::Inline => 8,
                (None, None) => return Err(invalid("missing required /BitsPerComponent")),
            };
            let space = match resolved_space {
                Some(space) => space,
                None => match dict.get("ColorSpace") {
                    Some(value) => self.color_space(value)?,
                    None if source == ImageSource::Inline => ColorSpaceModel::DeviceGray,
                    // §7.4.9：JPX 的 /ColorSpace 可选；头部解析成功时按
                    // 枚举空间/分量数回退到设备空间。
                    None if jpx.is_some() => {
                        match jpx.as_ref().and_then(JpxHeader::pdf_color_space_hint) {
                            Some("DeviceGray") => ColorSpaceModel::DeviceGray,
                            Some("DeviceRGB") => ColorSpaceModel::DeviceRGB,
                            Some("DeviceCMYK") => ColorSpaceModel::DeviceCMYK,
                            _ => return Err(invalid("missing required /ColorSpace")),
                        }
                    }
                    None => return Err(invalid("missing required /ColorSpace")),
                },
            };
            if space.component_count().is_none() {
                return Err(unsupported("/Pattern color space on an image"));
            }
            (Some(space), bits)
        };

        let decode = match dict.get("Decode") {
            None => None,
            Some(value) => {
                let values = self.image_real_array(value, "Decode")?;
                if values.iter().any(|value| !value.is_finite()) {
                    return Err(invalid("/Decode must contain finite numbers"));
                }
                Some(values)
            }
        };
        if image_mask {
            match &decode {
                Some(decode)
                    if decode.len() != 2
                        || !((decode[0] == 0.0 && decode[1] == 1.0)
                            || (decode[0] == 1.0 && decode[1] == 0.0)) =>
                {
                    return Err(invalid("image mask /Decode must be [0 1] or [1 0]"));
                }
                _ => {}
            }
        } else if let (Some(decode), Some(space)) = (&decode, &color_space) {
            let components = space
                .component_count()
                .ok_or_else(|| unsupported("color space carries no sample components"))?;
            if decode.len() != components * 2 {
                return Err(invalid(format!(
                    "/Decode must contain {} numbers for a {components}-component image, got {}",
                    components * 2,
                    decode.len()
                )));
            }
        }

        let interpolate = match dict.get("Interpolate") {
            None => false,
            Some(value) => match self.resolve_image_value(value)? {
                Value::Bool(flag) => flag,
                _ => return Err(invalid("/Interpolate must be a boolean")),
            },
        };
        let intent = match dict.get("Intent") {
            None => None,
            Some(value) => match self.resolve_image_value(value)? {
                Value::Name(name) => Some(name),
                _ => return Err(invalid("/Intent must be a name")),
            },
        };
        let matte = match dict.get("Matte") {
            None => None,
            Some(_) if image_mask => {
                return Err(invalid("an image mask must not have a /Matte entry"));
            }
            Some(value) => {
                let values = self.image_real_array(value, "Matte")?;
                if values.is_empty() {
                    return Err(invalid("/Matte must contain at least one number"));
                }
                if values.iter().any(|value| !value.is_finite()) {
                    return Err(invalid("/Matte must contain finite numbers"));
                }
                Some(values)
            }
        };
        let smask_in_data = match dict.get("SMaskInData") {
            None => None,
            Some(value) => {
                let flag = self.image_integer(value, "SMaskInData")?;
                if !(0..=2).contains(&flag) {
                    return Err(invalid(format!(
                        "/SMaskInData must be 0, 1 or 2, got {flag}"
                    )));
                }
                Some(flag as u8)
            }
        };

        let mask = match dict.get("Mask") {
            None => None,
            Some(value) => Some(self.parse_image_mask(
                value,
                color_space.as_ref(),
                bits_per_component,
                depth,
            )?),
        };
        if image_mask && (mask.is_some() || dict.get("SMask").is_some()) {
            return Err(invalid(
                "an image mask must not have /Mask or /SMask entries",
            ));
        }

        let smask = match dict.get("SMask") {
            None => None,
            Some(value) => {
                let smask = self.image_depth(value, depth + 1)?;
                if smask.width != width || smask.height != height {
                    return Err(invalid(format!(
                        "SMask dimensions {}x{} do not match image {width}x{height}",
                        smask.width, smask.height
                    )));
                }
                if !matches!(smask.color_space, Some(ColorSpaceModel::DeviceGray)) {
                    return Err(invalid("SMask color space must be DeviceGray"));
                }
                if smask.smask.is_some() {
                    return Err(invalid("SMask must not contain another SMask"));
                }
                if let Some(matte) = &smask.matte {
                    let components = color_space
                        .as_ref()
                        .and_then(|space| space.component_count())
                        .ok_or_else(|| unsupported("SMask on an image without color space"))?;
                    if matte.len() != components {
                        return Err(invalid(format!(
                            "SMask /Matte must contain {components} numbers for a \
                             {components}-component base image, got {}",
                            matte.len()
                        )));
                    }
                }
                Some(Box::new(smask))
            }
        };

        let alternates = match dict.get("Alternates") {
            None => Vec::new(),
            Some(value) => {
                let resolved = self.resolve_image_value(value)?;
                let array = resolved
                    .as_array()
                    .ok_or_else(|| invalid("/Alternates must be an array"))?;
                let mut alternates = Vec::with_capacity(array.0.len());
                for item in &array.0 {
                    let resolved = self.resolve_image_value(item)?;
                    // Table 90：数组元素是 `<< /Image n 0 R >>` 形式的字典；
                    // 为兼容早期实现也接受直接给出的图像流/引用。
                    let image = match &resolved {
                        Value::Dict(entry) => entry.get("Image").cloned().ok_or_else(|| {
                            invalid("/Alternates dictionary must contain an /Image entry")
                        })?,
                        _ => resolved,
                    };
                    alternates.push(self.image_depth(&image, depth + 1)?);
                }
                alternates
            }
        };

        // `filters` 已在函数开头计算（JPX 判定需要先行）。
        let opi = parse_opi_entry(self, dict).map_err(|error| invalid(format!("/OPI: {error}")))?;
        let mut other = BTreeMap::new();
        for (key, value) in &dict.0 {
            if !IMAGE_ENTRIES.contains(&key.as_str()) {
                other.insert(key.clone(), value.clone());
            }
        }

        Ok(ImageModel {
            width,
            height,
            color_space,
            bits_per_component,
            decode,
            interpolate,
            intent,
            matte,
            image_mask,
            mask,
            smask,
            smask_in_data,
            alternates,
            opi,
            jpx,
            filters,
            other,
            data,
        })
    }

    fn parse_image_mask(
        &self,
        value: &Value,
        color_space: Option<&ColorSpaceModel>,
        bits: u8,
        depth: usize,
    ) -> Result<ImageMaskModel> {
        let resolved = self.resolve_image_value(value)?;
        if let Value::Array(array) = &resolved {
            let ranges = array
                .0
                .iter()
                .map(|item| self.image_integer(item, "Mask"))
                .collect::<Result<Vec<_>>>()?;
            let components = color_space
                .and_then(|space| space.component_count())
                .ok_or_else(|| unsupported("color key mask on an image without color space"))?;
            if ranges.len() != components * 2 {
                return Err(invalid(format!(
                    "color key /Mask must contain {} integers for a {components}-component \
                     image, got {}",
                    components * 2,
                    ranges.len()
                )));
            }
            let limit = (1i64 << bits) - 1;
            for pair in ranges.as_chunks::<2>().0 {
                if pair[0] < 0 || pair[0] > pair[1] || pair[1] > limit {
                    return Err(invalid(format!(
                        "color key /Mask ranges must satisfy 0 <= min <= max <= {limit}"
                    )));
                }
            }
            return Ok(ImageMaskModel::ColorKey(ranges));
        }
        let mask = self.image_depth(&resolved, depth + 1)?;
        if !mask.image_mask {
            return Err(invalid(
                "/Mask entry must reference an image mask (/ImageMask true)",
            ));
        }
        Ok(ImageMaskModel::Explicit(Box::new(mask)))
    }

    fn image_dimension(&self, dict: &Dict, key: &str) -> Result<u32> {
        let Some(value) = dict.get(key) else {
            return Err(invalid(format!("missing required /{key}")));
        };
        let value = self.image_integer(value, key)?;
        if !(1..=u32::MAX as i64).contains(&value) {
            return Err(invalid(format!(
                "/{key} must be positive and fit in 32 bits, got {value}"
            )));
        }
        Ok(value as u32)
    }

    fn image_integer(&self, value: &Value, key: &str) -> Result<i64> {
        match self.resolve_image_value(value)? {
            Value::Integer(number) => Ok(number),
            Value::Float(number) if number.fract() == 0.0 => Ok(number as i64),
            other => Err(invalid(format!(
                "/{key} must be an integer, got {}",
                other.type_name()
            ))),
        }
    }

    fn image_real_array(&self, value: &Value, key: &str) -> Result<Vec<f64>> {
        let resolved = self.resolve_image_value(value)?;
        let array = resolved
            .as_array()
            .ok_or_else(|| invalid(format!("/{key} must be an array")))?;
        array
            .0
            .iter()
            .map(|item| match item {
                Value::Integer(number) => Ok(*number as f64),
                Value::Float(number) => Ok(*number),
                other => Err(invalid(format!(
                    "/{key} has a non-numeric element ({})",
                    other.type_name()
                ))),
            })
            .collect()
    }

    fn image_filters(&self, dict: &Dict) -> Result<Vec<String>> {
        let Some(value) = dict.get("Filter") else {
            return Ok(Vec::new());
        };
        let resolved = self.resolve_image_value(value)?;
        let names = match resolved {
            Value::Name(name) => vec![name],
            Value::Null => Vec::new(),
            Value::Array(array) => array
                .0
                .iter()
                .map(|item| match item {
                    Value::Name(name) => Ok(name.clone()),
                    other => Err(invalid(format!(
                        "/Filter array has a non-name element ({})",
                        other.type_name()
                    ))),
                })
                .collect::<Result<Vec<_>>>()?,
            other => {
                return Err(invalid(format!(
                    "/Filter must be a name or array, got {}",
                    other.type_name()
                )));
            }
        };
        Ok(names
            .iter()
            .map(|name| crate::core::filters::canonical_filter(name).to_string())
            .collect())
    }

    fn resolve_image_value(&self, value: &Value) -> Result<Value> {
        match value {
            Value::Ref(num, generation) => {
                let object = self.get_object(*num)?.ok_or_else(|| {
                    invalid(format!("unresolved object reference {num} {generation} R"))
                })?;
                Ok(object.value)
            }
            other => Ok(other.clone()),
        }
    }
}

/// Table 94 的内联图像缩写键 → 标准图像字典键。
fn inline_key(key: &str) -> String {
    match key {
        "BPC" => "BitsPerComponent".into(),
        "CS" => "ColorSpace".into(),
        "D" => "Decode".into(),
        "DP" => "DecodeParms".into(),
        "F" => "Filter".into(),
        "H" => "Height".into(),
        "IM" => "ImageMask".into(),
        "I" => "Interpolate".into(),
        "W" => "Width".into(),
        "L" => "Length".into(),
        other => other.to_string(),
    }
}

/// Table 95 的内联图像颜色空间名缩写（值或颜色空间数组元素）。
fn inline_color_space_value(value: &Value) -> Value {
    match value {
        Value::Name(name) => Value::Name(inline_color_space_name(name).into()),
        Value::Array(array) => Value::Array(Array(
            array.0.iter().map(inline_color_space_value).collect(),
        )),
        other => other.clone(),
    }
}

fn inline_color_space_name(name: &str) -> &str {
    match name {
        "G" => "DeviceGray",
        "RGB" => "DeviceRGB",
        "CMYK" => "DeviceCMYK",
        "I" => "Indexed",
        other => other,
    }
}
