//! FDF 目录（Table 242）→ FDF 字典（Table 243）→ 字段树（Table 246）
//! 的语法层解析。
//!
//! 解引用口径：`/Root`、`/FDF`、`/Fields`、`/Kids` 等**结构键**按
//! `reader/navigation::resolve` 宽容解引用（间接引用与直接对象皆可，
//! 与写端常见形态对齐）；`/V`、`/Opt`、`/Pages` 等**内容值**保留原值
//! 不解引用，由调用方按需处理。

use std::collections::HashSet;

use crate::core::reader::content::text_value;
use crate::core::reader::navigation::resolve;
use crate::core::types::{Dict, Document, Value};

use super::{
    FdfDictionary, FdfDocument, FdfError, FdfField, MAX_FDF_FIELD_DEPTH, MAX_FDF_FIELD_NODES,
    malformed,
};

type Result<T> = std::result::Result<T, FdfError>;

/// 字段树遍历防护：节点计数 + 引用环检测（与 `reader/form.rs` 同一手法）。
struct Walker {
    nodes: usize,
    visited: HashSet<(i64, i64)>,
}

impl Walker {
    fn admit(&mut self, depth: usize) -> Result<()> {
        if depth > MAX_FDF_FIELD_DEPTH {
            return Err(FdfError::LimitExceeded("FDF field depth"));
        }
        if self.nodes >= MAX_FDF_FIELD_NODES {
            return Err(FdfError::LimitExceeded("FDF field count"));
        }
        self.nodes += 1;
        Ok(())
    }
}

/// 入口：检测并解析 FDF 读模型；非 FDF 返回 `Ok(None)`。
pub(crate) fn fdf_document(document: &Document) -> Result<Option<FdfDocument>> {
    let Some(root) = document.trailer.dict.get("Root") else {
        return Ok(None);
    };
    // 普通 PDF 的悬空 `/Root` 不应让 FDF 探测报错（库策略：探测阶段宽容）。
    let Ok(root) = resolve(document, root) else {
        return Ok(None);
    };
    let Some(catalog) = root.as_dict().cloned() else {
        return Ok(None);
    };
    let Some(fdf_value) = catalog.get("FDF") else {
        return Ok(None);
    };

    // 至此确认按 FDF 处理（Table 242：`/FDF` 为必填字典条目），结构错误如实报。
    let fdf_value = resolve(document, fdf_value).map_err(|error| malformed(error.to_string()))?;
    let fdf_dict = fdf_value
        .as_dict()
        .ok_or_else(|| malformed("FDF catalog /FDF does not reference a dictionary"))?
        .clone();

    let catalog_version = match catalog.get("Version") {
        None => None,
        Some(value) => Some(
            value
                .as_name()
                .ok_or_else(|| malformed("FDF catalog /Version is not a name"))?
                .to_owned(),
        ),
    };

    let mut walker = Walker {
        nodes: 0,
        visited: HashSet::new(),
    };
    let fields = match fdf_dict.get("Fields") {
        None => None,
        Some(value) => {
            let value = resolve(document, value).map_err(|error| malformed(error.to_string()))?;
            let array = value
                .as_array()
                .ok_or_else(|| malformed("FDF dictionary /Fields is not an array"))?;
            Some(parse_field_array(document, &array.0, "", 0, &mut walker)?)
        }
    };

    let status = match fdf_dict.get("Status") {
        None => None,
        Some(value) => Some(
            text_value(value)
                .ok_or_else(|| malformed("FDF dictionary /Status is not a text string"))?,
        ),
    };
    let encoding = match fdf_dict.get("Encoding") {
        None => None,
        Some(value) => Some(
            value
                .as_name()
                .ok_or_else(|| malformed("FDF dictionary /Encoding is not a name"))?
                .to_owned(),
        ),
    };

    Ok(Some(FdfDocument {
        catalog_version,
        catalog,
        fdf: FdfDictionary {
            source_file: fdf_dict.get("F").cloned(),
            identifier: fdf_dict.get("ID").cloned(),
            fields,
            status,
            pages: fdf_dict.get("Pages").cloned(),
            encoding,
            annots: fdf_dict.get("Annots").cloned(),
            differences: fdf_dict.get("Differences").cloned(),
            target: fdf_dict.get("Target").cloned(),
            embedded_fdfs: fdf_dict.get("EmbeddedFDFs").cloned(),
            javascript: fdf_dict.get("JavaScript").cloned(),
            dict: fdf_dict,
        },
    }))
}

