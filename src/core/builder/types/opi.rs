//! OPI 代理字典写端（ISO 32000-1 §14.11.7 Table 368–370）。
//!
//! 图像 XObject（Table 89）与 Form XObject（Table 95）的可选 `/OPI`
//! 条目写出为**直接**的版本字典 `<< /1.3 << … >> >>` 或 `<< /2.0 << … >> >>`
//! （Table 368 的值就是字典，无需额外对象号）。类型化模型与读端共用
//! [`crate::core::reader::opi`] 的定义，写端在此补充取值校验与字典序列化：
//!
//! - 1.3 字典（Table 369）：`/Size` 与 `/CropRect` 为正且在图像范围内，
//!   `/Position` 两两对边等长，`/Color` 分量与 `/Tint` 落在 0.0–1.0，
//!   `/GrayMap` 长度为 `2^bits` 且值在 0–65535。
//! - 2.0 字典（Table 370）：`/Size` 与 `/CropRect` 成对出现且裁切框
//!   落在代理尺寸内，`/Inks` 的 monochrome 数组为名字/色值对，
//!   `/IncludedImageQuality` 为 1、2 或 3。
//!
//! 规范默认值不物化：模型中的 `Option` 为 `Some` 时才写出该条目。1.3
//! 专有条目与 2.0 专有条目由两个字典类型隔离，无法互相误写。

use crate::core::types::{Array, Dict, Value};

pub use crate::core::reader::opi::{
    Opi13Dictionary, Opi20Dictionary, OpiColor, OpiColorType, OpiInks, OpiProxy, OpiTagText,
    OpiVersion,
};

use super::text_string_value;

impl OpiProxy {
    /// 校验 OPI 代理字典的全部写出条目；失败返回可读原因。
    pub(crate) fn validate(&self) -> Result<(), String> {
        match self {
            Self::V1_3(dictionary) => dictionary.validate(),
            Self::V2_0(dictionary) => dictionary.validate(),
        }
    }

    /// 序列化为 Table 368 版本字典（单条目，值为直接 OPI 字典）。
    pub(crate) fn to_dict(&self) -> Dict {
        match self {
            Self::V1_3(dictionary) => {
                Dict::new().insert(OpiVersion::V1_3.as_str(), Value::Dict(dictionary.to_dict()))
            }
            Self::V2_0(dictionary) => {
                Dict::new().insert(OpiVersion::V2_0.as_str(), Value::Dict(dictionary.to_dict()))
            }
        }
    }
}

impl Opi13Dictionary {
    /// 校验 Table 369 的全部写出条目。
    pub(crate) fn validate(&self) -> Result<(), String> {
        const CONTEXT: &str = "OPI 1.3 dictionary";
        validate_type_entry(self.type_entry.as_deref(), CONTEXT)?;
        if self.version != 1.3 {
            return Err(format!(
                "{CONTEXT} /Version must be the number 1.3, got {}",
                self.version
            ));
        }
        validate_file(&self.file.file, CONTEXT)?;
        if self.size[0] <= 0 || self.size[1] <= 0 {
            return Err(format!("{CONTEXT} /Size must be positive"));
        }
        validate_crop_i64(self.crop_rect, self.size, CONTEXT)?;
        if let Some(crop_fixed) = &self.crop_fixed
            && crop_fixed.iter().any(|value| !value.is_finite())
        {
            return Err(format!("{CONTEXT} /CropFixed must contain finite numbers"));
        }
        if self.position.iter().any(|value| !value.is_finite()) {
            return Err(format!("{CONTEXT} /Position must contain finite numbers"));
        }
        validate_position(&self.position, CONTEXT)?;
        if let Some(resolution) = &self.resolution
            && resolution.iter().any(|value| !value.is_finite())
        {
            return Err(format!("{CONTEXT} /Resolution must contain finite numbers"));
        }
        if let Some(color) = &self.color {
            validate_color(color, CONTEXT)?;
        }
        if let Some(tint) = self.tint
            && (!tint.is_finite() || !(0.0..=1.0).contains(&tint))
        {
            return Err(format!("{CONTEXT} /Tint must be in 0.0–1.0"));
        }
        if let Some(image_type) = self.image_type {
            if image_type[0] <= 0 {
                return Err(format!("{CONTEXT} /ImageType samples must be positive"));
            }
            if !(1..=8).contains(&image_type[1]) {
                return Err(format!("{CONTEXT} /ImageType bits per sample must be 1–8"));
            }
            if let Some(gray_map) = &self.gray_map {
                let expected = 1usize << image_type[1];
                if gray_map.len() != expected {
                    return Err(format!(
                        "{CONTEXT} /GrayMap must contain 2^{} = {expected} integers",
                        image_type[1]
                    ));
                }
                if gray_map.iter().any(|value| !(0..=65535).contains(value)) {
                    return Err(format!("{CONTEXT} /GrayMap values must be in 0–65535"));
                }
            }
        } else if self.gray_map.is_some() {
            return Err(format!("{CONTEXT} /GrayMap requires /ImageType"));
        }
        for (tag, text) in &self.tags {
            validate_tag(*tag, CONTEXT)?;
            if !text.is_ascii() {
                return Err(format!("{CONTEXT} /Tags text must be ASCII"));
            }
        }
        Ok(())
    }

