//! 页面边界写端（ISO 32000-1 14.11.2 Table 30 与 Table 360/361）。
//!
//! [`PageBoundaries`] 是一页的边界构建态：`/MediaBox` 必填，其余框可选。
//! 矩形与框样式复用读端类型（[`Rectangle`] / [`BoxStyle`] /
//! [`BoxStyleName`] / [`BoxColorInfo`]），写出时只物化显式声明的条目。
//!
//! 校验在任何对象号分配与字节写出之前完成（零字节契约）：
//! - 各框须为有限坐标且正宽高（本库选择：零面积框无实际意义，拒绝）；
//! - `/C` 分量 0.0–1.0；`/W` ≥ 0；`/D` 非空、非负且不全为零；
//! - `/D` 只允许与 `/S /D` 同用（Table 361 措辞）；
//! - 扩展 `/S` 名须为合法 PDF 名称；
//! - `/BleedBox` `/TrimBox` `/ArtBox` 需 PDF 1.3，`/BoxColorInfo` 需
//!   PDF 1.4。
//!
//! Table 360 注：页面对象未定义对应框时其框样式条目被忽略。写端按声明
//! 原样写出，不做对应性强制（由调用方保证语义一致）。

use crate::core::reader::page_box::{BoxColorInfo, BoxStyle, BoxStyleName, Rectangle};
use crate::core::types::{Array, Dict, PdfFeature, PdfVersion, Value};
use crate::core::writer::WriteError;

use super::validate_name_value;

/// 一页的页面边界构建态（14.11.2 Table 30）。
#[derive(Debug, Clone, PartialEq)]
pub struct PageBoundaries {
    /// `/MediaBox`：介质框（必填，须正宽高）。
    pub media: Rectangle,
    /// `/CropBox`：裁剪框（缺省等于 MediaBox）。
    pub crop: Option<Rectangle>,
    /// `/BleedBox`：出血框（PDF 1.3，缺省等于 CropBox）。
    pub bleed: Option<Rectangle>,
    /// `/TrimBox`：裁切框（PDF 1.3，缺省等于 CropBox）。
    pub trim: Option<Rectangle>,
    /// `/ArtBox`：作品框（PDF 1.3，缺省等于 CropBox）。
    pub art: Option<Rectangle>,
    /// `/BoxColorInfo`：框颜色信息（PDF 1.4）。
    pub box_color_info: Option<BoxColorInfo>,
}

impl PageBoundaries {
    /// 以 `/MediaBox` 构造；其余框缺省。
    pub fn new(media: Rectangle) -> Self {
        Self {
            media,
            crop: None,
            bleed: None,
            trim: None,
            art: None,
            box_color_info: None,
        }
    }

    /// 设置 `/CropBox`。
    pub fn with_crop(mut self, crop: Rectangle) -> Self {
        self.crop = Some(crop);
        self
    }

    /// 设置 `/BleedBox`（PDF 1.3）。
    pub fn with_bleed(mut self, bleed: Rectangle) -> Self {
        self.bleed = Some(bleed);
        self
    }

    /// 设置 `/TrimBox`（PDF 1.3）。
    pub fn with_trim(mut self, trim: Rectangle) -> Self {
        self.trim = Some(trim);
        self
    }

    /// 设置 `/ArtBox`（PDF 1.3）。
    pub fn with_art(mut self, art: Rectangle) -> Self {
        self.art = Some(art);
        self
    }

    /// 设置 `/BoxColorInfo`（PDF 1.4）。
    pub fn with_box_color_info(mut self, info: BoxColorInfo) -> Self {
        self.box_color_info = Some(info);
        self
    }

    /// 校验全部边界取值与版本门禁，在分配任何对象号之前调用。
    pub(crate) fn validate(&self, version: PdfVersion, context: &str) -> Result<(), WriteError> {
        validate_rectangle(self.media, &format!("{context} /MediaBox"))?;
        for (key, rectangle) in [
            ("CropBox", self.crop),
            ("BleedBox", self.bleed),
            ("TrimBox", self.trim),
            ("ArtBox", self.art),
        ] {
            if let Some(rectangle) = rectangle {
                validate_rectangle(rectangle, &format!("{context} /{key}"))?;
            }
        }

        let has_processing_box = self.bleed.is_some() || self.trim.is_some() || self.art.is_some();
        if has_processing_box && !version.supports(PdfFeature::PageBoundaries) {
            return Err(WriteError::UnsupportedVersion(format!(
                "{context}: {} require PDF {}, got {}",
                PdfFeature::PageBoundaries,
                PdfFeature::PageBoundaries.min_version().as_str(),
                version.as_str()
            )));
        }
        if self.box_color_info.is_some() && !version.supports(PdfFeature::PageBoxColorInfo) {
            return Err(WriteError::UnsupportedVersion(format!(
                "{context}: {} require PDF {}, got {}",
                PdfFeature::PageBoxColorInfo,
                PdfFeature::PageBoxColorInfo.min_version().as_str(),
                version.as_str()
            )));
        }
        if let Some(info) = &self.box_color_info {
            for (key, style) in [
                ("CropBox", &info.crop),
                ("BleedBox", &info.bleed),
                ("TrimBox", &info.trim),
                ("ArtBox", &info.art),
            ] {
                if let Some(style) = style {
                    validate_style(style, &format!("{context} /BoxColorInfo /{key}"))?;
                }
            }
        }
        Ok(())
    }

