//! 图像 XObject 与内联图像的解析编排：`/Subtype` 校验、过滤链解码、
//! 图像字典条目建模（Table 89/90/94/95）。

use std::collections::BTreeMap;

use crate::core::reader::color_space::ColorSpaceModel;
use crate::core::reader::content::InlineImage;
use crate::core::reader::opi::parse_opi_entry;
use crate::core::types::{Array, Dict, Document, Value};

use super::error::{ImageError, Result, invalid, unsupported};
use super::jpx::JpxHeader;
use super::model::{AlternateImage, ImageMetadata, ImageModel, ImageOptionalContent};
use super::{jbig2, jpx};

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

/// 图像嵌套（`/Mask`、`/SMask`、`/Alternates`）最大深度，防止引用环。
const MAX_IMAGE_DEPTH: usize = 8;

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

    pub(super) fn image_depth(&self, value: &Value, depth: usize) -> Result<ImageModel> {
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
        let data = if jpx::is_jpx_filter(&filters) || jbig2::is_jbig2_filter(&filters) {
            // §7.4.9/§7.4.7：JPX 与 JBIG2 步骤透传，保留原始字节给语法层
            // 解析；链上其余步骤（Flate 等）与 /Crypt 处理照常。
            self.decode_stream_bytes_except(&resolved, &|name| {
                matches!(name, "JPXDecode" | "JPX" | "JBIG2Decode" | "JBIG2")
            })
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
        // §7.4.7：JBIG2Decode 过滤器不得用于内联图像（shall not）。
        let declared_filters = self.image_filters(&dict)?;
        if jbig2::is_jbig2_filter(&declared_filters) {
            return Err(invalid(
                "JBIG2Decode shall not be used with inline images (PDF 32000-1 7.4.7)",
            ));
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
        let jbig2 = if jbig2::is_jbig2_filter(&filters) {
            Some(self.jbig2_image_info(dict, &data, &filters))
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
        // Table 89 的 `/OC`（PDF 1.5）与 `/Metadata`（PDF 1.4）只定义在
        // 图像 XObject 上；内联图像（Table 93）不含这两个条目。
        let (oc, metadata) = if source == ImageSource::XObject {
            let oc = match dict.get("OC") {
                None => None,
                Some(value) => Some(parse_optional_content(value)?),
            };
            let metadata = match dict.get("Metadata") {
                None => None,
                Some(value) => {
                    let resolved = self.resolve_image_value(value)?;
                    let Value::Stream(stream_dict, _) = &resolved else {
                        return Err(invalid("/Metadata must be a stream"));
                    };
                    let data = self
                        .decode_stream_bytes(&resolved)
                        .map_err(ImageError::Filter)?;
                    Some(ImageMetadata {
                        dict: stream_dict.clone(),
                        data,
                    })
                }
            };
            (oc, metadata)
        } else {
            (None, None)
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
                    // Table 91：数组元素是 `<< /Image n 0 R … >>` 形式的
                    // 字典；为兼容早期实现也接受直接给出的图像流/引用。
                    let (image, default_for_printing, oc) = match &resolved {
                        Value::Dict(entry) => {
                            let image = entry.get("Image").cloned().ok_or_else(|| {
                                invalid("/Alternates dictionary must contain an /Image entry")
                            })?;
                            let default_for_printing = match entry.get("DefaultForPrinting") {
                                None => false,
                                Some(value) => match self.resolve_image_value(value)? {
                                    Value::Bool(flag) => flag,
                                    _ => {
                                        return Err(invalid(
                                            "/DefaultForPrinting must be a boolean",
                                        ));
                                    }
                                },
                            };
                            let oc = match entry.get("OC") {
                                None => None,
                                Some(value) => Some(parse_optional_content(value)?),
                            };
                            (image, default_for_printing, oc)
                        }
                        _ => (resolved.clone(), false, None),
                    };
                    alternates.push(AlternateImage {
                        image: self.image_depth(&image, depth + 1)?,
                        default_for_printing,
                        oc,
                    });
                }
                // Table 91：「At most one alternate for a given base image
                // shall be so designated.」
                if alternates
                    .iter()
                    .filter(|alternate| alternate.default_for_printing)
                    .count()
                    > 1
                {
                    return Err(invalid(
                        "/Alternates: at most one alternate image may set /DefaultForPrinting",
                    ));
                }
                alternates
            }
        };

        // `filters` 已在函数开头计算（JPX 判定需要先行）。
        let opi = parse_opi_entry(self, dict).map_err(|error| invalid(format!("/OPI: {error}")))?;
        let mut other = BTreeMap::new();
        let inline = source == ImageSource::Inline;
        for (key, value) in &dict.0 {
            // 内联图像不含 `/OC`/`/Metadata`（Table 93）；保留在 `other`
            // 以免再次静默丢弃。
            if !IMAGE_ENTRIES.contains(&key.as_str())
                || (inline && matches!(key.as_str(), "OC" | "Metadata"))
            {
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
            oc,
            metadata,
            opi,
            jpx,
            jbig2,
            filters,
            other,
            data,
        })
    }
}

/// 解析 `/OC`（Table 89/91）：只接受间接引用或内联 OCMD 字典。
fn parse_optional_content(value: &Value) -> Result<ImageOptionalContent> {
    match value {
        Value::Ref(object, generation) => Ok(ImageOptionalContent::Reference {
            object: *object,
            generation: *generation,
        }),
        Value::Dict(dict) => Ok(ImageOptionalContent::Membership(dict.clone())),
        other => Err(invalid(format!(
            "/OC must be a reference or an optional content membership dictionary, got {}",
            other.type_name()
        ))),
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
        other => other.to_string(),
    }
}

/// Table 94 的内联图像颜色空间名缩写（值或颜色空间数组元素）。
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
