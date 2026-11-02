//! 名称树与命名目标写端（ISO 32000-1 12.3.2.3、12.7.6）：目录
//! `/Names /Dests`、`/Names /Pages` 的通用叶子节点构造与显式目标的
//! `/D` 值装配。
//!
//! 名称树叶子节点的键按 PDF 文本字符串编码后升序排列（12.3.4.5），
//! 供嵌入文件、模板页等其他名称类别复用。
//!
//! 本文件还实现 12.3.2.3 的**旧式（PDF 1.1）命名目标通道**：目录 `/Dests`
//! 字典，键是名字对象（不是文本串）、值是 Table 151 的目标数组或含 `/D`
//! 数组的字典（12.3.2.3 第 2 段两种形态并列，NOTE 2 说明字典形态用于附加
//! 属性）。它与名称树通道是**两条独立显式 API**，互不干涉：NOTE 3 建议无
//! 向后兼容压力时应用名称树的文本串形态，故同一名字在两处各注册一次不会被
//! 本库判为重名（规范无此 shall，读端也按名称树优先解析旧式字典）。

use crate::core::reader::navigation::Destination;
use crate::core::types::{Array, Dict, PdfTextString, Value};
use crate::core::writer::WriteError;

use super::super::types::text_string_value;
use super::PdfBuilder;

/// 旧式目录 `/Dests` 条目的值形态（12.3.2.3，PDF 1.1 段落）。
///
/// 规范原文（PDF32000_2008.pdf `pdftotext -raw` L20521–L20524）：「The value
/// of this entry shall be a dictionary in which each key is a destination name
/// and the corresponding value is either an array defining the destination,
/// using the syntax shown in Table 151, or a dictionary with a D entry whose
/// value is such an array.」两种形态同属 PDF 1.1 通道，字典形态**不**需要更高
/// 版本，故与 [`PdfBuilder::legacy_named_destination`] 共用 1.1 版本门。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::core::builder) enum LegacyDestinationForm {
    /// Table 151 的目标数组：`/name [ page_ref /Fit … ]`。
    Array,
    /// 字典形态：`/name << /D [ page_ref /Fit … ] >>`（NOTE 2 的附加属性载体）。
    Dict,
}

/// 旧式条目的名字与目标（形态见 [`LegacyDestinationForm`]）。
pub(in crate::core::builder) type LegacyDestinationEntry =
    (String, Destination, LegacyDestinationForm);

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
    /// 条目值是 Table 151 的**数组形态**；字典形态（`<< /D … >>`）见
    /// [`Self::legacy_named_destination_dict`]，两者写进**同一个** `/Dests`
    /// 字典，故同名跨形态同样判重名。
    ///
    /// 与 [`Self::named_destination`]（1.2+ 名称树通道）互不干涉：同一名字
    /// 在两条通道各注册一次不会被判为重名（12.3.2.3 NOTE 3，见该方法文档）。
    pub fn legacy_named_destination(
        &mut self,
        name: impl Into<String>,
        destination: Destination,
    ) -> &mut Self {
        self.navigation.legacy_named_destinations.push((
            name.into(),
            destination,
            LegacyDestinationForm::Array,
        ));
        self
    }

    /// 注册一个**旧式**命名目标的**字典形态**条目（12.3.2.3，PDF 1.1），写入
    /// 目录 `/Dests` 字典为 `<< /D <Table 151 目标数组> >>`。
    ///
    /// 规范同一段落并列两种形态（原文见 `LegacyDestinationForm` 的文档），
    /// NOTE 2：「The latter form allows additional attributes to be associated
    /// with the destination, as well as enabling a go-to action (…) that shall
    /// be used as the target of a named destination.」本库只写 `/D` 条目，
    /// 附加属性与 `/A` 目标动作尚未建模。
    ///
    /// 与 [`Self::legacy_named_destination`] 共用一条通道，故校验、错误与版本
    /// 门完全一致（字典形态不需要更高版本）：名字空/含 NUL/非可打印 ASCII、
    /// 在本通道内重名——**含与数组形态同名**——以及页码越界、非有限操作数均
    /// 返回 [`WriteError::InvalidObject`]，低于 PDF 1.1 返回
    /// [`WriteError::UnsupportedVersion`]，两类失败均零字节输出。
    ///
    /// 与名称树通道（[`Self::named_destination`]）同名共存不判重名，读端按
    /// 名称树优先解析。
    pub fn legacy_named_destination_dict(
        &mut self,
        name: impl Into<String>,
        destination: Destination,
    ) -> &mut Self {
        self.navigation.legacy_named_destinations.push((
            name.into(),
            destination,
            LegacyDestinationForm::Dict,
        ));
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
/// 值按条目的 [`LegacyDestinationForm`] 分两形态：数组形态复用
/// [`destination_value`]（与名称树通道共享同一套 Table 151 装配），字典形态
/// 把同一个数组包进 `<< /D … >>`。两种形态混排进同一个字典、按同一套键序
/// 写出，因此重名校验也必须跨形态做一次（见 `validate::legacy_named_destinations`）。
pub(in crate::core::builder) fn legacy_dests_dict(
    entries: &[LegacyDestinationEntry],
    page_refs: &[(i64, i64)],
) -> Result<Value, WriteError> {
    let mut entries = entries.to_vec();
    entries.sort_by_cached_key(|(name, _, _)| name.as_bytes().to_vec());
    let mut dict = Dict::new();
    for (name, destination, form) in &entries {
        let array = destination_value(destination, page_refs)?;
        let value = match form {
            LegacyDestinationForm::Array => array,
            LegacyDestinationForm::Dict => Value::Dict(Dict::new().insert("D", array)),
        };
        dict = dict.insert(name.clone(), value);
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
