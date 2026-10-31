//! 标准结构属性所有者的写端校验（ISO 32000-1 14.8.5.2、14.8.5.4–14.8.5.7）。
//!
//! 属性对象仍按 14.7.5 Table 327 表达；本模块在
//! `builder/navigation/structure.rs` 的展开阶段按 `/O` 复核 Layout/List/
//! PrintField/Table 标准条目的值形态，并在展平完成后核对 `/Headers`
//! 引用的元素 `/ID` 确实是解析后的 `TH`。元素 `/A` 另做适用性检查：
//!
//! - PrintField 属性只能挂在 `Form`（含经 `/RoleMap` 映射为 `Form`）上；
//! - Table 的 `/RowSpan` `/ColSpan` `/Headers` 只能用于 `TH`/`TD`，`/Scope`
//!   只能用于 `TH`，`/Summary` 只能用于 `Table`；
//! - `/ListNumbering` 可继承（14.8.5.3），允许出现在任意元素上向子孙传播。
//!
//! Layout 条目的值形态与适用性由 [`super::structure_layout`] 处理。
//! `/ClassMap` 的属性对象可能被不同类型的元素复用，因此只做值形态检查，
//! 不做适用性检查。全部失败返回 [`WriteError::InvalidObject`]；本模块不
//! 分配对象号，调用方保证在写出任何字节之前完成。

use std::collections::{BTreeMap, BTreeSet};

use crate::core::types::{Dict, FeatureProfile, PdfFeature, PdfVersion, Value};
use crate::core::writer::WriteError;

use super::invalid;
use super::structure_layout;

/// `/Headers` 单个数组的条目上限（与结构元素子项上限一致）。
const MAX_TABLE_HEADERS: usize = 65536;

/// 结构树使用标准属性的情况，供版本门禁与特性画像使用。
#[derive(Debug, Default)]
pub(super) struct StandardUsage {
    /// 出现 Layout/List/PrintField/Table 任一标准所有者的属性对象（PDF 1.4）。
    pub(super) standard_attributes: bool,
    /// 出现 PrintField 所有者（PDF 1.7）。
    pub(super) print_field_attributes: bool,
    /// 出现 `/Headers` 或 `/Scope`（PDF 1.5）。
    pub(super) table_header_attributes: bool,
    /// 出现 `/Summary`（PDF 1.7）。
    pub(super) table_summary: bool,
    /// 出现 Table 343/345 中 PDF 1.5 引入的 Layout 条目。
    pub(super) layout_extended_attributes: bool,
    /// 出现 Table 346 中 PDF 1.6 引入的分栏 Layout 条目。
    pub(super) layout_column_attributes: bool,
    /// 待核对的 `/Headers` 引用：上下文 + 表头 `/ID` 列表。
    headers: Vec<(String, Vec<Vec<u8>>)>,
}

impl StandardUsage {
    /// 按使用情况检查版本门禁；任一特性不被支持即返回
    /// [`WriteError::UnsupportedVersion`]。
    pub(super) fn check_version(&self, version: PdfVersion) -> Result<(), WriteError> {
        if self.standard_attributes && !version.supports(PdfFeature::StandardStructureAttributes) {
            return Err(WriteError::UnsupportedVersion(format!(
                "standard structure attributes require PDF {}, got {version}",
                PdfFeature::StandardStructureAttributes.min_version()
            )));
        }
        if self.print_field_attributes && !version.supports(PdfFeature::PrintFieldAttributes) {
            return Err(WriteError::UnsupportedVersion(format!(
                "PrintField attributes require PDF {}, got {version}",
                PdfFeature::PrintFieldAttributes.min_version()
            )));
        }
        if self.table_header_attributes && !version.supports(PdfFeature::TableHeaderAttributes) {
            return Err(WriteError::UnsupportedVersion(format!(
                "table header attributes require PDF {}, got {version}",
                PdfFeature::TableHeaderAttributes.min_version()
            )));
        }
        if self.table_summary && !version.supports(PdfFeature::TableSummary) {
            return Err(WriteError::UnsupportedVersion(format!(
                "table summary requires PDF {}, got {version}",
                PdfFeature::TableSummary.min_version()
            )));
        }
        if self.layout_extended_attributes
            && !version.supports(PdfFeature::LayoutExtendedAttributes)
        {
            return Err(WriteError::UnsupportedVersion(format!(
                "extended Layout attributes require PDF {}, got {version}",
                PdfFeature::LayoutExtendedAttributes.min_version()
            )));
        }
        if self.layout_column_attributes && !version.supports(PdfFeature::LayoutColumnAttributes) {
            return Err(WriteError::UnsupportedVersion(format!(
                "Layout column attributes require PDF {}, got {version}",
                PdfFeature::LayoutColumnAttributes.min_version()
            )));
        }
        Ok(())
    }

