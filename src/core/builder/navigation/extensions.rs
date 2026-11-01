//! 扩展字典写端（ISO 32000-1 7.12、目录 `/Extensions` 与 Table 50）。
//!
//! 每个注册前缀写一个开发者扩展字典：`/Type /DeveloperExtensions`、
//! `/BaseVersion` 版本名与 `/ExtensionLevel` 正整数。规范要求全部内容
//! 为**直接对象**嵌套在目录里（7.12.1），因此不分配任何间接对象号。
//! PDF 1.7（ISO 32000-1）起可用。
//!
//! 超出规范的政策：7.12.1 只要求键为 Annex E 注册前缀、`/ExtensionLevel`
//! 为整数，注册状态无法在构建期核验；本库以「恰好 4 个 ASCII 字母数字、
//! 互不重复、级别为正整数」作为注册前缀形态校验，并在 `apply` 里对空
//! 条目列表不写出 `/Extensions`（规范禁止写出空扩展字典）。

use std::collections::BTreeSet;

use crate::core::types::{Dict, PdfVersion, Value};
use crate::core::writer::WriteError;

use super::invalid;

/// 构建期注册的单个扩展：前缀 + BaseVersion + ExtensionLevel。
pub(crate) type RegisteredExtension = (String, PdfVersion, i64);

/// 校验扩展条目；在目录改写之前调用，保持零字节输出契约。
///
/// 前缀须为 Annex E 的 4 字节注册前缀形态（恰好 4 个 ASCII 字母或数字）
/// 且互不重复；`/ExtensionLevel` 须为正整数（7.12.4：同一开发者的级别
/// 号随时间单调递增，非正值无法参与该序）；`/BaseVersion` 不得超过目标
/// 文档版本（7.12.3：BaseVersion 视作文版本号时须 ≤ 文件头与目录
/// `/Version` 的版本）。
pub(super) fn validate(
    extensions: &[RegisteredExtension],
    version: PdfVersion,
) -> Result<(), WriteError> {
    let mut prefixes = BTreeSet::new();
    for (prefix, base_version, level) in extensions {
        if prefix.is_empty() {
            return Err(invalid("extension prefix name is empty"));
        }
        if prefix.len() != 4 || !prefix.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
            return Err(invalid(format!(
                "extension prefix {prefix:?} is not a 4-byte alphanumeric registered prefix"
            )));
        }
        if !prefixes.insert(prefix.as_str()) {
            return Err(invalid(format!("duplicate extension prefix {prefix:?}")));
        }
        if *level < 1 {
            return Err(invalid(format!(
                "extension /ExtensionLevel for prefix {prefix:?} must be a positive integer, got {level}"
            )));
        }
        if *base_version > version {
            return Err(invalid(format!(
                "extension /BaseVersion {} for prefix {prefix:?} exceeds the document version {}",
                base_version.as_str(),
                version.as_str()
            )));
        }
    }
    Ok(())
}

/// 构造目录 `/Extensions` 字典：前缀即字典键（`Dict` 为 `BTreeMap`，
/// 写出按键排序），值为直接对象开发者扩展字典（Table 50）。
/// 调用方保证 `extensions` 非空且已校验。
pub(super) fn extensions_dict(extensions: &[RegisteredExtension]) -> Dict {
    let mut dict = Dict::new();
    for (prefix, base_version, level) in extensions {
        let inner = Dict::new()
            .insert("Type", Value::Name("DeveloperExtensions".into()))
            .insert("BaseVersion", Value::Name(base_version.as_str()))
            .insert("ExtensionLevel", Value::Integer(*level));
        dict = dict.insert(prefix.clone(), Value::Dict(inner));
    }
    dict
}
