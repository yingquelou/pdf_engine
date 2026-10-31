//! Open Prepress Interface (OPI) 代理字典读端模型（ISO 32000-1 §14.11.7，
//! Table 368–370）。
//!
//! 图像 XObject（Table 89）与 Form XObject（Table 95）都可带可选条目
//! `/OPI`（PDF 1.2 引入）。其值是**OPI 版本字典**（Table 368）：只含一个
//! 条目，键为名字 `1.3` 或 `2.0`，值为对应版本的 OPI 字典（可为间接
//! 引用）。本模块把两版条目类型化：
//!
//! - 1.3（Table 369）：`/F` `/Size` `/CropRect` `/Position` 必填，其余
//!   条目可选；`/Position` 的四边形须两两对边等长。
//! - 2.0（Table 370）：`/F` 必填；`/Size` 与 `/CropRect` 必须同时出现或
//!   同时缺省，且裁切矩形落在代理图像尺寸内。
//!
//! 缺省值**不物化**：带规范默认值的条目保存为 `Option`，由调用方套用
//! 默认值。未建模的其它键（包括另一版本的条目）保留在 [`OpiProxy::dict`]
//! 返回的原始字典中，读端不因未知键报错。

use std::fmt;

use crate::core::reader::action::FileSpec;
use crate::core::reader::navigation::resolve;
use crate::core::types::{Dict, Document, Value};

/// 变长数组条目（`/Tags`、`/Inks`、`/GrayMap`）的条目上限。
const MAX_OPI_ARRAY: usize = 4096;

/// OPI 版本（Table 368 版本字典的键）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpiVersion {
    /// OPI 1.3（Table 369）。
    V1_3,
    /// OPI 2.0（Table 370）。
    V2_0,
}

impl OpiVersion {
    /// 版本字典中的名字键。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::V1_3 => "1.3",
            Self::V2_0 => "2.0",
        }
    }
}

impl fmt::Display for OpiVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// OPI 1.3 `/ColorType`（Table 369）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpiColorType {
    /// 处理色。
    Process,
    /// 专色（缺省）。
    Spot,
    /// 分色。
    Separation,
}

impl OpiColorType {
    /// 字典中的名字。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Process => "Process",
            Self::Spot => "Spot",
            Self::Separation => "Separation",
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        match name {
            "Process" => Some(Self::Process),
            "Spot" => Some(Self::Spot),
            "Separation" => Some(Self::Separation),
            _ => None,
        }
    }
}

/// OPI 1.3 `/Color`（Table 369）：四个 CMYK 分量加色料名字节串。
#[derive(Debug, Clone, PartialEq)]
pub struct OpiColor {
    /// `[C M Y K]`，各分量 0.0–1.0。
    pub cmyk: [f64; 4],
    /// 色料名字节串（颜色名）。
    pub color_name: Vec<u8>,
}

/// OPI 2.0 `/Inks`（Table 370）。
#[derive(Debug, Clone, PartialEq)]
pub enum OpiInks {
    /// 名字 `full_color`。
    FullColor,
    /// 名字 `registration`。
    Registration,
    /// 数组形式 `[/monochrome name1 tint1 ... namen tintn]`。
    Monochrome(Vec<(Vec<u8>, f64)>),
}

/// OPI 2.0 `/Tags` 的 tagText：文本串或文本串数组（Table 370）。
#[derive(Debug, Clone, PartialEq)]
pub enum OpiTagText {
    /// 单个 ASCII 文本串。
    Text(String),
    /// ASCII 文本串数组。
    Texts(Vec<String>),
}