    /// `/MediaBox` 数组值。
    pub(crate) fn media_array(&self) -> Array {
        rectangle_array(self.media)
    }

    /// 本次写出会登记的特性。
    pub(crate) fn features(&self) -> Vec<PdfFeature> {
        let mut features = Vec::new();
        if self.bleed.is_some() || self.trim.is_some() || self.art.is_some() {
            features.push(PdfFeature::PageBoundaries);
        }
        if self.box_color_info.is_some() {
            features.push(PdfFeature::PageBoxColorInfo);
        }
        features
    }

    /// 把显式声明的框与 `/BoxColorInfo` 并入页面字典（不含 `/MediaBox`）。
    ///
    /// 仅写出已声明条目：缺省框与缺省样式均由读者按 Table 30/361 推导。
    pub(crate) fn extend_page_dict(&self, page_dict: Dict) -> Dict {
        let mut dict = page_dict;
        for (key, rectangle) in [
            ("CropBox", self.crop),
            ("BleedBox", self.bleed),
            ("TrimBox", self.trim),
            ("ArtBox", self.art),
        ] {
            if let Some(rectangle) = rectangle {
                dict = dict.insert(key, Value::Array(rectangle_array(rectangle)));
            }
        }
        if let Some(info) = &self.box_color_info {
            let mut info_dict = Dict::new();
            for (key, style) in [
                ("CropBox", &info.crop),
                ("BleedBox", &info.bleed),
                ("TrimBox", &info.trim),
                ("ArtBox", &info.art),
            ] {
                if let Some(style) = style {
                    info_dict = info_dict.insert(key, Value::Dict(style_dict(style)));
                }
            }
            dict = dict.insert("BoxColorInfo", Value::Dict(info_dict));
        }
        dict
    }
}

/// 矩形须坐标有限且宽高为正。
fn validate_rectangle(rectangle: Rectangle, context: &str) -> Result<(), WriteError> {
    for (name, value) in [
        ("llx", rectangle.llx),
        ("lly", rectangle.lly),
        ("urx", rectangle.urx),
        ("ury", rectangle.ury),
    ] {
        if !value.is_finite() {
            return Err(WriteError::InvalidObject(format!(
                "{context} /{name} must be finite"
            )));
        }
    }
    if rectangle.width() <= 0.0 || rectangle.height() <= 0.0 {
        return Err(WriteError::InvalidObject(format!(
            "{context} must have positive width and height"
        )));
    }
    Ok(())
}

/// 框样式字典（Table 361）取值契约。
fn validate_style(style: &BoxStyle, context: &str) -> Result<(), WriteError> {
    if let Some(color) = &style.color {
        for (index, component) in color.iter().enumerate() {
            if !component.is_finite() || !(0.0..=1.0).contains(component) {
                return Err(WriteError::InvalidObject(format!(
                    "{context} /C component {index} must be between 0 and 1"
                )));
            }
        }
    }
    if let Some(width) = style.width
        && (!width.is_finite() || width < 0.0)
    {
        return Err(WriteError::InvalidObject(format!(
            "{context} /W must be a non-negative finite number"
        )));
    }
    if let Some(BoxStyleName::Other(name)) = &style.style {
        validate_name_value(name, &format!("{context} /S")).map_err(WriteError::InvalidObject)?;
    }
    if let Some(dash) = &style.dash {
        // Table 361：虚线数组只用于 /S /D。
        if !matches!(style.style, Some(BoxStyleName::Dashed)) {
            return Err(WriteError::InvalidObject(format!(
                "{context} /D is only allowed when /S is D (Table 361)"
            )));
        }
        if dash.is_empty() {
            return Err(WriteError::InvalidObject(format!(
                "{context} /D must not be empty"
            )));
        }
        if dash
            .iter()
            .any(|element| !element.is_finite() || *element < 0.0)
        {
            return Err(WriteError::InvalidObject(format!(
                "{context} /D must contain non-negative finite numbers"
            )));
        }
        if dash.iter().all(|element| *element == 0.0) {
            return Err(WriteError::InvalidObject(format!(
                "{context} /D must not be all zero"
            )));
        }
    }
    Ok(())
}

/// 矩形 → `[llx lly urx ury]` 数组。
fn rectangle_array(rectangle: Rectangle) -> Array {
    Array::new()
        .push(Value::Float(rectangle.llx))
        .push(Value::Float(rectangle.lly))
        .push(Value::Float(rectangle.urx))
        .push(Value::Float(rectangle.ury))
}

/// 框样式 → 字典（只含已声明条目）。
fn style_dict(style: &BoxStyle) -> Dict {
    let mut dict = Dict::new();
    if let Some(color) = style.color {
        dict = dict.insert(
            "C",
            Value::Array(Array(
                color.iter().map(|value| Value::Float(*value)).collect(),
            )),
        );
    }
    if let Some(width) = style.width {
        dict = dict.insert("W", Value::Float(width));
    }
    if let Some(name) = &style.style {
        dict = dict.insert("S", Value::Name(name.name().to_owned()));
    }
    if let Some(dash) = &style.dash {
        dict = dict.insert(
            "D",
            Value::Array(Array(
                dash.iter().map(|value| Value::Float(*value)).collect(),
            )),
        );
    }
    dict
}
