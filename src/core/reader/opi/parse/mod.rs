//! OPI 版本字典与各条目的解析（Table 368–370）。

mod accessors;
mod checks;
mod dictionaries;
mod objects;

use crate::core::types::{Dict, Document};

use super::{OpiError, OpiProxy};

use dictionaries::parse_version_dictionary;

/// 从流/字典中解析 `/OPI`；条目缺省返回 `Ok(None)`。
pub(crate) fn parse_opi_entry(
    document: &Document,
    dict: &Dict,
) -> Result<Option<OpiProxy>, OpiError> {
    let Some(value) = dict.get("OPI") else {
        return Ok(None);
    };
    parse_version_dictionary(document, value).map(Some)
}