    /// 成功写出后把标准属性特性登记到文档画像。
    pub(super) fn apply_features(&self, features: &mut FeatureProfile) {
        if self.standard_attributes {
            features.insert(PdfFeature::StandardStructureAttributes);
        }
        if self.print_field_attributes {
            features.insert(PdfFeature::PrintFieldAttributes);
        }
        if self.table_header_attributes {
            features.insert(PdfFeature::TableHeaderAttributes);
        }
        if self.table_summary {
            features.insert(PdfFeature::TableSummary);
        }
        if self.layout_extended_attributes {
            features.insert(PdfFeature::LayoutExtendedAttributes);
        }
        if self.layout_column_attributes {
            features.insert(PdfFeature::LayoutColumnAttributes);
        }
    }

    /// 将累计的 `/Headers` 引用与展平后的元素 `/ID` 对照。
    ///
    /// `id_types` 为元素 `/ID` → 经 `/RoleMap` 解析后的结构类型。未知 ID
    /// 或非 `TH` 目标均返回 [`WriteError::InvalidObject`]。
    pub(super) fn validate_header_ids(
        &self,
        id_types: &BTreeMap<Vec<u8>, String>,
    ) -> Result<(), WriteError> {
        for (context, ids) in &self.headers {
            for id in ids {
                let Some(resolved) = id_types.get(id) else {
                    return Err(invalid(format!(
                        "{context} /Headers references an unknown structure element /ID {id:?}"
                    )));
                };
                if resolved != "TH" {
                    return Err(invalid(format!(
                        "{context} /Headers /ID {id:?} must reference a TH element (resolved type {resolved})"
                    )));
                }
            }
        }
        Ok(())
    }
}

/// 校验元素 `/A` 中的一个属性对象（含适用性检查）。
pub(super) fn validate_element_attribute(
    value: &Value,
    context: &str,
    structure_type: &str,
    role_map: &BTreeMap<String, String>,
    usage: &mut StandardUsage,
) -> Result<(), WriteError> {
    let (owner, dict) = attribute_parts(value, context)?;
    let resolved = resolved_type(role_map, structure_type);
    validate_entries(owner, dict, context, Some(&resolved), usage)
}

/// 校验 `/ClassMap` 中的一个属性对象（只查值形态，不查适用性）。
pub(super) fn validate_class_attribute(
    value: &Value,
    context: &str,
    usage: &mut StandardUsage,
) -> Result<(), WriteError> {
    let (owner, dict) = attribute_parts(value, context)?;
    validate_entries(owner, dict, context, None, usage)
}

/// 沿 `/RoleMap` 解析结构类型；成环时停在环上已访问的名字，避免无限循环。
pub(super) fn resolved_type(role_map: &BTreeMap<String, String>, structure_type: &str) -> String {
    let mut current = structure_type.to_owned();
    let mut seen = BTreeSet::new();
    seen.insert(current.clone());
    while let Some(next) = role_map.get(&current) {
        if !seen.insert(next.clone()) {
            break;
        }
        current = next.clone();
    }
    current
}

/// 取出属性对象的 `/O` 与字典；Table 327 的完整校验由调用方先前完成。
fn attribute_parts<'a>(value: &'a Value, context: &str) -> Result<(&'a str, &'a Dict), WriteError> {
    let Some(dict) = value.as_dict() else {
        return Err(invalid(format!(
            "{context} must be an attribute dictionary or stream ({})",
            value.type_name()
        )));
    };
    let Some(Value::Name(owner)) = dict.get("O") else {
        return Err(invalid(format!("{context} /O must be a name")));
    };
    Ok((owner.as_str(), dict))
}

/// 按所有者复核标准条目，并累计使用情况。
fn validate_entries(
    owner: &str,
    dict: &Dict,
    context: &str,
    resolved: Option<&str>,
    usage: &mut StandardUsage,
) -> Result<(), WriteError> {
    match owner {
        "Layout" => {
            usage.standard_attributes = true;
            structure_layout::validate_entries(dict, context, resolved, usage)?;
        }
        "List" => {
            usage.standard_attributes = true;
            if let Some(value) = dict.get("ListNumbering") {
                require_name(value, context, "ListNumbering")?;
            }
        }
        "PrintField" => {
            usage.standard_attributes = true;
            usage.print_field_attributes = true;
            // 14.8.5.6：PrintField 属性用于 Form 结构元素。
            if let Some(resolved) = resolved
                && resolved != "Form"
            {
                return Err(invalid(format!(
                    "{context} /O /PrintField applies only to Form structure elements (resolved type {resolved})"
                )));
            }
            for key in ["Role", "checked"] {
                if let Some(value) = dict.get(key) {
                    require_name(value, context, key)?;
                }
            }
            if let Some(value) = dict.get("Desc") {
                require_text_string(value, context, "Desc")?;
            }
        }
        "Table" => {
            usage.standard_attributes = true;
            apply_table_entries(dict, context, resolved, usage)?;
        }
        _ => {}
    }
    Ok(())
}

