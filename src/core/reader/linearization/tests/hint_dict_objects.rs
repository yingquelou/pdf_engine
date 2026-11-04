//! F.3.6 对 **hint 流字典**的直接对象核对用例（自 [`hints`] 拆出以守 600 行/
//! 25600 字节文件上限；位级表字节与键位条目沿用 [`hints`] 的装配 helper）。
//!
//! 原文（`pdftotext -raw docs/PDF32000_2008.pdf` 输出 L35310）：「With one
//! exception, the values of all entries in the hint streams' dictionaries shall be
//! direct objects and may contain no indirect object references. The exception is the
//! stream dictionary's Length entry」——两句分别由「顶层即引用」「直接容器里嵌引用」
//! 两例驱动，`/Length` 豁免与溢出流复数形态各一例。

use super::hints::{crafted, hint_entries, hint_payload};
use super::{Knobs, open};
use crate::core::reader::linearization::LinearizationIssue;

#[test]
fn hint_stream_indirect_entry_is_diagnosed() {
    // F.3.6（L35310）：hint 流字典的值除 `/Length` 外须是直接对象；`/S` 写成
    // 引用即违规。原值**不**被解引用，故 Table F.2 键位解析另给类型不符诊断。
    let payload = hint_payload(0, 2);
    let entries = " /S 4 0 R".to_string();
    let mut knobs = Knobs::default();
    crafted(&mut knobs, &payload, &entries);
    let document = open(&super::build(&knobs), "hint-indirect-entry");
    let hints = document.linearization_hints().expect("io").expect("hints");
    assert!(
        hints
            .issues
            .contains(&LinearizationIssue::HintStreamIndirectValue {
                which: "primary",
                key: "S".to_string()
            }),
        "{:#?}",
        hints.issues
    );
    assert!(
        hints.issues.iter().any(|issue| matches!(
            issue,
            LinearizationIssue::HintTablePositionInvalid { key, .. } if key == "S"
        )),
        "引用值须另落类型不符诊断: {:#?}",
        hints.issues
    );
    // 页偏移表恒在偏移 0，与本键无关，照常解析。
    assert!(hints.page_offset.is_some(), "页偏移表不应被该键拖垮");
}

#[test]
fn hint_stream_reference_nested_in_direct_array_is_diagnosed() {
    // F.3.6 的第二半句「may contain no indirect object references」：直接容器
    // 里嵌着引用同样违规，故核对下探进数组元素。
    let payload = hint_payload(0, 2);
    let entries = " /S [4 0 R]".to_string();
    let mut knobs = Knobs::default();
    crafted(&mut knobs, &payload, &entries);
    let document = open(&super::build(&knobs), "hint-nested-ref");
    let hints = document.linearization_hints().expect("io").expect("hints");
    assert!(
        hints
            .issues
            .contains(&LinearizationIssue::HintStreamIndirectValue {
                which: "primary",
                key: "S".to_string()
            }),
        "{:#?}",
        hints.issues
    );
}

#[test]
fn hint_stream_indirect_length_is_the_f36_exception() {
    // 「The exception is the stream dictionary's Length entry」：间接 `/Length`
    // 是 7.3.10 的正常形态，读端确实按它取流字节，故不得报 F.3.6 诊断。
    let payload = hint_payload(0, 2);
    let entries = hint_entries();
    let mut knobs = Knobs::default();
    crafted(&mut knobs, &payload, &entries);
    knobs.replaced = &[("LengthRef", "1")];
    let document = open(&super::build(&knobs), "hint-length-exception");
    let hints = document.linearization_hints().expect("io").expect("hints");
    assert!(
        !hints
            .issues
            .iter()
            .any(|issue| matches!(issue, LinearizationIssue::HintStreamIndirectValue { .. })),
        "间接 /Length 是唯一豁免: {:#?}",
        hints.issues
    );
    assert!(hints.page_offset.is_some(), "表仍须可读");
    assert!(hints.shared_object.is_some(), "表仍须可读");
}

#[test]
fn overflow_hint_stream_indirect_entry_is_diagnosed() {
    // F.3.6 说的是「hint streams' dictionaries」（复数）：`/H` 第二对指向的
    // 溢出流字典同样受核，且 `which` 须区分两条流。
    let payload = hint_payload(0, 2);
    let entries = hint_entries();
    let mut knobs = Knobs::default();
    crafted(&mut knobs, &payload, &entries);
    knobs.overflow_hint = true;
    knobs.overflow_entries = " /X 4 0 R";
    let document = open(&super::build(&knobs), "hint-overflow-indirect-entry");
    let hints = document.linearization_hints().expect("io").expect("hints");
    assert!(
        hints
            .issues
            .contains(&LinearizationIssue::HintStreamIndirectValue {
                which: "overflow",
                key: "X".to_string()
            }),
        "{:#?}",
        hints.issues
    );
    assert!(
        !hints.issues.iter().any(|issue| matches!(
            issue,
            LinearizationIssue::HintStreamIndirectValue {
                which: "primary",
                ..
            }
        )),
        "主流字典保持合规，不该被连带报告: {:#?}",
        hints.issues
    );
}
