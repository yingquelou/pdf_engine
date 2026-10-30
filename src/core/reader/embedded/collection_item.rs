//! 集合项字典读取（7.11.6 Table 48/49，PDF 1.7）。
//!
//! 文件规格字典的 `/CI`（Table 44）须为间接引用；条目键对应集合模式
//! （12.3.5 Table 156）的字段键，值为文本串、数值或集合子项字典。
//! 子项字典（Table 49）的 `/D` 为数据、`/P` 为呈现前缀。
//!
//! 读端不结合集合模式校验条目类型：文本与日期同为 PDF 文本串，
//! 类别判定属于调用方。

use std::collections::BTreeMap;

use crate::core::reader::content::text_value;
use crate::core::reader::navigation::resolve;
use crate::core::types::{Dict, Document, Value};

use super::{Result, malformed};

/// 集合项字典（7.11.6 Table 48）。
#[derive(Debug, Clone, PartialEq)]
pub struct CollectionItem {
    /// 字段键（不含 `/Type`）→ 类型化数据。
    pub values: BTreeMap<String, CollectionItemValue>,
    /// 完整集合项字典。
    pub dict: Dict,
}

/// 集合项条目值（Table 48）。
#[derive(Debug, Clone, PartialEq)]
pub enum CollectionItemValue {
    /// 文本串（`S` 与 `D` 字段；日期串按文本解码）。
    Text(String),
    /// 数值（`N` 字段）。
    Number(f64),
    /// 集合子项字典（Table 49）。
    Subitem(CollectionSubitem),
}

/// 集合子项字典（7.11.6 Table 49）。
#[derive(Debug, Clone, PartialEq)]
pub struct CollectionSubitem {
    /// `/D` 数据（文本串或数值）。
    pub datum: Option<CollectionSubitemValue>,
    /// `/P` 呈现前缀文本串。
    pub prefix: Option<String>,
    /// 完整子项字典。
    pub dict: Dict,
}

/// 集合子项数据（Table 49 `/D`）。
#[derive(Debug, Clone, PartialEq)]
pub enum CollectionSubitemValue {
    /// 文本串（含日期串）。
    Text(String),
    /// 数值。
    Number(f64),
}

/// 解析 `/CI`：须为间接引用，`/Type` 存在时须为 `CollectionItem`。
pub(super) fn parse_collection_item(document: &Document, value: &Value) -> Result<CollectionItem> {
    if value.as_ref().is_none() {
        return Err(malformed(
            "/Filespec /CI must be an indirect reference (Table 44)",
        ));
    }
    let resolved = resolve(document, value).map_err(|error| malformed(error.to_string()))?;
    let dict = resolved
        .as_dict()
        .ok_or_else(|| malformed("/Filespec /CI is not a dictionary"))?;
    check_type(dict, "CollectionItem", "collection item")?;
    let mut values = BTreeMap::new();
    for (key, entry) in &dict.0 {
        if key == "Type" {
            continue;
        }
        values.insert(key.clone(), parse_item_value(document, entry)?);
    }
    Ok(CollectionItem {
        values,
        dict: dict.clone(),
    })
}

fn parse_item_value(document: &Document, entry: &Value) -> Result<CollectionItemValue> {
    match entry {
        Value::String(_) | Value::HexString(_) => text_value(entry)
            .map(CollectionItemValue::Text)
            .ok_or_else(|| malformed("/CI entry is not a text string")),
        Value::Integer(number) => Ok(CollectionItemValue::Number(*number as f64)),
        Value::Float(number) if number.is_finite() => Ok(CollectionItemValue::Number(*number)),
        Value::Dict(_) | Value::Ref(_, _) => {
            let resolved =
                resolve(document, entry).map_err(|error| malformed(error.to_string()))?;
            let dict = resolved.as_dict().ok_or_else(|| {
                malformed("/CI entry dictionary is not a collection subitem dictionary")
            })?;
            Ok(CollectionItemValue::Subitem(parse_subitem(dict)?))
        }
        other => Err(malformed(format!(
            "/CI entry must be a text string, number or collection subitem dictionary, got {}",
            other.type_name()
        ))),
    }
}

fn parse_subitem(dict: &Dict) -> Result<CollectionSubitem> {
    check_type(dict, "CollectionSubitem", "collection subitem")?;
    let datum = match dict.get("D") {
        None => None,
        Some(value) => Some(parse_datum(value)?),
    };
    let prefix = match dict.get("P") {
        None => None,
        Some(value) => Some(
            text_value(value)
                .ok_or_else(|| malformed("collection subitem /P is not a text string"))?,
        ),
    };
    Ok(CollectionSubitem {
        datum,
        prefix,
        dict: dict.clone(),
    })
}

fn parse_datum(value: &Value) -> Result<CollectionSubitemValue> {
    match value {
        Value::String(_) | Value::HexString(_) => text_value(value)
            .map(CollectionSubitemValue::Text)
            .ok_or_else(|| malformed("collection subitem /D is not a text string")),
        Value::Integer(number) => Ok(CollectionSubitemValue::Number(*number as f64)),
        Value::Float(number) if number.is_finite() => Ok(CollectionSubitemValue::Number(*number)),
        other => Err(malformed(format!(
            "collection subitem /D must be a text string or number, got {}",
            other.type_name()
        ))),
    }
}

/// `/Type` 缺省可省，存在时须匹配期望名字。
fn check_type(dict: &Dict, expected: &str, context: &str) -> Result<()> {
    match dict.get("Type") {
        None => Ok(()),
        Some(value) if value.as_name() == Some(expected) => Ok(()),
        Some(_) => Err(malformed(format!("{context} /Type must be {expected}"))),
    }
}
