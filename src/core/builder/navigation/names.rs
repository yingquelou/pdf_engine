//! 名称树与命名目标写端（ISO 32000-1 12.3.2.3、12.7.6）：目录
//! `/Names /Dests`、`/Names /Pages` 的通用叶子节点构造与显式目标的
//! `/D` 值装配。
//!
//! 名称树叶子节点的键按 PDF 文本字符串编码后升序排列（12.3.4.5），
//! 供嵌入文件、模板页等其他名称类别复用。
//!
//! 本文件还实现 12.3.2.3 的**旧式（PDF 1.1）命名目标通道**：目录 `/Dests`
//! 字典，键是名字对象（不是文本串）、值是 Table 151 的目标数组。它与名称树
//! 通道是**两条独立显式 API**，互不干涉：NOTE 3 建议无向后兼容压力时应用
//! 用名称树的文本串形态，故同一名字在两处各注册一次不会被本库判为重名
//! （规范无此 shall，读端也按名称树优先解析旧式字典）。

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

    /// 注册一个**旧式**命名目标（12.3.2.3，PDF 1.1 形态），写入目录 `/Dests`
    /// 字典（Table 28：该字典 shall be an indirect reference，故写出为
    /// 独立间接对象，目录条目只是引用）。
    ///
    /// 名字是**名字对象**（非文本串）：须非空、不含 NUL 且限于可打印 ASCII
    /// （0x20–0x7E；拒控制字符与非 ASCII 属库策略而非规范 shall），
    /// 并在本通道内互不重复，否则构建返回 [`WriteError::InvalidObject`]；
    /// 页码越界同样返回该错误，目标版本低于 1.1 返回
    /// [`WriteError::UnsupportedVersion`]，均零字节输出。
    ///
    /// 与 [`Self::named_destination`]（1.2+ 名称树通道）互不干涉：同一名字
    /// 在两条通道各注册一次不会被判为重名（12.3.2.3 NOTE 3）。本轮只写
    /// Table 151 的**数组形态**，含 `/D` 条目的字典形态尚未实现。
    pub fn legacy_named_destination(
        &mut self,
        name: impl Into<String>,
        destination: Destination,
    ) -> &mut Self {
        self.navigation
            .legacy_named_destinations
            .push((name.into(), destination));
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

/// 旧式目录 `/Dests` 字典（12.3.2.3，PDF 1.1 形态）。
///
/// 每个键是一个 destination name（**名字对象**，不是文本串）：`Dict` 的键
/// 写出时经 `writer::value::write_name` 序列化为名字，因此这里直接存原始
/// 字符串，**不**走名称树的 `text_string_value` 编码路径。键按名字字节序
/// 升序（`Dict` 底层是 `BTreeMap<String, _>`，插入前再显式排序一次）。
///
/// 值复用 [`destination_value`]，与名称树通道共享同一套 Table 151 目标
/// 数组装配；含 `/D` 条目的字典形态（NOTE 2）本轮不写。
pub(in crate::core::builder) fn legacy_dests_dict(
    entries: &[(String, Destination)],
    page_refs: &[(i64, i64)],
) -> Result<Value, WriteError> {
    let mut entries = entries.to_vec();
    entries.sort_by_cached_key(|(name, _)| name.as_bytes().to_vec());
    let mut dict = Dict::new();
    for (name, destination) in &entries {
        dict = dict.insert(name.clone(), destination_value(destination, page_refs)?);
    }
    Ok(Value::Dict(dict))
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
