//! 嵌入文件（7.11.3–7.11.4）构建态：文件规格条目 + 嵌入文件流字节。
//!
//! 一次 [`EmbeddedFile`] 注册在 `build` 时展开为一对间接对象：文件规格
//! 字典（Table 44，`/F` `/UF` `/Desc` `/EF`）与嵌入文件流（Table 45，
//! `/Subtype` MIME 名 + `/Params` 文件参数），并登记进目录
//! `/Names /EmbeddedFiles` 名称树；句柄可供 FileAttachment 注解的 `/FS`
//! 引用（12.5.6.14）。
//!
//! [`CollectionItem`]（7.11.6 Table 48/49）经 `/CI` 写出为第三个间接
//! 对象（Table 44 要求间接引用），用于便携集合的条目数据。

use std::collections::BTreeMap;

use md5::{Digest, Md5};

use super::{text_string_value, validate_name_value};
use crate::core::types::{Dict, Value};

/// 嵌入文件：名称、说明、MIME 子类型与文件字节。
#[derive(Debug, Clone, PartialEq)]
pub struct EmbeddedFile {
    name: String,
    short_name: Option<String>,
    description: Option<String>,
    subtype: Option<String>,
    creation_date: Option<String>,
    modification_date: Option<String>,
    data: Vec<u8>,
    collection_item: Option<CollectionItem>,
}

impl EmbeddedFile {
    /// 新建嵌入文件；`name` 为展示名（≥1.7 写出为 `/UF`，ASCII 时兼作 `/F`），
    /// `data` 为文件原始字节。
    pub fn new(name: impl Into<String>, data: impl Into<Vec<u8>>) -> Self {
        Self {
            name: name.into(),
            short_name: None,
            description: None,
            subtype: None,
            creation_date: None,
            modification_date: None,
            data: data.into(),
            collection_item: None,
        }
    }

    /// 覆盖 `/F` 7 位 ASCII 短名；非 ASCII 展示名必须显式提供。
    pub fn with_short_name(mut self, short_name: impl Into<String>) -> Self {
        self.short_name = Some(short_name.into());
        self
    }

    /// `/Desc` 说明文本串（PDF 1.6）。
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// 嵌入文件流 `/Subtype` 的 MIME 类型名，如 `application/pdf`
    /// （`/` 由写出层转义为 `#2F`）。
    pub fn with_subtype(mut self, subtype: impl Into<String>) -> Self {
        self.subtype = Some(subtype.into());
        self
    }

    /// `/Params` `/CreationDate` 日期文本串（如 `D:20261028050000Z`）。
    pub fn with_creation_date(mut self, date: impl Into<String>) -> Self {
        self.creation_date = Some(date.into());
        self
    }

    /// `/Params` `/ModDate` 日期文本串。
    pub fn with_modification_date(mut self, date: impl Into<String>) -> Self {
        self.modification_date = Some(date.into());
        self
    }

    /// 文件规格 `/CI` 集合项字典（PDF 1.7，7.11.6）；写出为间接对象。
    pub fn with_collection_item(mut self, item: CollectionItem) -> Self {
        self.collection_item = Some(item);
        self
    }

    /// 展示名（名称树键与 `/UF`）。
    pub fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    pub(crate) fn subtype(&self) -> Option<&str> {
        self.subtype.as_deref()
    }

    pub(crate) fn creation_date(&self) -> Option<&str> {
        self.creation_date.as_deref()
    }

    pub(crate) fn modification_date(&self) -> Option<&str> {
        self.modification_date.as_deref()
    }

    pub(crate) fn data(&self) -> &[u8] {
        &self.data
    }

    pub(crate) fn collection_item(&self) -> Option<&CollectionItem> {
        self.collection_item.as_ref()
    }

    /// 是否只有 ASCII 展示名（`/UF` 仅 ≥1.7 写出，低版本可省）。
    pub(crate) fn name_is_ascii(&self) -> bool {
        self.name.is_ascii()
    }

    /// `/F` 短名：显式覆盖优先，否则取 ASCII 展示名。
    pub(crate) fn short_name(&self) -> Option<&str> {
        self.short_name
            .as_deref()
            .or(self.name.is_ascii().then_some(self.name.as_str()))
    }

    /// 7.11.4.3：`/CheckSum` 为去掉数据中 ASCII 空白字符（NUL、TAB、LF、
    /// FF、CR、SPACE）后的 MD5 摘要。
    pub(crate) fn checksum(&self) -> [u8; 16] {
        let filtered: Vec<u8> = self
            .data
            .iter()
            .copied()
            .filter(|byte| !matches!(byte, 0 | 0o11 | 0o12 | 0o14 | 0o15 | b' '))
            .collect();
        Md5::digest(&filtered).into()
    }

    /// 校验名称与配置；错误串直接用于
    /// [`WriteError`](crate::core::writer::WriteError)。
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.name.is_empty() {
            return Err("embedded file name must not be empty".into());
        }
        if self.name.contains('/') || self.name.contains('\\') {
            return Err(format!(
                "embedded file name must not contain path separators, got {:?}",
                self.name
            ));
        }
        if let Some(short) = &self.short_name {
            if short.is_empty() {
                return Err("embedded file /F short name must not be empty".into());
            }
            if !short.is_ascii() {
                return Err("embedded file /F short name must be pure ASCII".into());
            }
            if short.contains('/') || short.contains('\\') {
                return Err("embedded file /F short name must not contain path separators".into());
            }
        } else if !self.name.is_ascii() {
            return Err(format!(
                "non-ASCII embedded file name {:?} requires an explicit ASCII short name",
                self.name
            ));
        }
        if self.subtype.as_deref().is_some_and(str::is_empty) {
            return Err("embedded file /Subtype must not be empty".into());
        }
        for (key, date) in [
            ("/CreationDate", &self.creation_date),
            ("/ModDate", &self.modification_date),
        ] {
            if date.as_deref().is_some_and(str::is_empty) {
                return Err(format!("embedded file /Params {key} must not be empty"));
            }
        }
        if let Some(item) = &self.collection_item {
            item.validate()?;
        }
        Ok(())
    }
}

