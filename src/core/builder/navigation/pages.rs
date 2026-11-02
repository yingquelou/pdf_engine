//! 页面标签与目录/页面树取址写端（ISO 32000-1 12.4.2）：`/PageLabels`
//! 字典装配，以及导航后处理阶段读取目录对象与页面树 `/Kids` 引用的
//! 公共工具。
//!
//! 页面标签要求目标版本 ≥1.3；每个范围的 `/S`、`/P`、`/St` 仅在非默认
//! 值时写出。

use crate::core::reader::navigation::{PageLabelRange, PageLabelStyle};
use crate::core::types::{Array, Dict, Document, Value};
use crate::core::writer::WriteError;

use super::super::types::text_string_value;
use super::PdfBuilder;
use super::invalid;

impl PdfBuilder {
    /// 设置页面标签范围（12.4.2），按起始页升序给出。
    ///
    /// 每个范围的 `start` 为 0 基页索引且必须严格递增、小于页数；
    /// `first_number` 须 ≥1。页面标签要求目标版本 ≥1.3，否则
    /// [`build`](PdfBuilder::build) 返回 [`WriteError::UnsupportedVersion`]。
    /// 非法输入返回 [`WriteError::InvalidObject`]。
    pub fn page_labels(&mut self, ranges: &[PageLabelRange]) -> &mut Self {
        self.navigation.page_labels = ranges.to_vec();
        self
    }
}

fn style_name(style: PageLabelStyle) -> &'static str {
    match style {
        PageLabelStyle::Decimal => "D",
        PageLabelStyle::UppercaseRoman => "R",
        PageLabelStyle::LowercaseRoman => "r",
        PageLabelStyle::UppercaseLetters => "A",
        PageLabelStyle::LowercaseLetters => "a",
    }
}

pub(super) fn page_labels_dict(labels: &[PageLabelRange]) -> Dict {
    let mut nums = Array::new();
    for range in labels {
        let mut dict = Dict::new();
        if let Some(style) = range.style {
            dict = dict.insert("S", Value::Name(style_name(style).into()));
        }
        if !range.prefix.is_empty() {
            dict = dict.insert("P", text_string_value(&range.prefix));
        }
        if range.first_number != 1 {
            dict = dict.insert("St", Value::Integer(range.first_number as i64));
        }
        nums = nums
            .push(Value::Integer(range.start as i64))
            .push(Value::Dict(dict));
    }
    Dict::new().insert("Nums", Value::Array(nums))
}

/// 目录对象号、代数与字典副本。
pub(super) fn catalog_dict(doc: &Document) -> Result<(i64, i64, Dict), WriteError> {
    let (num, generation) = doc
        .trailer
        .dict
        .get("Root")
        .and_then(Value::as_ref)
        .ok_or_else(|| invalid("document trailer /Root is not an indirect reference"))?;
    let objects = doc.objects_for_write();
    let object = objects
        .iter()
        .find(|object| object.num == num && object.generation == generation)
        .ok_or_else(|| invalid(format!("catalog object {num} {generation} is missing")))?;
    let dict = object
        .value
        .as_dict()
        .cloned()
        .ok_or_else(|| invalid("catalog object is not a dictionary"))?;
    Ok((num, generation, dict))
}

/// 页面树根 `/Kids` 中的间接页引用（构建器只产生扁平树）。
pub(super) fn page_refs(doc: &Document, catalog: &Dict) -> Result<Vec<(i64, i64)>, WriteError> {
    let (pages_num, pages_generation) = catalog
        .get("Pages")
        .and_then(Value::as_ref)
        .ok_or_else(|| invalid("catalog /Pages is not an indirect reference"))?;
    let objects = doc.objects_for_write();
    let pages = objects
        .iter()
        .find(|object| object.num == pages_num && object.generation == pages_generation)
        .ok_or_else(|| invalid("page tree root object is missing"))?;
    let pages = pages
        .value
        .as_dict()
        .ok_or_else(|| invalid("page tree root is not a dictionary"))?;
    let kids = pages
        .get("Kids")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("page tree /Kids is not an array"))?;
    kids.iter()
        .map(|kid| {
            kid.as_ref()
                .ok_or_else(|| invalid("page tree kid is not an indirect reference"))
        })
        .collect()
}
