//! 3D 流 `/Resources` 名称树（13.6.3.1 Table 300）。
//!
//! `/Resources` 是名称树，键必须为文本串（以便 JavaScript 访问）；值为
//! 应用或脚本可使用的任意对象，本层原样保留（可能是间接引用）。名称树
//! 遍历复用导航层的加固实现（深度、节点数与环限制），解码后的键排序返回。

use std::collections::BTreeMap;

use crate::core::reader::navigation::name_tree_entries;
use crate::core::types::{Document, PdfTextString, Value};

use super::{ThreeDError, malformed, resolve_value};

/// `/Resources` 条目数上限（防护不可信文件）。
const MAX_RESOURCES: usize = 4096;

/// 解析 `/Resources` 名称树；缺省返回 `None`，值不解引用。
pub(crate) fn parse_resources(
    document: &Document,
    value: Option<&Value>,
    context: &str,
) -> Result<Option<BTreeMap<String, Value>>, ThreeDError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let resolved = resolve_value(document, value)?;
    let Value::Dict(dict) = resolved else {
        return Err(malformed(format!(
            "{context} is not a name tree dictionary"
        )));
    };
    let entries =
        name_tree_entries(document, &dict).map_err(|error| malformed(error.to_string()))?;
    if entries.len() > MAX_RESOURCES {
        return Err(ThreeDError::LimitExceeded("3D stream /Resources entries"));
    }
    let mut resources = BTreeMap::new();
    for (raw_name, value) in entries {
        let name = decode_name(&raw_name)
            .ok_or_else(|| malformed(format!("{context} name is not a valid text string")))?;
        if resources.insert(name.clone(), value).is_some() {
            return Err(malformed(format!("{context} has duplicate name {name:?}")));
        }
    }
    Ok(Some(resources))
}

/// 遍历器返回的原始键（每字节一个 `char`）按 PDF 文本串解码。
fn decode_name(raw: &str) -> Option<String> {
    let bytes: Vec<u8> = raw
        .chars()
        .map(|ch| u8::try_from(ch as u32).ok())
        .collect::<Option<_>>()?;
    PdfTextString::decode(&bytes)
        .map(PdfTextString::into_string)
        .ok()
}