/// 集合项字典（7.11.6 Table 48）：集合字段键 → 数据值。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CollectionItem {
    values: BTreeMap<String, CollectionItemValue>,
}

impl CollectionItem {
    /// 空集合项（只写 `/Type /CollectionItem`）。
    pub fn new() -> Self {
        Self::default()
    }

    /// 文本串条目（`S`/`D` 字段）。
    pub fn with_text(mut self, key: impl Into<String>, text: impl Into<String>) -> Self {
        self.values
            .insert(key.into(), CollectionItemValue::Text(text.into()));
        self
    }

    /// 数值条目（`N` 字段）；整数值写成整数对象。
    pub fn with_number(mut self, key: impl Into<String>, number: f64) -> Self {
        self.values
            .insert(key.into(), CollectionItemValue::Number(number));
        self
    }

    /// 集合子项字典条目（Table 49）。
    pub fn with_subitem(mut self, key: impl Into<String>, subitem: CollectionSubitem) -> Self {
        self.values
            .insert(key.into(), CollectionItemValue::Subitem(subitem));
        self
    }

    /// 已声明的条目，供写端对照集合模式交叉校验。
    pub(crate) fn values(&self) -> &BTreeMap<String, CollectionItemValue> {
        &self.values
    }

    /// 键须为合法 PDF 名字，数值须有限。
    fn validate(&self) -> Result<(), String> {
        for (key, value) in &self.values {
            validate_name_value(key, "collection item key")?;
            match value {
                CollectionItemValue::Number(number) if !number.is_finite() => {
                    return Err(format!("collection item {key:?} number must be finite"));
                }
                CollectionItemValue::Subitem(subitem) => subitem.validate()?,
                _ => {}
            }
        }
        Ok(())
    }

    pub(crate) fn to_dict(&self) -> Dict {
        let mut dict = Dict::new().insert("Type", Value::Name("CollectionItem".into()));
        for (key, value) in &self.values {
            let value = match value {
                CollectionItemValue::Text(text) => text_string_value(text),
                CollectionItemValue::Number(number) => number_value(*number),
                CollectionItemValue::Subitem(subitem) => Value::Dict(subitem.to_dict()),
            };
            dict = dict.insert(key.as_str(), value);
        }
        dict
    }
}

/// 集合项条目值（Table 48）。
#[derive(Debug, Clone, PartialEq)]
pub enum CollectionItemValue {
    /// PDF 文本串（日期字段同样写文本串）。
    Text(String),
    /// PDF 数值。
    Number(f64),
    /// 集合子项字典（Table 49）。
    Subitem(CollectionSubitem),
}

/// 集合子项字典（7.11.6 Table 49）：数据 `/D` 与呈现前缀 `/P`。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CollectionSubitem {
    datum: Option<CollectionSubitemValue>,
    prefix: Option<String>,
}

impl CollectionSubitem {
    /// 空子项（只写 `/Type /CollectionSubitem`）。
    pub fn new() -> Self {
        Self::default()
    }

    /// `/D` 文本串数据。
    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.datum = Some(CollectionSubitemValue::Text(text.into()));
        self
    }

    /// `/D` 数值数据。
    pub fn with_number(mut self, number: f64) -> Self {
        self.datum = Some(CollectionSubitemValue::Number(number));
        self
    }

    /// `/P` 呈现前缀文本串。
    pub fn with_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.prefix = Some(prefix.into());
        self
    }

    /// 已声明的 `/D` 数据，供写端对照集合模式交叉校验。
    pub(crate) fn datum(&self) -> Option<&CollectionSubitemValue> {
        self.datum.as_ref()
    }

    fn validate(&self) -> Result<(), String> {
        if let Some(CollectionSubitemValue::Number(number)) = &self.datum
            && !number.is_finite()
        {
            return Err("collection subitem /D number must be finite".into());
        }
        Ok(())
    }

    pub(crate) fn to_dict(&self) -> Dict {
        let mut dict = Dict::new().insert("Type", Value::Name("CollectionSubitem".into()));
        if let Some(datum) = &self.datum {
            let datum = match datum {
                CollectionSubitemValue::Text(text) => text_string_value(text),
                CollectionSubitemValue::Number(number) => number_value(*number),
            };
            dict = dict.insert("D", datum);
        }
        if let Some(prefix) = &self.prefix {
            dict = dict.insert("P", text_string_value(prefix));
        }
        dict
    }
}

/// 集合子项数据（Table 49 `/D`）。
#[derive(Debug, Clone, PartialEq)]
pub enum CollectionSubitemValue {
    /// PDF 文本串（日期字段同样写文本串）。
    Text(String),
    /// PDF 数值。
    Number(f64),
}

/// 整数写整数对象、其余写实数对象；调用前已校验有限。
fn number_value(number: f64) -> Value {
    if number.fract() == 0.0 && number >= i64::MIN as f64 && number <= i64::MAX as f64 {
        Value::Integer(number as i64)
    } else {
        Value::Float(number)
    }
}