    /// 序列化为 Table 369 字典。
    fn to_dict(&self) -> Dict {
        let mut dict = Dict::new()
            .insert("Version", Value::Float(1.3))
            .insert("F", text_string_value(&self.file.file))
            .insert("Size", integer_array(&self.size))
            .insert("CropRect", integer_array(&self.crop_rect))
            .insert("Position", number_array(&self.position));
        if let Some(type_entry) = &self.type_entry {
            dict = dict.insert("Type", Value::Name(type_entry.clone()));
        }
        if let Some(id) = &self.id {
            dict = dict.insert("ID", hex_string_value(id));
        }
        if let Some(comments) = &self.comments {
            dict = dict.insert("Comments", text_string_value(comments));
        }
        if let Some(crop_fixed) = &self.crop_fixed {
            dict = dict.insert("CropFixed", number_array(crop_fixed));
        }
        if let Some(resolution) = &self.resolution {
            dict = dict.insert("Resolution", number_array(resolution));
        }
        if let Some(color_type) = self.color_type {
            dict = dict.insert("ColorType", Value::Name(color_type.as_str().into()));
        }
        if let Some(color) = &self.color {
            let mut array = number_array(&color.cmyk).into_array().expect("array");
            array.0.push(hex_string_value(&color.color_name));
            dict = dict.insert("Color", Value::Array(array));
        }
        if let Some(tint) = self.tint {
            dict = dict.insert("Tint", Value::Float(tint));
        }
        if let Some(overprint) = self.overprint {
            dict = dict.insert("Overprint", Value::Bool(overprint));
        }
        if let Some(image_type) = self.image_type {
            dict = dict.insert("ImageType", integer_array(&image_type));
        }
        if let Some(gray_map) = &self.gray_map {
            dict = dict.insert("GrayMap", integer_array(gray_map));
        }
        if let Some(transparency) = self.transparency {
            dict = dict.insert("Transparency", Value::Bool(transparency));
        }
        if !self.tags.is_empty() {
            let mut array = Array::new();
            for (tag, text) in &self.tags {
                array = array
                    .push(Value::Integer(*tag))
                    .push(text_string_value(text));
            }
            dict = dict.insert("Tags", Value::Array(array));
        }
        dict
    }
}

