//! 扩展字典读取（ISO 32000-1 7.12、目录 `/Extensions` 与 Table 50）。
//!
//! 扩展字典是目录上的可选条目（Table 3 标注 “Optional; ISO 32000”，即
//! 随 ISO 32000-1/PDF 1.7 引入）：键为 Annex E 注册的开发者前缀，值为
//! 开发者扩展字典（`/BaseVersion` 名字必填、`/ExtensionLevel` 整数必填、
//! `/Type` 可选且必为 `DeveloperExtensions`）。规范要求全部嵌套为直接
//! 对象；读端按宽容策略仍解引用间接引用（库策略）。外层字典的 `/Type`
//! 可选，出现时必为 `Extensions`。7.12.1 规定扩展字典如存在 shall
//! 至少含一个开发者条目，空字典报 `Malformed`。未建模的其它条目通过
//! 保留的原始字典取回。注册前缀的字符合法性与键大小写一律原样保留，
//! 属库策略。

use std::collections::BTreeMap;

use crate::core::types::{Dict, Document, PdfVersion, Value};

use super::{NavigationError, catalog, malformed, resolve};

/// 单个开发者前缀的扩展字典（Table 50）。
#[derive(Debug, Clone, PartialEq)]
pub struct DeveloperExtension {
    /// `/BaseVersion` 原始名字（如 `1.7`）；用 [`Self::base_version_parsed`]
    /// 按目录 `/Version` 语法解析为 [`PdfVersion`]。
    pub base_version: String,
    /// `/ExtensionLevel` 整数值。
    pub extension_level: i64,
    /// `/Type`，出现时必为 `DeveloperExtensions`（缺失为 `None`）。
    pub type_name: Option<String>,
    /// 原始开发者扩展字典（保留未建模条目）。
    pub dict: Dict,
}

impl DeveloperExtension {
    /// 按 7.12.3 的版本名语法解析 `/BaseVersion`；不可解析返回 `None`。
    pub fn base_version_parsed(&self) -> Option<PdfVersion> {
        PdfVersion::from_name(&self.base_version)
    }
}

/// 目录 `/Extensions` 扩展字典（7.12.1）。
#[derive(Debug, Clone, PartialEq)]
pub struct ExtensionsDictionary {
    /// 外层字典 `/Type`，出现时必为 `Extensions`（缺失为 `None`）。
    pub type_name: Option<String>,
    /// 前缀 → 开发者扩展字典，按键排序。
    pub prefixes: BTreeMap<String, DeveloperExtension>,
}

impl Document {
    /// 读取目录 `/Extensions` 扩展字典（7.12、Table 50）。
    ///
    /// 目录无该条目时返回 `Ok(None)`；`/Extensions` 不是字典、为空字典
    /// （7.12.1：如存在 shall 含一个或多个条目）、`/Type` 值不符、
    /// 前缀值不是字典或缺少必填的 `/BaseVersion`（名字）与
    /// `/ExtensionLevel`（整数）均报 [`NavigationError::Malformed`]。
    /// 规范要求条目全部为直接对象，读端对间接引用宽容解引用（库策略）。
    pub fn extensions(&self) -> Result<Option<ExtensionsDictionary>, NavigationError> {
        let catalog = catalog(self)?;
        let Some(value) = catalog.get("Extensions") else {
            return Ok(None);
        };
        let value = resolve(self, value)?;
        let dict = value
            .as_dict()
            .ok_or_else(|| malformed("catalog /Extensions is not a dictionary"))?;
        let type_name = match dict.get("Type") {
            None => None,
            Some(value) => {
                let name = value
                    .as_name()
                    .ok_or_else(|| malformed("extensions dictionary /Type is not a name"))?;
                if name != "Extensions" {
                    return Err(malformed(format!(
                        "extensions dictionary /Type must be Extensions, got /{name}"
                    )));
                }
                Some(name.to_owned())
            }
        };
        let mut prefixes = BTreeMap::new();
        for (prefix, value) in &dict.0 {
            if prefix == "Type" {
                continue;
            }
            let value = resolve(self, value)?;
            let inner = value
                .as_dict()
                .ok_or_else(|| {
                    malformed(format!(
                        "extensions dictionary /{prefix} is not a dictionary"
                    ))
                })?
                .clone();
            prefixes.insert(prefix.clone(), parse_developer_extension(prefix, &inner)?);
        }
        if prefixes.is_empty() {
            return Err(malformed(
                "extensions dictionary must contain at least one developer entry (7.12.1)",
            ));
        }
        Ok(Some(ExtensionsDictionary {
            type_name,
            prefixes,
        }))
    }
}

/// Table 50：单个开发者扩展字典。
fn parse_developer_extension(
    prefix: &str,
    dict: &Dict,
) -> Result<DeveloperExtension, NavigationError> {
    let context = format!("extensions dictionary /{prefix}");
    let type_name = match dict.get("Type") {
        None => None,
        Some(value) => {
            let name = value
                .as_name()
                .ok_or_else(|| malformed(format!("{context} /Type is not a name")))?;
            if name != "DeveloperExtensions" {
                return Err(malformed(format!(
                    "{context} /Type must be DeveloperExtensions, got /{name}"
                )));
            }
            Some(name.to_owned())
        }
    };
    let base_version = dict
        .get("BaseVersion")
        .ok_or_else(|| malformed(format!("{context} is missing /BaseVersion")))?;
    let base_version = base_version
        .as_name()
        .ok_or_else(|| malformed(format!("{context} /BaseVersion is not a name")))?
        .to_owned();
    let extension_level = match dict.get("ExtensionLevel") {
        Some(Value::Integer(level)) => *level,
        Some(_) => {
            return Err(malformed(format!(
                "{context} /ExtensionLevel is not an integer"
            )));
        }
        None => {
            return Err(malformed(format!("{context} is missing /ExtensionLevel")));
        }
    };
    Ok(DeveloperExtension {
        base_version,
        extension_level,
        type_name,
        dict: dict.clone(),
    })
}

#[cfg(test)]
mod tests;
