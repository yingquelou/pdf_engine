//! 便携集合读取（ISO 32000-1 12.3.5、Table 155–158）。
//!
//! 集合字典挂在文档目录 `/Collection` 上，描述嵌入式文件集合的展示与
//! 组织方式：字段模式（Table 156/157）、初始文档（`/D`）、初始视图
//! （`/View`）与排序（Table 158）。集合项 `/CI`（7.11.6）经
//! [`Document::collection_items`] 与字段模式交叉校验。

use std::collections::BTreeMap;

use crate::core::reader::content::text_value;
use crate::core::reader::embedded::{CollectionItem, CollectionItemValue, CollectionSubitemValue};
use crate::core::reader::page_content::decode_name_escapes;
use crate::core::types::{Dict, Document, Value};

use super::{NavigationError, catalog, malformed, resolve};

/// 集合字段的数据类型（Table 157 `/Subtype`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CollectionFieldType {
    /// `S`：PDF 文本串。
    Text,
    /// `D`：PDF 日期串。
    Date,
    /// `N`：PDF 数值。
    Number,
    /// `F`：嵌入文件流文件名（`/UF` 优先，其次 `/F`）。
    FileName,
    /// `Desc`：文件规格字典的描述。
    Description,
    /// `ModDate`：嵌入文件参数字典的修改日期。
    ModificationDate,
    /// `CreationDate`：嵌入文件参数字典的创建日期。
    CreationDate,
    /// `Size`：嵌入文件参数字典的字节数。
    Size,
    /// 未在 Table 157 中定义的名字，保留原文。
    Other(String),
}

/// 集合字段字典（Table 157）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionField {
    /// 字段键（模式字典中的键，供集合项 `/CI` 与排序引用）。
    pub key: String,
    /// `/Subtype` 要求的数据类型。
    pub field_type: CollectionFieldType,
    /// `/N` 呈现给用户的字段名（文本串）。
    pub name: String,
    /// `/O` 用户界面中的相对次序。
    pub order: Option<i64>,
    /// `/V` 字段是否初始可见；缺省 `true` 不物化。
    pub visible: Option<bool>,
    /// `/E` 阅读器是否应提供编辑；缺省 `false` 不物化。
    pub editable: Option<bool>,
}

/// 集合初始视图（Table 155 `/View`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectionView {
    /// `D`：多列详情模式（缺省）。
    Details,
    /// `T`：图标平铺模式。
    Tiles,
    /// `H`：初始隐藏集合视图。
    Hidden,
}

/// 集合排序字典（Table 158）。
///
/// `/A` 按 `/S` 的位置逐项归一化：布尔值对全部字段生效、数组缺项按
/// `true` 补齐、多余项忽略；`ascending` 长度恒等于 `fields`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollectionSort {
    /// `/S` 排序字段名，至少一项。
    pub fields: Vec<String>,
    /// `/A` 每个字段的升序标志。
    pub ascending: Vec<bool>,
}

/// 文档目录 `/Collection` 的读取结果（12.3.5）。
#[derive(Debug, Clone, PartialEq)]
pub struct Collection {
    /// `/Schema` 字段模式；`None` 表示字典缺省，阅读器可自行选择默认字段。
    pub schema: Option<BTreeMap<String, CollectionField>>,
    /// `/D` 初始展示文档在 EmbeddedFiles 名称树中的键，原始字节。
    pub initial_document: Option<Vec<u8>>,
    /// `/View` 初始视图；缺省 `D` 物化为 [`CollectionView::Details`]。
    pub view: CollectionView,
    /// `/Sort` 初始排序；缺省不排序。
    pub sort: Option<CollectionSort>,
}

impl Document {
    /// 读取便携集合（12.3.5）；目录无 `/Collection` 时返回 `None`。
    pub fn collection(&self) -> Result<Option<Collection>, NavigationError> {
        read_collection(self)
    }

    /// 读取并校验集合项（7.11.6 Table 48/49），按键排序。
    ///
    /// `/CI` 的键须在集合模式（12.3.5 Table 157）中声明，值形态须与字段
    /// `/Subtype` 一致；无集合或无模式时 `/CI` 无参照可依，返回类型化错误。
    pub fn collection_items(&self) -> Result<BTreeMap<String, CollectionItem>, NavigationError> {
        let collection = self.collection()?;
        let files = self
            .embedded_files()
            .map_err(|error| malformed(error.to_string()))?;
        let Some(collection) = collection else {
            if let Some((name, _)) = files
                .iter()
                .find(|(_, filespec)| filespec.collection_item.is_some())
            {
                return Err(malformed(format!(
                    "embedded file {name:?} has a collection item but no collection is set"
                )));
            }
            return Ok(BTreeMap::new());
        };
        if collection.schema.is_none()
            && files
                .values()
                .any(|filespec| filespec.collection_item.is_some())
        {
            return Err(malformed("collection items require a collection schema"));
        }
        let mut items = BTreeMap::new();
        for (name, filespec) in &files {
            let Some(item) = &filespec.collection_item else {
                continue;
            };
            if let Some(schema) = &collection.schema {
                validate_collection_item(name, item, schema)?;
            }
            items.insert(name.clone(), item.clone());
        }
        Ok(items)
    }
}

