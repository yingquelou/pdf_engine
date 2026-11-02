//! 名称树与命名目标写端（ISO 32000-1 12.3.2.3、12.7.6）：目录
//! `/Names /Dests`、`/Names /Pages` 的通用叶子节点构造与显式目标的
//! `/D` 值装配。
//!
//! 名称树叶子节点的键按 PDF 文本字符串编码后升序排列（12.3.4.5），
//! 供嵌入文件、模板页等其他名称类别复用。

use crate::core::reader::navigation::Destination;
use crate::core::types::{Array, Dict, PdfTextString, Value};
use crate::core::writer::WriteError;

use super::super::types::text_string_value;
use super::PdfBuilder;

impl PdfBuilder {
    /// 注册一个命名目标（12.3.2.3），写入目录 `/Names /Dests` 名称树。
    ///
    /// 名称重复或目标页码越界时构建返回 [`WriteError::InvalidObject`]；
    /// 目标版本低于 1.2 返回 [`WriteError::UnsupportedVersion`]。
    pub fn named_destination(
        &mut self,
        name: impl Into<String>,
        destination: Destination,
    ) -> &mut Self {
        self.navigation
            .named_destinations
            .push((name.into(), destination));
        self
    }

    /// 注册一个命名页面（12.7.6），写入目录 `/Names /Pages` 名称树。
    ///
    /// 名称重复或页码越界时构建返回 [`WriteError::InvalidObject`]；
    /// 目标版本低于 1.3 返回 [`WriteError::UnsupportedVersion`]。
    pub fn named_page(&mut self, name: impl Into<String>, page: usize) -> &mut Self {
        self.navigation.named_pages.push((name.into(), page));
        self
    }
}

/// 名称树叶子节点：键按 PDF 文本字符串编码后升序排列。
pub(in crate::core::builder) fn name_tree(entries: Vec<(String, Value)>) -> Value {
    let mut entries = entries;
    entries.sort_by_cached_key(|(name, _)| PdfTextString::from(name.as_str()).encode());
    let mut names = Array::new();
    for (name, value) in entries {
        names = names.push(text_string_value(&name)).push(value);
    }
    Value::Dict(Dict::new().insert("Names", Value::Array(names)))
}

pub(in crate::core::builder) fn destination_value(
    destination: &Destination,
    page_refs: &[(i64, i64)],
) -> Result<Value, WriteError> {
    let (num, generation) = page_refs[destination.page()];
    let mut array = Array::new().push(Value::Ref(num, generation));
    array = match destination {
        Destination::Xyz {
            left, top, zoom, ..
        } => array
            .push(Value::Name("XYZ".into()))
            .push(optional_number(*left))
            .push(optional_number(*top))
            .push(optional_number(*zoom)),
        Destination::Fit { .. } => array.push(Value::Name("Fit".into())),
        Destination::FitH { top, .. } => array
            .push(Value::Name("FitH".into()))
            .push(optional_number(*top)),
        Destination::FitV { left, .. } => array
            .push(Value::Name("FitV".into()))
            .push(optional_number(*left)),
        Destination::FitR { rect, .. } => array
            .push(Value::Name("FitR".into()))
            .push(Value::Float(rect[0]))
            .push(Value::Float(rect[1]))
            .push(Value::Float(rect[2]))
            .push(Value::Float(rect[3])),
        Destination::FitB { .. } => array.push(Value::Name("FitB".into())),
        Destination::FitBH { top, .. } => array
            .push(Value::Name("FitBH".into()))
            .push(optional_number(*top)),
        Destination::FitBV { left, .. } => array
            .push(Value::Name("FitBV".into()))
            .push(optional_number(*left)),
    };
    Ok(Value::Array(array))
}

fn optional_number(value: Option<f64>) -> Value {
    value.map_or(Value::Null, Value::Float)
}
