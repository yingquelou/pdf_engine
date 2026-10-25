//! 子集化用的 DICT 原始条目游走与改写。
//!
//! 与 `raw::parse` 不同，这里保留每个条目的原始字节跨度：未触及的条目
//! 逐字节拷贝（保留实数格式等原始编码），仅把需要重算偏移的条目以定长
//! 5 字节 `29`（longint）操作数重新编码。定长编码让 DICT 长度与偏移
//! 取值无关，布局可以单趟确定，无需迭代收敛。

use super::super::raw::parse::dict_number;
use crate::core::reader::font::{FontError, FontResult};

/// 一个 DICT 条目：操作符、操作数值与原始字节跨度。
#[derive(Debug)]
pub(super) struct RawDictEntry {
    /// 操作符（`12 x` 转义形式编码为 `0x0C00 | x`）。
    pub(super) operator: u16,
    /// 操作数值（与 `raw::parse` 的 DICT 数字解码一致）。
    pub(super) operands: Vec<f64>,
    /// 条目首字节（第一个操作数）在源 DICT 中的位置。
    pub(super) start: usize,
    /// 操作符字节的位置。
    pub(super) op_start: usize,
    /// 条目末尾（操作符之后）的位置。
    pub(super) end: usize,
}

/// 按字节游走一个 DICT，返回全部条目（含操作数值与跨度）。
///
/// 截断或非法编码返回类型化错误；全程迭代，无递归。
pub(super) fn walk_dict(data: &[u8]) -> FontResult<Vec<RawDictEntry>> {
    let mut entries = Vec::new();
    let mut position = 0;
    let mut entry_start = 0;
    let mut operands: Vec<f64> = Vec::new();
    while position < data.len() {
        let first = data[position];
        if first <= 21 {
            let op_start = position;
            position += 1;
            let operator = if first == 12 {
                let Some(&second) = data.get(position) else {
                    return Err(truncated_dict(position));
                };
                position += 1;
                0x0C00 | u16::from(second)
            } else {
                u16::from(first)
            };
            entries.push(RawDictEntry {
                operator,
                operands: std::mem::take(&mut operands),
                start: entry_start,
                op_start,
                end: position,
            });
            entry_start = position;
            continue;
        }
        operands.push(dict_number(data, &mut position)?);
        if operands.len() > 48 {
            return Err(FontError::Invalid(
                "invalid CFF subset: DICT has more than 48 operands".into(),
            ));
        }
    }
    if entry_start != position {
        return Err(FontError::Invalid(
            "invalid CFF subset: DICT ends with operands but no operator".into(),
        ));
    }
    Ok(entries)
}

/// 以定长 5 字节 longint（`29`）编码一个非负整数操作数。
pub(super) fn dict_longint(value: usize) -> [u8; 5] {
    let value = u32::try_from(value).expect("CFF offsets fit in u32");
    let mut out = [0u8; 5];
    out[0] = 29;
    out[1..5].copy_from_slice(&value.to_be_bytes());
    out
}

/// 改写 DICT：`operands_for` 返回 `Some(操作数序列)` 的条目以 longint
/// 编码重建，其余条目逐字节拷贝。
///
/// 改写条目的字节数固定为 `5 * 操作数个数 + 操作符长度`，与偏移取值
/// 无关，因此返回长度可以先用占位偏移计算再回填真实值。
pub(super) fn rewrite_dict(
    data: &[u8],
    entries: &[RawDictEntry],
    operands_for: impl FnMut(&RawDictEntry) -> Option<Vec<usize>>,
) -> Vec<u8> {
    rewrite_dict_skipping(data, entries, |_| false, operands_for)
}

/// 同 [`rewrite_dict`]，但 `skip` 返回 `true` 的条目被整条删除。
///
/// name-keyed 子集用它丢弃 Top DICT 的 `Encoding`：字形重编号后旧的
/// 内建编码不再对应保留字形，输出退回默认 StandardEncoding。
pub(super) fn rewrite_dict_skipping(
    data: &[u8],
    entries: &[RawDictEntry],
    mut skip: impl FnMut(&RawDictEntry) -> bool,
    mut operands_for: impl FnMut(&RawDictEntry) -> Option<Vec<usize>>,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    for entry in entries {
        if skip(entry) {
            continue;
        }
        match operands_for(entry) {
            Some(operands) => {
                for value in operands {
                    out.extend_from_slice(&dict_longint(value));
                }
                out.extend_from_slice(&data[entry.op_start..entry.end]);
            }
            None => out.extend_from_slice(&data[entry.start..entry.end]),
        }
    }
    out
}

fn truncated_dict(position: usize) -> FontError {
    FontError::Invalid(format!(
        "invalid CFF subset: truncated DICT at offset {position}"
    ))
}