/// 解析一个字段数组（`/Fields` 根数组或 `/Kids` 子数组）。
///
/// Table 246：FDF 字段的 `/Kids` 元素允许**直接对象**（PDF 字段树则要求
/// 间接引用），因此逐项 resolve 后按字典校验。
fn parse_field_array(
    document: &Document,
    entries: &[Value],
    parent_fqn: &str,
    depth: usize,
    walker: &mut Walker,
) -> Result<Vec<FdfField>> {
    let mut fields = Vec::with_capacity(entries.len());
    for entry in entries {
        if let Some(key) = entry.as_ref()
            && !walker.visited.insert(key)
        {
            return Err(malformed("FDF field tree contains a reference cycle"));
        }
        let value = resolve(document, entry).map_err(|error| malformed(error.to_string()))?;
        let dict = value
            .as_dict()
            .ok_or_else(|| malformed("FDF field is not a dictionary"))?;
        fields.push(parse_field(document, dict, parent_fqn, depth, walker)?);
    }
    Ok(fields)
}

/// 解析单个 FDF 字段字典（Table 246）。
fn parse_field(
    document: &Document,
    dict: &Dict,
    parent_fqn: &str,
    depth: usize,
    walker: &mut Walker,
) -> Result<FdfField> {
    walker.admit(depth)?;

    let partial_name = match dict.get("T") {
        Some(value) => text_value(value).ok_or_else(|| {
            malformed(format!("FDF field /T is not a text string at {parent_fqn}"))
        })?,
        None => {
            return Err(malformed(format!(
                "FDF field is missing required /T under \"{parent_fqn}\""
            )));
        }
    };
    // 全限定名连接规则与 `reader/form.rs`（AcroForm 字段树，12.7.3.2）
    // 完全一致：非空 `/T` 以 `.` 挂到父名之后；空或缺失 `/T` 不扩展父名。
    let fully_qualified_name = if partial_name.is_empty() {
        parent_fqn.to_string()
    } else if parent_fqn.is_empty() {
        partial_name.clone()
    } else {
        format!("{parent_fqn}.{partial_name}")
    };

    let kids = match dict.get("Kids") {
        None => Vec::new(),
        Some(value) => {
            let value = resolve(document, value).map_err(|error| malformed(error.to_string()))?;
            let array = value
                .as_array()
                .ok_or_else(|| malformed("FDF field /Kids is not an array"))?;
            parse_field_array(document, &array.0, &fully_qualified_name, depth + 1, walker)?
        }
    };

    Ok(FdfField {
        partial_name,
        fully_qualified_name,
        value: dict.get("V").cloned(),
        field_flags: parse_integer(dict, "Ff")?,
        set_field_flags: parse_integer(dict, "SetFf")?,
        clear_field_flags: parse_integer(dict, "ClrFf")?,
        widget_flags: parse_integer(dict, "F")?,
        set_widget_flags: parse_integer(dict, "SetF")?,
        clear_widget_flags: parse_integer(dict, "ClrF")?,
        options: dict.get("Opt").cloned(),
        appearance: dict.get("AP").cloned(),
        appearance_reference: dict.get("APRef").cloned(),
        icon_fit: dict.get("IF").cloned(),
        action: dict.get("A").cloned(),
        additional_actions: dict.get("AA").cloned(),
        rich_value: dict.get("RV").cloned(),
        kids,
        dict: dict.clone(),
    })
}

/// Table 246 的整型标志条目（`/Ff`、`/SetFf`、`/ClrFf`、`/F`、`/SetF`、
/// `/ClrF`）：出现时必须为整数，原值保留（位含义由调用方按 Table 221/
/// 226/228/230 与 12.5.3 解释）。
fn parse_integer(dict: &Dict, key: &str) -> Result<Option<i64>> {
    match dict.get(key) {
        None => Ok(None),
        Some(Value::Integer(bits)) => Ok(Some(*bits)),
        Some(_) => Err(malformed(format!("FDF field /{key} is not an integer"))),
    }
}