fn read_collection(document: &Document) -> Result<Option<Collection>, NavigationError> {
    let catalog = catalog(document)?;
    let Some(value) = catalog.get("Collection") else {
        return Ok(None);
    };
    let dict = resolve(document, value)?;
    let dict = dict
        .as_dict()
        .ok_or_else(|| malformed("catalog /Collection is not a dictionary"))?;
    if let Some(kind) = dict.get("Type")
        && kind.as_name() != Some("Collection")
    {
        return Err(malformed("collection /Type is not Collection"));
    }
    let schema = match dict.get("Schema") {
        Some(value) => Some(read_schema(document, value)?),
        None => None,
    };
    let initial_document = match dict.get("D") {
        Some(value) => Some(
            byte_string(value).ok_or_else(|| malformed("collection /D is not a byte string"))?,
        ),
        None => None,
    };
    let view = match dict.get("View") {
        Some(Value::Name(name)) => match name.as_str() {
            "D" => CollectionView::Details,
            "T" => CollectionView::Tiles,
            "H" => CollectionView::Hidden,
            other => {
                return Err(malformed(format!(
                    "collection /View has unknown value /{other}"
                )));
            }
        },
        Some(_) => return Err(malformed("collection /View is not a name")),
        None => CollectionView::Details,
    };
    let sort = match dict.get("Sort") {
        Some(value) => Some(read_sort(document, value, schema.as_ref())?),
        None => None,
    };
    Ok(Some(Collection {
        schema,
        initial_document,
        view,
        sort,
    }))
}

fn read_schema(
    document: &Document,
    value: &Value,
) -> Result<BTreeMap<String, CollectionField>, NavigationError> {
    let dict = resolve(document, value)?;
    let dict = dict
        .as_dict()
        .ok_or_else(|| malformed("collection /Schema is not a dictionary"))?;
    if let Some(kind) = dict.get("Type")
        && kind.as_name() != Some("CollectionSchema")
    {
        return Err(malformed("collection schema /Type is not CollectionSchema"));
    }
    let mut fields = BTreeMap::new();
    for (raw_key, value) in &dict.0 {
        if raw_key == "Type" {
            continue;
        }
        let key = decode_name_escapes(raw_key);
        let field = read_field(document, &key, value)?;
        if fields.insert(key.clone(), field).is_some() {
            return Err(malformed(format!(
                "collection schema declares field {key:?} twice"
            )));
        }
    }
    Ok(fields)
}

fn read_field(
    document: &Document,
    key: &str,
    value: &Value,
) -> Result<CollectionField, NavigationError> {
    let dict = resolve(document, value)?;
    let dict = dict
        .as_dict()
        .ok_or_else(|| malformed(format!("collection field {key:?} is not a dictionary")))?;
    if let Some(kind) = dict.get("Type")
        && kind.as_name() != Some("CollectionField")
    {
        return Err(malformed(format!(
            "collection field {key:?} /Type is not CollectionField"
        )));
    }
    let subtype = dict
        .get("Subtype")
        .and_then(Value::as_name)
        .ok_or_else(|| malformed(format!("collection field {key:?} has no /Subtype name")))?;
    let field_type = match subtype {
        "S" => CollectionFieldType::Text,
        "D" => CollectionFieldType::Date,
        "N" => CollectionFieldType::Number,
        "F" => CollectionFieldType::FileName,
        "Desc" => CollectionFieldType::Description,
        "ModDate" => CollectionFieldType::ModificationDate,
        "CreationDate" => CollectionFieldType::CreationDate,
        "Size" => CollectionFieldType::Size,
        other => CollectionFieldType::Other(other.into()),
    };
    let name = dict
        .get("N")
        .and_then(text_value)
        .ok_or_else(|| malformed(format!("collection field {key:?} has no /N text string")))?;
    let order =
        match dict.get("O") {
            Some(value) => Some(value.as_integer().ok_or_else(|| {
                malformed(format!("collection field {key:?} /O is not an integer"))
            })?),
            None => None,
        };
    let visible = optional_bool(dict, "V", key)?;
    let editable = optional_bool(dict, "E", key)?;
    Ok(CollectionField {
        key: key.into(),
        field_type,
        name,
        order,
        visible,
        editable,
    })
}

fn optional_bool(dict: &Dict, entry: &str, key: &str) -> Result<Option<bool>, NavigationError> {
    match dict.get(entry) {
        Some(Value::Bool(value)) => Ok(Some(*value)),
        Some(_) => Err(malformed(format!(
            "collection field {key:?} /{entry} is not a boolean"
        ))),
        None => Ok(None),
    }
}