impl Opi20Dictionary {
    /// 校验 Table 370 的全部写出条目。
    pub(crate) fn validate(&self) -> Result<(), String> {
        const CONTEXT: &str = "OPI 2.0 dictionary";
        validate_type_entry(self.type_entry.as_deref(), CONTEXT)?;
        if self.version != 2.0 {
            return Err(format!(
                "{CONTEXT} /Version must be the number 2 or 2.0, got {}",
                self.version
            ));
        }
        validate_file(&self.file.file, CONTEXT)?;
        match (&self.size, &self.crop_rect) {
            (None, None) => {}
            (Some(size), Some(crop)) => {
                if size.iter().any(|value| !value.is_finite()) || size[0] <= 0.0 || size[1] <= 0.0 {
                    return Err(format!("{CONTEXT} /Size must be positive and finite"));
                }
                if crop.iter().any(|value| !value.is_finite()) {
                    return Err(format!("{CONTEXT} /CropRect must contain finite numbers"));
                }
                if crop[0] < 0.0 || crop[1] < 0.0 || crop[0] >= crop[2] || crop[1] >= crop[3] {
                    return Err(format!(
                        "{CONTEXT} /CropRect must satisfy 0 <= left < right and 0 <= top < bottom"
                    ));
                }
                if crop[2] > size[0] || crop[3] > size[1] {
                    return Err(format!(
                        "{CONTEXT} /CropRect must lie within /Size [{}, {}]",
                        size[0], size[1]
                    ));
                }
            }
            _ => {
                return Err(format!(
                    "{CONTEXT} /Size and /CropRect must both be present or both be absent"
                ));
            }
        }
        if let Some(inks) = &self.inks
            && let OpiInks::Monochrome(entries) = inks
        {
            for (name, tint) in entries {
                if name.is_empty() {
                    return Err(format!("{CONTEXT} /Inks name must not be empty"));
                }
                if !tint.is_finite() || !(0.0..=1.0).contains(tint) {
                    return Err(format!("{CONTEXT} /Inks tint must be in 0.0–1.0"));
                }
            }
        }
        if let Some(dimensions) = self.included_image_dimensions
            && (dimensions[0] <= 0 || dimensions[1] <= 0)
        {
            return Err(format!(
                "{CONTEXT} /IncludedImageDimensions must be positive"
            ));
        }
        if let Some(quality) = self.included_image_quality
            && !(1..=3).contains(&quality)
        {
            return Err(format!(
                "{CONTEXT} /IncludedImageQuality must be 1, 2 or 3, got {quality}"
            ));
        }
        for (tag, text) in &self.tags {
            validate_tag(*tag, CONTEXT)?;
            match text {
                OpiTagText::Text(text) => {
                    if !text.is_ascii() {
                        return Err(format!("{CONTEXT} /Tags text must be ASCII"));
                    }
                }
                OpiTagText::Texts(texts) => {
                    if texts.iter().any(|text| !text.is_ascii()) {
                        return Err(format!("{CONTEXT} /Tags text must be ASCII"));
                    }
                }
            }
        }
        Ok(())
    }

    /// 序列化为 Table 370 字典。
    fn to_dict(&self) -> Dict {
        let mut dict = Dict::new()
            .insert("Version", Value::Float(2.0))
            .insert("F", text_string_value(&self.file.file));
        if let Some(type_entry) = &self.type_entry {
            dict = dict.insert("Type", Value::Name(type_entry.clone()));
        }
        if let Some(main_image) = &self.main_image {
            dict = dict.insert("MainImage", hex_string_value(main_image));
        }
        if let Some(size) = &self.size {
            dict = dict.insert("Size", number_array(size));
        }
        if let Some(crop_rect) = &self.crop_rect {
            dict = dict.insert("CropRect", number_array(crop_rect));
        }
        if let Some(overprint) = self.overprint {
            dict = dict.insert("Overprint", Value::Bool(overprint));
        }
        if let Some(inks) = &self.inks {
            let value = match inks {
                OpiInks::FullColor => Value::Name("full_color".into()),
                OpiInks::Registration => Value::Name("registration".into()),
                OpiInks::Monochrome(entries) => {
                    let mut array = Array::new().push(Value::Name("monochrome".into()));
                    for (name, tint) in entries {
                        array = array.push(hex_string_value(name)).push(Value::Float(*tint));
                    }
                    Value::Array(array)
                }
            };
            dict = dict.insert("Inks", value);
        }
        if let Some(dimensions) = self.included_image_dimensions {
            dict = dict.insert("IncludedImageDimensions", integer_array(&dimensions));
        }
        if let Some(quality) = self.included_image_quality {
            dict = dict.insert("IncludedImageQuality", Value::Integer(quality as i64));
        }
        if !self.tags.is_empty() {
            let mut array = Array::new();
            for (tag, text) in &self.tags {
                array = array.push(Value::Integer(*tag));
                array = match text {
                    OpiTagText::Text(text) => array.push(text_string_value(text)),
                    OpiTagText::Texts(texts) => array.push(Value::Array(Array(
                        texts.iter().map(|text| text_string_value(text)).collect(),
                    ))),
                };
            }
            dict = dict.insert("Tags", Value::Array(array));
        }
        dict
    }
}

