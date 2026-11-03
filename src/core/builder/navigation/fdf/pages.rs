//! FDF 页/注解写端**装配前校验**（Table 248–251；口径总述在
//! [`super`] 模块头）：只下降直接数组里的直接字典，间接引用原样透传，
//! 条目数上限与读端 `reader/fdf/pages.rs` 的
//! `MAX_FDF_PAGES_ARRAY_ENTRIES` 共用，保证「写得出就读得回、且不截断」。
//! Table 249 模板 `/Fields` 的字段项复用 [`super::walk_field`] 的
//! Table 246 校验并共享同一 [`super::FieldCounter`] 节点计数。

use crate::core::reader::fdf::{MAX_FDF_PAGES_ARRAY_ENTRIES, PROHIBITED_FDF_ANNOTATION_TYPES};
use crate::core::types::{Dict, PdfVersion, Value};
use crate::core::writer::WriteError;

use super::{FieldCounter, invalid, walk_field};

/// 单个数组条目数与读端上限同口径：读端超限截断并记
/// `FdfPagesIssue::ArrayTruncated`（见 `reader/fdf/pages.rs`），写端直接
/// 拒绝，保证产出的文件能被读端**完整**读回。
fn check_pages_array_len(entries: &[Value], what: &str) -> Result<(), WriteError> {
    if entries.len() > MAX_FDF_PAGES_ARRAY_ENTRIES {
        return Err(invalid(format!(
            "FDF {what} exceeds the per-array limit {MAX_FDF_PAGES_ARRAY_ENTRIES} shared with the reader"
        )));
    }
    Ok(())
}

/// Table 243 — `/Pages` 数组（PDF 1.3；raw L24874–L24877），逐项走
/// Table 248。12.7.7.3.3（raw L25055–L25057）「The optional Pages field in
/// an FDF dictionary (see Table 243) shall contain an array of FDF page
/// dictionaries (PDF 1.3) describing new pages that shall be added to the
/// target document」。
pub(super) fn walk_pages_array(
    entries: &[Value],
    counter: &mut FieldCounter,
    effective: PdfVersion,
) -> Result<(), WriteError> {
    check_pages_array_len(entries, "/Pages array (Table 243)")?;
    for (index, entry) in entries.iter().enumerate() {
        match entry {
            Value::Dict(dict) => walk_page(dict, index, counter, effective)?,
            // 间接引用：本入口只写一个间接对象，无法解引用 → 原样透传。
            Value::Ref(..) => {}
            _ => {
                return Err(invalid(format!(
                    "FDF /Pages entry {index} must be an FDF page dictionary (Table 248)"
                )));
            }
        }
    }
    Ok(())
}

/// 单个 FDF 页字典（Table 248，raw L25085–L25089）。
fn walk_page(
    dict: &Dict,
    index: usize,
    counter: &mut FieldCounter,
    effective: PdfVersion,
) -> Result<(), WriteError> {
    // Table 248 — `/Templates` array（**Required**；raw L25086–L25087
    // 「An array of FDF template dictionaries (see Table 249) that shall
    // describe the named pages that serve as templates on the page」）。
    let array = dict
        .get("Templates")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            invalid(format!(
                "FDF page {index} requires a /Templates array (Table 248, Required)"
            ))
        })?;
    check_pages_array_len(&array.0, &format!("FDF page {index} /Templates array"))?;
    for (slot, template) in array.0.iter().enumerate() {
        match template {
            Value::Dict(dict) => walk_template(dict, index, slot, counter, effective)?,
            Value::Ref(..) => {}
            _ => {
                return Err(invalid(format!(
                    "FDF page {index} template {slot} must be an FDF template dictionary (Table 249)"
                )));
            }
        }
    }
    // Table 248 — `/Info` dictionary（Optional；raw L25088–L25089「An FDF
    // page information dictionary」）：该术语在 32000-1 全文只出现这一次、
    // 无条目表（Adobe 扩展，待核），故只按「非缺失即字典」把关、不下降。
    if let Some(info) = dict.get("Info")
        && !matches!(info, Value::Dict(_) | Value::Stream(_, _) | Value::Ref(..))
    {
        return Err(invalid(format!(
            "FDF page {index} /Info must be a dictionary (Table 248)"
        )));
    }
    Ok(())
}

