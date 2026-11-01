//! 写端上限与共享小校验：MCID 上限、PDF 1.5 扩展标准结构类型目录
//! （14.8.4）与名字/MCID 取值校验，供其余子模块复用。

use crate::core::builder::navigation::invalid;
use crate::core::reader::structure::MAX_STRUCTURE_CHILDREN;
use crate::core::writer::WriteError;

/// 写端 MCID 上限：父树数组按 MCID 作零基下标，与读端单元素子项上限
/// 取同值，避免单页父树数组无界膨胀（65535 为最大可用 MCID）。
pub(in crate::core::builder::navigation) const MAX_STRUCTURE_MCID: i64 =
    MAX_STRUCTURE_CHILDREN as i64 - 1;

/// PDF 1.5 引入的标准结构类型（14.8.4）：`THead` `TBody` `TFoot`
/// （Table 337）、`Annot`（Table 338）与 Ruby/Warichu 家族（Table 339）。
const EXTENDED_STRUCTURE_TYPES: &[&str] = &[
    "THead", "TBody", "TFoot", "Annot", "Ruby", "RB", "RT", "RP", "Warichu", "WT", "WP",
];

/// 名字是否为 PDF 1.5 引入的标准结构类型（14.8.4）。
pub(super) fn is_extended_structure_type(name: &str) -> bool {
    EXTENDED_STRUCTURE_TYPES.contains(&name)
}
/// MCID 取值校验（非负且不超 `MAX_STRUCTURE_MCID`），页面通道与流形态
/// `/Stm` 通道共用；错误文案保持既有形式。
pub(super) fn validate_mcid_value(mcid: i64) -> Result<(), WriteError> {
    if mcid < 0 {
        return Err(invalid(format!(
            "structure MCR MCID must be non-negative, got {mcid}"
        )));
    }
    if mcid > MAX_STRUCTURE_MCID {
        return Err(invalid(format!(
            "structure MCR MCID {mcid} exceeds the limit {MAX_STRUCTURE_MCID}"
        )));
    }
    Ok(())
}
/// 结构类型、类名与角色映射的名字：非空且不含空白/名字定界符。
pub(super) fn validate_name(value: &str, context: &str) -> Result<(), WriteError> {
    if value.is_empty() {
        return Err(invalid(format!("{context} must not be empty")));
    }
    if value
        .bytes()
        .any(|byte| byte <= 0x20 || byte >= 0x7F || b"()<>[]{}/%#".contains(&byte))
    {
        return Err(invalid(format!(
            "{context} {value:?} is not a valid PDF name"
        )));
    }
    Ok(())
}