/// Table 标准条目（Table 349）的值形态、适用性与使用情况。
fn apply_table_entries(
    dict: &Dict,
    context: &str,
    resolved: Option<&str>,
    usage: &mut StandardUsage,
) -> Result<(), WriteError> {
    for key in ["RowSpan", "ColSpan"] {
        if let Some(value) = dict.get(key) {
            require_span(value, context, key)?;
        }
    }
    for key in ["RowSpan", "ColSpan", "Headers"] {
        if dict.0.contains_key(key) {
            check_applicable(resolved, context, key, &["TH", "TD"])?;
        }
    }
    let mut headers = Vec::new();
    if let Some(value) = dict.get("Headers") {
        headers = require_headers(value, context)?;
        usage.table_header_attributes = true;
    }
    if let Some(value) = dict.get("Scope") {
        require_name(value, context, "Scope")?;
        check_applicable(resolved, context, "Scope", &["TH"])?;
        usage.table_header_attributes = true;
    }
    if let Some(value) = dict.get("Summary") {
        require_text_string(value, context, "Summary")?;
        check_applicable(resolved, context, "Summary", &["Table"])?;
        usage.table_summary = true;
    }
    if !headers.is_empty() {
        usage.headers.push((context.to_owned(), headers));
    }
    Ok(())
}

/// 元素 `/A` 的适用性：`resolved` 为 `None`（ClassMap）时跳过。
fn check_applicable(
    resolved: Option<&str>,
    context: &str,
    key: &str,
    allowed: &[&str],
) -> Result<(), WriteError> {
    let Some(resolved) = resolved else {
        return Ok(());
    };
    if allowed.contains(&resolved) {
        return Ok(());
    }
    Err(invalid(format!(
        "{context} /{key} applies only to {} structure elements (resolved type {resolved})",
        allowed.join(" or ")
    )))
}

/// `/Headers`：非空数组、每项非空 ASCII 字节串、互不重复。
fn require_headers(value: &Value, context: &str) -> Result<Vec<Vec<u8>>, WriteError> {
    let Value::Array(items) = value else {
        return Err(invalid(format!(
            "{context} /Headers must be an array of byte strings ({})",
            value.type_name()
        )));
    };
    if items.0.is_empty() {
        return Err(invalid(format!("{context} /Headers must not be empty")));
    }
    if items.0.len() > MAX_TABLE_HEADERS {
        return Err(invalid(format!(
            "{context} /Headers has {} entries (limit {MAX_TABLE_HEADERS})",
            items.0.len()
        )));
    }
    let mut seen = BTreeSet::new();
    let mut headers = Vec::with_capacity(items.0.len());
    for (position, item) in items.0.iter().enumerate() {
        let bytes = match item {
            Value::String(text) => text
                .chars()
                .map(|character| character as u32 as u8)
                .collect(),
            Value::HexString(hex) => decode_hex(hex).ok_or_else(|| {
                invalid(format!(
                    "{context} /Headers entry {position} is not valid hexadecimal"
                ))
            })?,
            other => {
                return Err(invalid(format!(
                    "{context} /Headers entry {position} must be a byte string ({})",
                    other.type_name()
                )));
            }
        };
        if bytes.is_empty() {
            return Err(invalid(format!(
                "{context} /Headers entry {position} must not be empty"
            )));
        }
        if bytes.iter().any(|byte| *byte > 0x7F) {
            return Err(invalid(format!(
                "{context} /Headers entry {position} must contain ASCII bytes only (0x00-0x7F)"
            )));
        }
        if !seen.insert(bytes.clone()) {
            return Err(invalid(format!(
                "{context} /Headers entry {position} duplicates another /ID"
            )));
        }
        headers.push(bytes);
    }
    Ok(headers)
}

fn require_name(value: &Value, context: &str, key: &str) -> Result<(), WriteError> {
    if matches!(value, Value::Name(_)) {
        Ok(())
    } else {
        Err(invalid(format!(
            "{context} /{key} must be a name ({})",
            value.type_name()
        )))
    }
}

fn require_text_string(value: &Value, context: &str, key: &str) -> Result<(), WriteError> {
    if matches!(value, Value::String(_) | Value::HexString(_)) {
        Ok(())
    } else {
        Err(invalid(format!(
            "{context} /{key} must be a text string ({})",
            value.type_name()
        )))
    }
}

fn require_span(value: &Value, context: &str, key: &str) -> Result<(), WriteError> {
    match value {
        Value::Integer(span) if *span >= 1 && *span <= i64::from(u32::MAX) => Ok(()),
        other => Err(invalid(format!(
            "{context} /{key} must be an integer of at least 1 ({})",
            other.type_name()
        ))),
    }
}

/// 十六进制串 → 原始字节；奇数位末尾补 0（与 `/ID` 解码一致）。
fn decode_hex(hex: &str) -> Option<Vec<u8>> {
    let mut digits: Vec<u8> = hex
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    if digits.len() % 2 == 1 {
        digits.push(b'0');
    }
    let mut bytes = Vec::with_capacity(digits.len() / 2);
    for pair in digits.as_chunks::<2>().0 {
        let high = hex_digit(pair[0])?;
        let low = hex_digit(pair[1])?;
        bytes.push(high * 16 + low);
    }
    Some(bytes)
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
