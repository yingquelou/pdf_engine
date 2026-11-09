//! 图像模型 [`ImageModel`]、`/Mask` 条目 [`ImageMaskModel`] 与样本读取
//! [`ImageModel::samples`]（不做合成/渲染）。

use std::collections::BTreeMap;

use crate::core::reader::color_space::ColorSpaceModel;
use crate::core::reader::opi::OpiProxy;
use crate::core::types::{Dict, Value};

use super::error::{ImageError, Result, invalid, unsupported};
use super::jbig2::{self, Jbig2ImageInfo};
use super::jpx::{self, JpxHeader};

/// 图像 `/Mask` 条目（Table 89，PDF 1.3）。
#[derive(Debug, Clone, PartialEq)]
pub enum ImageMaskModel {
    /// 显式蒙版：独立的图像蒙版 XObject（`/ImageMask true`，1 位样本）。
    Explicit(Box<ImageModel>),
    /// 颜色键蒙版：`2 × n` 个整数范围，落在范围内的样本不绘制。
    ColorKey(Vec<i64>),
}

/// 图像 `/OC` 条目（Table 89/91，PDF 1.5）：指向可选内容组或可选
/// 内容成员字典的间接引用，或内联的成员字典。本模型只保留引用/字典
/// 形状，不解析可见性。
#[derive(Debug, Clone, PartialEq)]
pub enum ImageOptionalContent {
    /// 间接引用（`n g R`）：目标为 OCG 或 OCMD 字典。
    Reference { object: i64, generation: i64 },
    /// 内联的可选内容成员字典（OCMD）。
    Membership(Dict),
}

/// 图像 `/Metadata` 流（Table 89，PDF 1.4）：流字典与过滤器解码后的
/// 原始 XML 字节。
#[derive(Debug, Clone, PartialEq)]
pub struct ImageMetadata {
    pub dict: Dict,
    pub data: Vec<u8>,
}

/// `/Alternates` 条目（Table 91）：交替图像及其打印缺省标记与 `/OC`。
#[derive(Debug, Clone, PartialEq)]
pub struct AlternateImage {
    /// `/Image`：交替图像本身。
    pub image: ImageModel,
    /// `/DefaultForPrinting`：是否默认用于打印（至多一个为 true）。
    pub default_for_printing: bool,
    /// `/OC`（PDF 1.5）：该交替图像的可选内容。
    pub oc: Option<ImageOptionalContent>,
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
    pub alternates: Vec<AlternateImage>,
    /// `/OC`（PDF 1.5）：可选内容组/成员字典的引用或内联字典。
    pub oc: Option<ImageOptionalContent>,
    /// `/Metadata`（PDF 1.4）：元数据流与其解码后的 XML 字节。
    pub metadata: Option<ImageMetadata>,
    /// `/OPI`：OPI 代理版本字典（§14.11.7 Table 368–370，PDF 1.2）。
    pub opi: Option<OpiProxy>,
    /// JPXDecode 图像的头部语法解析结果（§7.4.9，仅语法层，不解码
    /// 像素）；`/Filter` 不含 JPX 或载荷无法解析时为 `None`。
    pub jpx: Option<JpxHeader>,
    /// JBIG2Decode 图像的段语法解析结果（§7.4.7，仅段语法层，不解码
    /// 像素）：图像段链、`/JBIG2Globals` 全局段流与类型化一致性发现；
    /// `/Filter` 不含 JBIG2 时为 `None`。
    pub jbig2: Option<Jbig2ImageInfo>,
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
        if jbig2::is_jbig2_filter(&self.filters) {
            return Err(unsupported(
                "JBIG2Decode image data is a JBIG2 segment stream; this crate parses the JBIG2 \
                 segment syntax only and does not decode pixels",
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

        // Indexed 与 Separation/DeviceN 的 `/Decode` 作用于查表/求值之前，
        // 必须在样本解码阶段传入；其余色彩空间在解码后按原生分量映射。
        let special_decode = match space {
            ColorSpaceModel::Indexed(_)
            | ColorSpaceModel::Separation(_)
            | ColorSpaceModel::DeviceN(_) => self.decode.as_deref(),
            _ => None,
        };
        let values = space
            .decode_samples_with_decode(bits, width, &self.data, special_decode)
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
                // `/Decode` 已在样本解码阶段作用于调色板索引。
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
                // `/Decode` 已在样本解码阶段作用于 tint 输入。
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