fn read_sort(
    document: &Document,
    value: &Value,
    schema: Option<&BTreeMap<String, CollectionField>>,
) -> Result<CollectionSort, NavigationError> {
    let dict = resolve(document, value)?;
    let dict = dict
        .as_dict()
        .ok_or_else(|| malformed("collection /Sort is not a dictionary"))?;
    if let Some(kind) = dict.get("Type")
        && kind.as_name() != Some("CollectionSort")
    {
        return Err(malformed("collection sort /Type is not CollectionSort"));
    }
    let fields = match dict.get("S") {
        Some(Value::Name(name)) => vec![name.clone()],
        Some(Value::Array(items)) => {
            if items.is_empty() {
                return Err(malformed("collection sort /S array is empty"));
            }
            items
                .iter()
                .map(|item| {
                    item.as_name()
                        .map(str::to_owned)
                        .ok_or_else(|| malformed("collection sort /S array item is not a name"))
                })
                .collect::<Result<Vec<_>, _>>()?
        }
        Some(_) => return Err(malformed("collection sort /S is not a name or array")),
        None => return Err(malformed("collection sort has no /S entry")),
    };
    let ascending = match dict.get("A") {
        Some(Value::Bool(value)) => vec![*value; fields.len()],
        Some(Value::Array(items)) => fields
            .iter()
            .enumerate()
            .map(|(index, _)| match items.0.get(index) {
                Some(Value::Bool(value)) => Ok(*value),
                Some(_) => Err(malformed("collection sort /A array item is not a boolean")),
                None => Ok(true),
            })
            .collect::<Result<Vec<_>, _>>()?,
        Some(_) => return Err(malformed("collection sort /A is not a boolean or array")),
        None => vec![true; fields.len()],
    };
    if let Some(schema) = schema {
        for field in &fields {
            if !schema.contains_key(field) {
                return Err(malformed(format!(
                    "collection sort field {field:?} is not declared in the schema"
                )));
            }
        }
    }
    Ok(CollectionSort { fields, ascending })
}

/// `/CI` 条目值的形态，用于对照 Table 157 `/Subtype`。
#[derive(Clone, Copy, PartialEq, Eq)]
enum ItemShape {
    Text,
    Number,
}

impl ItemShape {
    const fn describe(self) -> &'static str {
        match self {
            Self::Text => "a text string",
            Self::Number => "a number",
        }
    }
}

/// Table 157 `/Subtype` → `/CI` 值形态：`N`/`Size` 为数，其余为文本串；
/// 未定义的名字（`Other`）不限定形态。
fn expected_shape(field_type: &CollectionFieldType) -> Option<ItemShape> {
    match field_type {
        CollectionFieldType::Number | CollectionFieldType::Size => Some(ItemShape::Number),
        CollectionFieldType::Text
        | CollectionFieldType::Date
        | CollectionFieldType::FileName
        | CollectionFieldType::Description
        | CollectionFieldType::ModificationDate
        | CollectionFieldType::CreationDate => Some(ItemShape::Text),
        CollectionFieldType::Other(_) => None,
    }
}

/// `/CI` 键须在模式中声明，值形态须与字段 `/Subtype` 一致；
/// 子项 `/D` 按同一形态校验，缺 `/D` 时无从比较。
fn validate_collection_item(
    name: &str,
    item: &CollectionItem,
    schema: &BTreeMap<String, CollectionField>,
) -> Result<(), NavigationError> {
    for (key, value) in &item.values {
        let Some(field) = schema.get(key) else {
            return Err(malformed(format!(
                "collection item field {key:?} of embedded file {name:?} is not declared in the collection schema"
            )));
        };
        let Some(expected) = expected_shape(&field.field_type) else {
            continue;
        };
        let actual = match value {
            CollectionItemValue::Text(_) => ItemShape::Text,
            CollectionItemValue::Number(_) => ItemShape::Number,
            CollectionItemValue::Subitem(subitem) => match &subitem.datum {
                Some(CollectionSubitemValue::Text(_)) => ItemShape::Text,
                Some(CollectionSubitemValue::Number(_)) => ItemShape::Number,
                None => continue,
            },
        };
        if actual != expected {
            return Err(malformed(format!(
                "collection item field {key:?} of embedded file {name:?} expects {}, got {}",
                expected.describe(),
                actual.describe()
            )));
        }
    }
    Ok(())
}

/// PDF 字节串（字面量或十六进制）；字面量按原始字节逐字符存储。
fn byte_string(value: &Value) -> Option<Vec<u8>> {
    match value {
        Value::String(text) => text
            .chars()
            .map(|ch| u8::try_from(u32::from(ch)).ok())
            .collect(),
        Value::HexString(hex) => {
            let mut bytes = Vec::with_capacity(hex.len() / 2);
            let mut high = None;
            for ch in hex.chars() {
                let digit = ch.to_digit(16)?;
                match high.take() {
                    Some(high) => bytes.push((high << 4) | digit as u8),
                    None => high = Some(digit as u8),
                }
            }
            if let Some(high) = high {
                bytes.push(high << 4);
            }
            Some(bytes)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests;