/// OPI 1.3 字典（Table 369）。
///
/// `/Version` 必填且恒为 1.3；`/F` `/Size` `/CropRect` `/Position`
/// 必填。其余条目按规范缺省值保留为 `Option`，不物化默认值。
#[derive(Debug, Clone, PartialEq)]
pub struct Opi13Dictionary {
    /// `/Type`：可选，若出现须为 `OPI`。
    pub type_entry: Option<String>,
    /// `/Version`：必填数字 1.3。
    pub version: f64,
    /// `/F`：外部高分辨率图像的文件规格。
    pub file: FileSpec,
    /// `/ID`：可选字节串。
    pub id: Option<Vec<u8>>,
    /// `/Comments`：可选文本串。
    pub comments: Option<String>,
    /// `/Size`：必填 `[pixelsWide pixelsHigh]`。
    pub size: [i64; 2],
    /// `/CropRect`：必填 `[left top right bottom]`（整数）。
    pub crop_rect: [i64; 4],
    /// `/CropFixed`：可选裁切矩形（实数），缺省同 `/CropRect`。
    pub crop_fixed: Option<[f64; 4]>,
    /// `/Position`：必填八个数字 `[llx lly ulx uly urx ury lrx lry]`。
    pub position: [f64; 8],
    /// `/Resolution`：可选 `[horizRes vertRes]`（每英寸采样数）。
    pub resolution: Option<[f64; 2]>,
    /// `/ColorType`：可选，缺省 `Spot`。
    pub color_type: Option<OpiColorType>,
    /// `/Color`：可选，缺省 `[0 0 0 1 (Black)]`。
    pub color: Option<OpiColor>,
    /// `/Tint`：可选 0.0–1.0，缺省 1.0。
    pub tint: Option<f64>,
    /// `/Overprint`：可选，缺省 false。
    pub overprint: Option<bool>,
    /// `/ImageType`：可选 `[samples bits]`。
    pub image_type: Option<[i64; 2]>,
    /// `/GrayMap`：可选，`2^bits` 个 0–65535 整数。
    pub gray_map: Option<Vec<i64>>,
    /// `/Transparency`：可选，缺省 true。
    pub transparency: Option<bool>,
    /// `/Tags`：可选 `[tagNum1 tagText1 …]` 对，tagText 为文本串。
    pub tags: Vec<(i64, String)>,
    /// 原始 OPI 字典（保留未建模条目）。
    pub dict: Dict,
}

impl Opi13Dictionary {
    /// 以必填条目构造 OPI 1.3 字典（`/Version` 固定 1.3）。
    pub fn new(
        file: impl Into<String>,
        size: [i64; 2],
        crop_rect: [i64; 4],
        position: [f64; 8],
    ) -> Self {
        Self {
            type_entry: None,
            version: 1.3,
            file: FileSpec { file: file.into() },
            id: None,
            comments: None,
            size,
            crop_rect,
            crop_fixed: None,
            position,
            resolution: None,
            color_type: None,
            color: None,
            tint: None,
            overprint: None,
            image_type: None,
            gray_map: None,
            transparency: None,
            tags: Vec::new(),
            dict: Dict::new(),
        }
    }

    /// 设置 `/Type /OPI`。
    pub fn with_type_entry(mut self) -> Self {
        self.type_entry = Some("OPI".into());
        self
    }

