//! F.3.6 的 hint 流字典**直接对象**核对（读端诊断；与 [`super::assemble`] 的
//! Table F.2 键位收集分属两条 shall，故单列文件）。
//!
//! 原文：「With one exception, the values of all entries in the hint streams'
//! dictionaries shall be direct objects and may contain no indirect object
//! references. The exception is the stream dictionary's Length entry」
//! （`pdftotext -raw docs/PDF32000_2008.pdf` 输出 L35310）。两句各有一半约束力：
//! - 「shall be direct objects」——值本身是 `num gen R` 即违规；
//! - 「may contain no indirect object references」——直接容器（数组/字典）里
//!   嵌着引用同样违规，故核对**下探一层以上**而非只看顶层。
//!
//! `/Length` 是唯一豁免（间接 `/Length` 是 7.3.10 的正常形态，读端也确要按它
//! 取流字节）；本核对只**报告**、不改写值也不解引用，后续 Table F.2 键位解析
//! 仍按原值给类型不符诊断（见 [`super::assemble`] 的 `table_positions`）。

use crate::core::types::{Dict, Value};

use super::super::issues::LinearizationIssue;

/// 扫描两条 hint 流字典：`which` 取 `"primary"` 或 `"overflow"`，与
/// [`LinearizationIssue::HintStreamUnreadable`] 的口径一致。
pub(super) fn check_hint_stream_direct_objects(
    dict: &Dict,
    which: &'static str,
    issues: &mut Vec<LinearizationIssue>,
) {
    for (key, value) in dict.0.iter() {
        // F.3.6 明示的唯一例外。
        if key == "Length" {
            continue;
        }
        if contains_indirect_reference(value) {
            issues.push(LinearizationIssue::HintStreamIndirectValue {
                which,
                key: key.clone(),
            });
        }
    }
}

/// 值内是否出现间接引用（显式栈遍历：解析器可给出任意嵌套的直接容器）。
fn contains_indirect_reference(value: &Value) -> bool {
    let mut stack = vec![value];
    while let Some(current) = stack.pop() {
        match current {
            Value::Ref(..) => return true,
            Value::Dict(dict) => stack.extend(dict.0.values()),
            Value::Array(array) => stack.extend(array.0.iter()),
            _ => {}
        }
    }
    false
}
