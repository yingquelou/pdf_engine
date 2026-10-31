//! 分色字典写出（ISO 32000-1 14.11.4 Table 364）。
//!
//! 一组分色页共享同一个 `/SeparationInfo` 字典：`/Pages` 为组内页对象
//! 引用的数组，`/DeviceColorant` 为名字或文本串，可选 `/ColorSpace` 为
//! Separation/DeviceN 颜色空间。校验在任何对象号分配与字节写出之前
//! 完成（零字节契约）：组非空、页号在范围内、组内不重复、组间不重叠、
//! 色料名合法、颜色空间类型与取值合法，以及 PDF 1.3 版本门禁。

use crate::core::builder::types::{
    ColorSpace, DeviceColorant, SeparationGroup, text_string_value, validate_name_value,
};
use crate::core::types::{Array, Dict, PdfFeature, PdfVersion, Value};
use crate::core::writer::WriteError;

/// 校验分色页组注册，返回 `页号 → 组下标` 映射（长度 `page_count`）。
pub(super) fn validate_groups(
    groups: &[SeparationGroup],
    page_count: usize,
    version: PdfVersion,
) -> Result<Vec<Option<usize>>, WriteError> {
    let mut page_map: Vec<Option<usize>> = vec![None; page_count];
    if groups.is_empty() {
        return Ok(page_map);
    }
    if !version.supports(PdfFeature::SeparationInfo) {
        return Err(WriteError::UnsupportedVersion(format!(
            "{} require PDF {}, got {}",
            PdfFeature::SeparationInfo,
            PdfFeature::SeparationInfo.min_version().as_str(),
            version.as_str()
        )));
    }
    for (index, group) in groups.iter().enumerate() {
        let context = format!("separation group {index}");
        if group.pages.is_empty() {
            return Err(WriteError::InvalidObject(format!(
                "{context} must contain at least one page"
            )));
        }
        for &page in &group.pages {
            if page >= page_count {
                return Err(WriteError::InvalidObject(format!(
                    "{context} page {page} is out of range (page count {page_count})"
                )));
            }
            match page_map[page] {
                Some(existing) if existing == index => {
                    return Err(WriteError::InvalidObject(format!(
                        "{context} lists page {page} more than once"
                    )));
                }
                Some(existing) => {
                    return Err(WriteError::InvalidObject(format!(
                        "{context} page {page} is already in separation group {existing}"
                    )));
                }
                None => page_map[page] = Some(index),
            }
        }
        match &group.info.colorant {
            DeviceColorant::Name(name) => {
                validate_name_value(name, &format!("{context} /DeviceColorant"))
                    .map_err(WriteError::InvalidObject)?;
            }
            DeviceColorant::Text(text) => {
                if text.is_empty() {
                    return Err(WriteError::InvalidObject(format!(
                        "{context} /DeviceColorant must not be empty"
                    )));
                }
            }
        }
        if let Some(space) = &group.info.color_space {
            if !matches!(space, ColorSpace::Separation(_) | ColorSpace::DeviceN(_)) {
                return Err(WriteError::InvalidObject(format!(
                    "{context} /ColorSpace must be a Separation or DeviceN color space"
                )));
            }
            space.validate().map_err(|message| {
                WriteError::InvalidObject(format!("{context} /ColorSpace: {message}"))
            })?;
        }
    }
    Ok(page_map)
}

/// 构造分色字典（Table 364）：`/Pages`、`/DeviceColorant` 与可选
/// `/ColorSpace`。颜色空间值由调用方按组缓存，多页共享同一批流对象。
pub(super) fn info_dict(
    group: &SeparationGroup,
    page_slots: &[(i64, i64)],
    color_space: Option<Value>,
) -> Dict {
    let pages = group
        .pages
        .iter()
        .map(|&page| Value::Ref(page_slots[page].0, 0))
        .collect::<Vec<Value>>();
    let colorant = match &group.info.colorant {
        DeviceColorant::Name(name) => Value::Name(name.clone()),
        DeviceColorant::Text(text) => text_string_value(text),
    };
    let mut dict = Dict::new()
        .insert("Pages", Value::Array(Array(pages)))
        .insert("DeviceColorant", colorant);
    if let Some(space) = color_space {
        dict = dict.insert("ColorSpace", space);
    }
    dict
}