    /// 设置 `/ID` 字节串。
    pub fn with_id(mut self, id: impl Into<Vec<u8>>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// 设置 `/Comments` 文本串。
    pub fn with_comments(mut self, comments: impl Into<String>) -> Self {
        self.comments = Some(comments.into());
        self
    }

    /// 设置 `/CropFixed`。
    pub fn with_crop_fixed(mut self, crop: [f64; 4]) -> Self {
        self.crop_fixed = Some(crop);
        self
    }

    /// 设置 `/Resolution`。
    pub fn with_resolution(mut self, resolution: [f64; 2]) -> Self {
        self.resolution = Some(resolution);
        self
    }

    /// 设置 `/ColorType`。
    pub fn with_color_type(mut self, color_type: OpiColorType) -> Self {
        self.color_type = Some(color_type);
        self
    }

    /// 设置 `/Color`。
    pub fn with_color(mut self, color: OpiColor) -> Self {
        self.color = Some(color);
        self
    }

    /// 设置 `/Tint`。
    pub fn with_tint(mut self, tint: f64) -> Self {
        self.tint = Some(tint);
        self
    }

    /// 设置 `/Overprint`。
    pub fn with_overprint(mut self, overprint: bool) -> Self {
        self.overprint = Some(overprint);
        self
    }

    /// 设置 `/ImageType`。
    pub fn with_image_type(mut self, image_type: [i64; 2]) -> Self {
        self.image_type = Some(image_type);
        self
    }

    /// 设置 `/GrayMap`。
    pub fn with_gray_map(mut self, gray_map: impl Into<Vec<i64>>) -> Self {
        self.gray_map = Some(gray_map.into());
        self
    }

    /// 设置 `/Transparency`。
    pub fn with_transparency(mut self, transparency: bool) -> Self {
        self.transparency = Some(transparency);
        self
    }

    /// 追加一个 `/Tags` 对（TIFF 标签号与 ASCII 文本串）。
    pub fn with_tag(mut self, tag: i64, text: impl Into<String>) -> Self {
        self.tags.push((tag, text.into()));
        self
    }
}

/// OPI 2.0 字典（Table 370）。
///
/// `/Version` 必填且为数字 2（或 2.0），`/F` 必填。`/Size` 与
/// `/CropRect` 必须同时出现或同时缺省；出现时裁切矩形取正值并落在
/// 代理图像尺寸内（y 轴向下，故要求 top < bottom）。
#[derive(Debug, Clone, PartialEq)]
pub struct Opi20Dictionary {
    /// `/Type`：可选，若出现须为 `OPI`。
    pub type_entry: Option<String>,
    /// `/Version`：必填数字 2（或 2.0）。
    pub version: f64,
    /// `/F`：低分辨率代理图像的文件规格。
    pub file: FileSpec,
    /// `/MainImage`：可选字节串，全分辨率图像的路径名/标识。
    pub main_image: Option<Vec<u8>>,
    /// `/Tags`：可选对数组，tagText 可为文本串或文本串数组。
    pub tags: Vec<(i64, OpiTagText)>,
    /// `/Size`：可选 `[width height]`（像素）。
    pub size: Option<[f64; 2]>,
    /// `/CropRect`：可选 `[left top right bottom]`。
    pub crop_rect: Option<[f64; 4]>,
    /// `/Overprint`：可选，缺省 false。
    pub overprint: Option<bool>,
    /// `/Inks`：可选，名字或数组。
    pub inks: Option<OpiInks>,
    /// `/IncludedImageDimensions`：可选 `[pixelsWide pixelsHigh]`。
    pub included_image_dimensions: Option<[i64; 2]>,
    /// `/IncludedImageQuality`：可选数字 1、2 或 3。
    pub included_image_quality: Option<u8>,
    /// 原始 OPI 字典（保留未建模条目）。
    pub dict: Dict,
}

impl Opi20Dictionary {
    /// 以必填条目构造 OPI 2.0 字典（`/Version` 固定 2.0）。
    pub fn new(file: impl Into<String>) -> Self {
        Self {
            type_entry: None,
            version: 2.0,
            file: FileSpec { file: file.into() },
            main_image: None,
            tags: Vec::new(),
            size: None,
            crop_rect: None,
            overprint: None,
            inks: None,
            included_image_dimensions: None,
            included_image_quality: None,
            dict: Dict::new(),
        }
    }

    /// 设置 `/Type /OPI`。
    pub fn with_type_entry(mut self) -> Self {
        self.type_entry = Some("OPI".into());
        self
    }

    /// 设置 `/MainImage` 字节串。
    pub fn with_main_image(mut self, main_image: impl Into<Vec<u8>>) -> Self {
        self.main_image = Some(main_image.into());
        self
    }

    /// 追加一个 `/Tags` 对。
    pub fn with_tag(mut self, tag: i64, text: OpiTagText) -> Self {
        self.tags.push((tag, text));
        self
    }

