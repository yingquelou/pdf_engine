//! 透明组字典（`/Group`）的类型化读端模型（ISO 32000-1 §11.6.4）。
//!
//! [`Document::group`] 把组字典解析为 [`TransparencyGroup`]：`/S` 必须
//! 是 `/Transparency` 或已废弃的 `/Form`；`/CS` 保留原始值并经
//! [`Document::color_space`] 类型化；`/I` 省略即 `false`；`/K` 按规范
//! 对透明组必需，但此处宽松保留为 `Option`；其余条目（含 `/B` 边界
//! 数组）不解释语义，原样进入 `other`。
//! [`Document::page_group`] 解析页面字典的 `/Group`。

use std::collections::BTreeMap;
use std::fmt;

use crate::core::types::{Dict, Document, Value};

use super::ParseError;
use super::color_space::{ColorSpaceError, ColorSpaceModel};
use super::page_content::PageContentError;

#[cfg(test)]
mod tests;

type Result<T> = std::result::Result<T, GroupError>;

/// 组字典解析的失败原因。
#[derive(Debug)]
pub enum GroupError {
    /// 组引用的对象无法解析。
    Parse(ParseError),
    /// 页面定位失败。
    PageContent(PageContentError),
    /// 组字典本身不满足 Table 134。
    Invalid(String),
    /// `/CS` 颜色空间解析失败。
    ColorSpace(ColorSpaceError),
}

impl fmt::Display for GroupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(error) => write!(f, "{error}"),
            Self::PageContent(error) => write!(f, "{error}"),
            Self::Invalid(message) => write!(f, "invalid group: {message}"),
            Self::ColorSpace(error) => write!(f, "invalid group /CS: {error}"),
        }
    }
}

impl std::error::Error for GroupError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Parse(error) => Some(error),
            Self::PageContent(error) => Some(error),
            Self::Invalid(_) => None,
            Self::ColorSpace(error) => Some(error),
        }
    }
}

impl From<ParseError> for GroupError {
    fn from(error: ParseError) -> Self {
        Self::Parse(error)
    }
}

impl From<PageContentError> for GroupError {
    fn from(error: PageContentError) -> Self {
        Self::PageContent(error)
    }
}

impl From<ColorSpaceError> for GroupError {
    fn from(error: ColorSpaceError) -> Self {
        Self::ColorSpace(error)
    }
}

fn invalid(message: impl Into<String>) -> GroupError {
    GroupError::Invalid(message.into())
}

/// 组 subtype（`/S`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupSubtype {
    /// `/Transparency`：透明组。
    Transparency,
    /// `/Form`：已废弃的普通组（PDF 1.3）。
    Form,
}

/// 解析后的组字典（Table 134）。
#[derive(Debug, Clone, PartialEq)]
pub struct TransparencyGroup {
    /// `/S` subtype。
    pub subtype: GroupSubtype,
    /// `/CS` 类型化颜色空间；解析自原始值。
    pub color_space: Option<ColorSpaceModel>,
    /// `/CS` 原始值（间接引用已解一层），供调用方透传。
    pub color_space_raw: Option<Value>,
    /// `/I` isolated；省略即 `false`。
    pub isolated: bool,
    /// `/K` knockout；规范对透明组必需，此处宽松为 `Option`。
    pub knockout: Option<bool>,
    /// 不建模的其余条目（含 `/B` 边界数组），原值保留。
    pub other: BTreeMap<String, Value>,
}

impl Document {
    /// 解析一个组字典值。
    ///
    /// `value` 可为字典或指向字典的间接引用。
    pub fn group(&self, value: &Value) -> Result<TransparencyGroup> {
        let resolved = self.resolve_group_value(value)?;
        let dict = resolved.as_dict().ok_or_else(|| {
            invalid(format!(
                "group must be a dictionary, got {}",
                resolved.type_name()
            ))
        })?;
        self.parse_group(dict)
    }

    /// 解析 `page_index`（0 基）页面的 `/Group`；页面无组时返回 `None`。
    pub fn page_group(&self, page_index: usize) -> Result<Option<TransparencyGroup>> {
        let page_value = self.locate_page_node(page_index)?;
        let page = page_value
            .as_dict()
            .ok_or_else(|| invalid("page node is not a dictionary"))?;
        let Some(group) = page.get("Group") else {
            return Ok(None);
        };
        self.group(group).map(Some)
    }

    pub(crate) fn resolve_group_value(&self, value: &Value) -> Result<Value> {
        match value {
            Value::Ref(num, generation) => {
                let object = self.get_object(*num)?.ok_or_else(|| {
                    invalid(format!("unresolved object reference {num} {generation} R"))
                })?;
                Ok(object.value)
            }
            _ => Ok(value.clone()),
        }
    }

    fn parse_group(&self, dict: &Dict) -> Result<TransparencyGroup> {
        let subtype = match dict.get("S") {
            None => return Err(invalid("/S is required")),
            Some(value) => match self.resolve_group_value(value)? {
                Value::Name(name) if name == "Transparency" => GroupSubtype::Transparency,
                Value::Name(name) if name == "Form" => GroupSubtype::Form,
                Value::Name(name) => {
                    return Err(invalid(format!(
                        "group /S must be /Transparency or /Form, got /{name}"
                    )));
                }
                _ => {
                    return Err(invalid("group /S must be a name"));
                }
            },
        };

        let color_space_raw = match dict.get("CS") {
            None => None,
            Some(value) => Some(self.resolve_group_value(value)?),
        };
        let color_space = match &color_space_raw {
            None => None,
            Some(raw) => Some(self.color_space(raw)?),
        };

        let isolated = match dict.get("I") {
            None => false,
            Some(value) => match self.resolve_group_value(value)? {
                Value::Bool(flag) => flag,
                _ => return Err(invalid("group /I must be a boolean")),
            },
        };
        let knockout = match dict.get("K") {
            None => None,
            Some(value) => match self.resolve_group_value(value)? {
                Value::Bool(flag) => Some(flag),
                _ => return Err(invalid("group /K must be a boolean")),
            },
        };

        const GROUP_ENTRIES: [&str; 5] = ["Type", "S", "CS", "I", "K"];
        let mut other = BTreeMap::new();
        for (key, value) in &dict.0 {
            if !GROUP_ENTRIES.contains(&key.as_str()) {
                other.insert(key.clone(), value.clone());
            }
        }

        Ok(TransparencyGroup {
            subtype,
            color_space,
            color_space_raw,
            isolated,
            knockout,
            other,
        })
    }
}