fn validate_type_entry(type_entry: Option<&str>, context: &str) -> Result<(), String> {
    if let Some(name) = type_entry
        && name != "OPI"
    {
        return Err(format!("{context} /Type must be /OPI when present"));
    }
    Ok(())
}

fn validate_file(file: &str, context: &str) -> Result<(), String> {
    if file.is_empty() {
        return Err(format!("{context} /F must not be empty"));
    }
    Ok(())
}

fn validate_crop_i64(crop: [i64; 4], size: [i64; 2], context: &str) -> Result<(), String> {
    if crop[0] < 0 || crop[1] < 0 || crop[0] >= crop[2] || crop[1] >= crop[3] {
        return Err(format!(
            "{context} /CropRect must satisfy 0 <= left < right and 0 <= top < bottom"
        ));
    }
    if crop[2] > size[0] || crop[3] > size[1] {
        return Err(format!(
            "{context} /CropRect must lie within /Size [{}, {}]",
            size[0], size[1]
        ));
    }
    Ok(())
}

fn validate_position(position: &[f64; 8], context: &str) -> Result<(), String> {
    const TOLERANCE: f64 = 1e-6;
    let top_width = position[2] - position[0];
    let bottom_width = position[4] - position[6];
    let left_height = position[3] - position[1];
    let right_height = position[5] - position[7];
    if (top_width - bottom_width).abs() > TOLERANCE
        || (left_height - right_height).abs() > TOLERANCE
    {
        return Err(format!(
            "{context} /Position corners must form a rectangle \
             (ulx-llx = urx-lrx and uly-lly = ury-lry)"
        ));
    }
    Ok(())
}

fn validate_color(color: &OpiColor, context: &str) -> Result<(), String> {
    if color
        .cmyk
        .iter()
        .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
    {
        return Err(format!("{context} /Color components must be in 0.0–1.0"));
    }
    Ok(())
}

fn validate_tag(tag: i64, context: &str) -> Result<(), String> {
    if !(0..=65535).contains(&tag) {
        return Err(format!(
            "{context} /Tags tag number must be a TIFF tag number in 0–65535"
        ));
    }
    Ok(())
}

fn hex_string_value(bytes: &[u8]) -> Value {
    Value::HexString(hex_string(bytes))
}

fn hex_string(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 0x0F) as usize] as char);
    }
    out
}

fn integer_array(values: &[i64]) -> Value {
    Value::Array(Array(
        values.iter().map(|value| Value::Integer(*value)).collect(),
    ))
}

fn number_array(values: &[f64]) -> Value {
    Value::Array(Array(
        values.iter().map(|value| Value::Float(*value)).collect(),
    ))
}

/// 供 `Value::Array` 追加条目时取回内部数组。
trait IntoArray {
    fn into_array(self) -> Option<Array>;
}

impl IntoArray for Value {
    fn into_array(self) -> Option<Array> {
        match self {
            Value::Array(array) => Some(array),
            _ => None,
        }
    }
}