/// 单个 FDF 模板字典（Table 249，raw L25113–L25123）。
fn walk_template(
    dict: &Dict,
    page: usize,
    slot: usize,
    counter: &mut FieldCounter,
    effective: PdfVersion,
) -> Result<(), WriteError> {
    let label = format!("FDF page {page} template {slot}");
    // Table 249 — `/TRef` dictionary（**Required**；raw L25115–L25116
    // 「A named page reference dictionary (see Table 250) that shall
    // specify the location of the template」；12.7.7.3.3 raw L25097–L25098
    // 「The TRef entry in an FDF template dictionary shall hold a named
    // page reference dictionary」）。
    match dict.get("TRef") {
        None => {
            return Err(invalid(format!(
                "{label} requires /TRef (Table 249, Required)"
            )));
        }
        // 引用形态按原值透传（本入口只写一个间接对象，无法解引用）。
        Some(Value::Ref(..)) => {}
        Some(reference) => {
            let Some(reference) = reference.as_dict() else {
                return Err(invalid(format!(
                    "{label} /TRef must be a named page reference dictionary (Table 250)"
                )));
            };
            // Table 250 — `/Name` string（**Required**；raw L25126
            // 「The name of the referenced page」）。
            match reference.get("Name") {
                Some(Value::String(_) | Value::HexString(_)) => {}
                _ => {
                    return Err(invalid(format!(
                        "{label} /TRef requires a /Name text string (Table 250, Required)"
                    )));
                }
            }
            // Table 250 — `/F` file specification（Optional；raw
            // L25127–L25128「If this entry is absent, it shall be assumed
            // that the page resides in the associated PDF file」）：形态
            // 多样（串或文件规范字典），本切片不下降，缺席也不伪造。
        }
    }
    // Table 249 — `/Rename` boolean（Optional；**Default value: true**；
    // raw L25120–L25122）：缺省不物化，出现时必须是布尔。
    if let Some(flag) = dict.get("Rename")
        && !matches!(flag, Value::Bool(_))
    {
        return Err(invalid(format!(
            "{label} /Rename must be a boolean (Table 249; Default value: true)"
        )));
    }
    // Table 249 — `/Fields` array（Optional；raw L25117–L25119「An array of
    // references to FDF field dictionaries (see Table 246) describing the
    // root fields that shall be imported」）：与 `/Fields` 走同一套 Table 246
    // 校验并共享节点计数，保证写出的字段树必然落在读端上限内。
    if let Some(value) = dict.get("Fields") {
        let array = value
            .as_array()
            .ok_or_else(|| invalid(format!("{label} /Fields must be an array (Table 249)")))?;
        check_pages_array_len(&array.0, &format!("{label} /Fields array"))?;
        for (index, field) in array.0.iter().enumerate() {
            match field {
                Value::Dict(dict) => walk_field(dict, 1, counter, effective)?,
                // Table 249 的条目本就是引用：间接字段无法就地校验，透传。
                Value::Ref(..) => {}
                _ => {
                    return Err(invalid(format!(
                        "{label} /Fields entry {index} must be a field dictionary or a reference to one (Table 249)"
                    )));
                }
            }
        }
    }
    Ok(())
}

/// Table 243 — `/Annots` 数组（PDF 1.3；raw L24885–L24889），逐项走
/// Table 251 与注解类型白名单。12.7.7.3.4（raw L25100–L25101）「Each
/// annotation dictionary in an FDF file shall have a Page entry (see
/// Table 251) that shall indicate the page of the source document to which
/// the annotation is attached」。
pub(super) fn walk_annots_array(entries: &[Value]) -> Result<(), WriteError> {
    check_pages_array_len(entries, "/Annots array (Table 243)")?;
    for (index, entry) in entries.iter().enumerate() {
        match entry {
            Value::Dict(dict) => walk_annotation(dict, index)?,
            Value::Ref(..) => {}
            _ => {
                return Err(invalid(format!(
                    "FDF /Annots entry {index} must be an annotation dictionary (Table 251)"
                )));
            }
        }
    }
    Ok(())
}

/// 单个 FDF 注解字典：Table 251（raw L25129–L25132）的 `/Page` 与
/// Table 243 的注解类型黑名单；12.5.2 公共条目与 Table 169 起的子类型
/// 条目一律原样透传（本入口不负责注解装配，只把关 FDF 专有条目）。
fn walk_annotation(dict: &Dict, index: usize) -> Result<(), WriteError> {
    let label = format!("FDF annotation {index}");
    // Table 251 — `/Page` integer（**Required for annotations in FDF
    // files**；raw L25131–L25132「The ordinal page number on which this
    // annotation shall appear, where page 0 is the first page」）。
    match dict.get("Page") {
        Some(Value::Integer(page)) if *page >= 0 => {}
        Some(Value::Integer(page)) => {
            return Err(invalid(format!(
                "{label} /Page {page} is not a valid ordinal; page 0 is the first page (Table 251)"
            )));
        }
        Some(_) => {
            return Err(invalid(format!(
                "{label} /Page must be an integer (Table 251)"
            )));
        }
        None => {
            return Err(invalid(format!(
                "{label} requires /Page (Table 251, Required for annotations in FDF files)"
            )));
        }
    }
    // Table 243 — 注解类型（raw L24887–L24889）「may include annotations of
    // any of the standard types listed in Table 169 except Link, Movie,
    // Widget, PrinterMark, Screen, and TrapNet」；`/Subtype` 本身按 12.5.2
    // （Table 169）是 Required name。
    let Some(subtype) = dict.get("Subtype") else {
        return Err(invalid(format!(
            "{label} requires /Subtype (Table 169, Required)"
        )));
    };
    let Some(name) = subtype.as_name() else {
        return Err(invalid(format!(
            "{label} /Subtype must be a name object (Table 169)"
        )));
    };
    if PROHIBITED_FDF_ANNOTATION_TYPES.contains(&name) {
        return Err(invalid(format!(
            "{label} /Subtype /{name} is excluded from FDF /Annots arrays (Table 243)"
        )));
    }
    Ok(())
}