    /// 设置 `/Size` 与 `/CropRect`（Table 370 要求二者成对出现）。
    pub fn with_size_and_crop_rect(mut self, size: [f64; 2], crop_rect: [f64; 4]) -> Self {
        self.size = Some(size);
        self.crop_rect = Some(crop_rect);
        self
    }

    /// 设置 `/Overprint`。
    pub fn with_overprint(mut self, overprint: bool) -> Self {
        self.overprint = Some(overprint);
        self
    }

    /// 设置 `/Inks`。
    pub fn with_inks(mut self, inks: OpiInks) -> Self {
        self.inks = Some(inks);
        self
    }

    /// 设置 `/IncludedImageDimensions`。
    pub fn with_included_image_dimensions(mut self, dimensions: [i64; 2]) -> Self {
        self.included_image_dimensions = Some(dimensions);
        self
    }

    /// 设置 `/IncludedImageQuality`（有效值 1、2、3）。
    pub fn with_included_image_quality(mut self, quality: u8) -> Self {
        self.included_image_quality = Some(quality);
        self
    }
}

/// OPI 代理（Table 368）：版本名 + 对应版本的类型化字典。
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
pub enum OpiProxy {
    /// OPI 1.3（Table 369）。
    V1_3(Opi13Dictionary),
    /// OPI 2.0（Table 370）。
    V2_0(Opi20Dictionary),
}

impl OpiProxy {
    /// 构造 OPI 1.3 代理。
    pub fn v1_3(dictionary: Opi13Dictionary) -> Self {
        Self::V1_3(dictionary)
    }

    /// 构造 OPI 2.0 代理。
    pub fn v2_0(dictionary: Opi20Dictionary) -> Self {
        Self::V2_0(dictionary)
    }

    /// 版本。
    pub fn version(&self) -> OpiVersion {
        match self {
            Self::V1_3(_) => OpiVersion::V1_3,
            Self::V2_0(_) => OpiVersion::V2_0,
        }
    }

    /// 原始 OPI 字典（保留未建模条目）。
    pub fn dict(&self) -> &Dict {
        match self {
            Self::V1_3(dictionary) => &dictionary.dict,
            Self::V2_0(dictionary) => &dictionary.dict,
        }
    }
}

/// OPI 读取的失败原因。
#[derive(Debug, Clone, PartialEq)]
pub enum OpiError {
    /// 条目缺失、类型错误或取值不满足规范。
    Malformed(String),
    /// 变长数组条目超出上限。
    LimitExceeded(String),
}

impl fmt::Display for OpiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed(message) => write!(f, "invalid OPI dictionary: {message}"),
            Self::LimitExceeded(message) => {
                write!(f, "invalid OPI dictionary: limit exceeded: {message}")
            }
        }
    }
}

impl std::error::Error for OpiError {}

fn malformed(message: impl Into<String>) -> OpiError {
    OpiError::Malformed(message.into())
}

fn limit_exceeded(what: &str) -> OpiError {
    OpiError::LimitExceeded(format!("{what} exceeds {MAX_OPI_ARRAY} entries"))
}

impl Document {
    /// 读取图像或 Form XObject 流的 `/OPI`（§14.11.7 Table 368–370）。
    ///
    /// `value` 可以是流对象、流字典（`Value::Stream` 的字典部分也可直接
    /// 传入）或指向它们的间接引用。未声明 `/OPI` 时返回 `Ok(None)`；仅在
    /// 存在时解析版本字典。非流/字典容器返回类型化错误。
    pub fn xobject_opi(&self, value: &Value) -> Result<Option<OpiProxy>, OpiError> {
        let resolved =
            resolve(self, value).map_err(|error| malformed(format!("OPI container: {error}")))?;
        let dict = match &resolved {
            Value::Stream(dict, _) | Value::Dict(dict) => dict,
            other => {
                return Err(malformed(format!(
                    "OPI container must be a stream or dictionary, got {}",
                    other.type_name()
                )));
            }
        };
        parse_opi_entry(self, dict)
    }
}

mod parse;

pub(crate) use parse::parse_opi_entry;

#[cfg(test)]
mod tests;
