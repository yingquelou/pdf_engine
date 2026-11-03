//! 交叉引用表与 trailer 的一致性诊断：F.3.4 首页表/trailer 与 F.3.11 主表。
//!
//! 对象组规则在 [`super::objects`]；偏移范围与识别相关规则留在 [`super`]。

use crate::core::reader::find_subslice;
use crate::core::reader::lexer::Lexer;
use crate::core::reader::object::parse_trailer;
use crate::core::types::{Document, Value, XrefKind};

use super::super::{LinearizationInfo, LinearizationIssue, MAIN_TABLE_WINDOW};
use super::read_object_at;

/// F.3.4：首页 xref 表是单段、无 free 条目，且把字典列在表首、
/// 主 hint 流列在表尾。
pub(super) fn table_entry_order(
    document: &Document,
    info: &LinearizationInfo,
    issues: &mut Vec<LinearizationIssue>,
) {
    let Some(section) = document
        .xref
        .0
        .iter()
        .find(|section| (section.start..section.end()).contains(&info.object_number))
    else {
        return;
    };
    let free_entries = section
        .entries
        .iter()
        .filter(|entry| !entry.in_use())
        .count();
    if free_entries > 0 {
        issues.push(LinearizationIssue::FirstPageTableFreeEntries {
            count: free_entries,
        });
    }
    if section.start != info.object_number {
        issues.push(LinearizationIssue::TableEntryOrder {
            role: "linearization parameter dictionary",
            object_number: info.object_number,
        });
    }
    let Some(streams) = info.hint_streams else {
        return;
    };
    let offset = streams.primary.offset as i64;
    let Some(index) = section
        .entries
        .iter()
        .position(|entry| entry.kind == XrefKind::Normal && entry.offset == offset)
    else {
        return;
    };
    if index + 1 != section.entries.len() {
        issues.push(LinearizationIssue::TableEntryOrder {
            role: "primary hint stream",
            object_number: section.start + index as i64,
        });
    }
}

/// F.3.4 要求首页 trailer 携带的条目与该条目的角色。
const FIRST_PAGE_TRAILER_KEYS: [(&str, &str); 3] = [
    ("Prev", "the offset of the main cross-reference table"),
    ("Size", "the total number of cross-reference entries"),
    ("Root", "a reference to the document catalogue"),
];

/// F.3.4：「The first-page trailer shall contain valid Size and Root entries」，
/// 且「The trailer's Prev entry shall give the offset of the main cross-reference
/// table」。缺失或形态不符各判一条。
///
/// 只在 `startxref` 落到首页表时判定（解析到的第一段含线性化字典）：线性化文件
/// 被增量更新后 `startxref` 指向增量段，此时 `Document::trailer` 不是首页
/// trailer，照 F.3.4 下诊断会是假阳性。
pub(super) fn first_page_trailer(
    document: &Document,
    info: &LinearizationInfo,
    issues: &mut Vec<LinearizationIssue>,
) {
    let reaches_first_page_table = document
        .xref
        .0
        .first()
        .is_some_and(|section| (section.start..section.end()).contains(&info.object_number));
    if !reaches_first_page_table {
        return;
    }
    for (key, role) in FIRST_PAGE_TRAILER_KEYS {
        let Some(value) = document.trailer.dict.get(key) else {
            issues.push(LinearizationIssue::FirstPageTrailerEntry {
                key,
                problem: format!("it is missing, but the entry shall carry {role}"),
            });
            continue;
        };
        // `Prev` 与 `Size` 是字节偏移/条目计数，`Root` 是间接引用（Table 15）。
        let well_formed = match key {
            "Prev" | "Size" => matches!(value, Value::Integer(_)),
            _ => matches!(value, Value::Ref(..)),
        };
        if !well_formed {
            issues.push(LinearizationIssue::FirstPageTrailerEntry {
                key,
                problem: format!("its value is {} rather than {role}", value.type_name()),
            });
        }
    }
}

/// Table F.1 `/T` 与 F.3.11：主交叉引用表/流的定位与其 trailer。
///
/// 传统表按 F.3.3 的措辞取「主表首条目之前的那个空白字符」偏移；纯交叉引用
/// 流文档的 `/T` 是主 XRef 流对象的偏移（Table F.1 的 PDF 1.5 段）。
pub(super) fn main_cross_reference(
    document: &Document,
    info: &LinearizationInfo,
    issues: &mut Vec<LinearizationIssue>,
) {
    let Some(offset) = info.main_xref_offset else {
        return;
    };
    let Some(bytes) = document.read_at(offset, MAIN_TABLE_WINDOW) else {
        return;
    };
    let Some(first) = bytes.first().copied() else {
        return;
    };
    if Lexer::is_whitespace(first) {
        traditional_main_table(&bytes, offset, issues);
    } else if first.is_ascii_digit() {
        stream_main_table(document, offset, issues);
    } else {
        issues.push(LinearizationIssue::MainCrossReferenceOffset {
            offset,
            problem: format!(
                "the byte there is {first:#04x}, expected the white space preceding the first entry (Table F.1) or an object number (cross-reference stream)"
            ),
        });
    }
}

/// 传统主表：首条目须是 free 条目；主 trailer 除 `Size` 外不得含其他条目。
fn traditional_main_table(bytes: &[u8], offset: u64, issues: &mut Vec<LinearizationIssue>) {
    // `/T` 之后可能还有空白，逐个跳过再匹配 20 字节定长条目。
    let mut cursor = 1usize;
    while bytes
        .get(cursor)
        .is_some_and(|byte| Lexer::is_whitespace(*byte))
    {
        cursor += 1;
    }
    match first_entry_type(bytes.get(cursor..).unwrap_or(&[])) {
        None => issues.push(LinearizationIssue::MainCrossReferenceOffset {
            offset,
            problem: "no cross-reference entry is readable at the given offset".to_string(),
        }),
        Some(b'f') => {}
        Some(_) => issues.push(LinearizationIssue::MainCrossReferenceTable {
            problem: "its first entry (object number 0) shall be a free entry".to_string(),
        }),
    }
    // 表尾 trailer：F.3.11 规定主 trailer 只有 `Size`，且没有 `Prev`。
    // 在 `/T` 起的有界窗口内查找 `trailer` 关键字；窗口内找不到（主表过长）
    // 则跳过本检查，避免把窗口边界误报成违规。
    let Some(position) = find_subslice(bytes, b"trailer", 0) else {
        return;
    };
    let mut lex = Lexer::at(bytes, offset);
    lex.pos = position;
    let Ok(trailer) = parse_trailer(&mut lex) else {
        return;
    };
    let others: Vec<String> = trailer
        .dict
        .0
        .keys()
        .filter(|key| key.as_str() != "Size")
        .cloned()
        .collect();
    if !others.is_empty() {
        issues.push(LinearizationIssue::MainTrailerEntries { keys: others });
    }
}

/// XRef 流承担主表 trailer 语义的条目：F.3.11 允许 `Size`，禁止 `Prev`；
/// 其余 trailer 专有条目（`Root`/`Info`/`ID`/`Encrypt`）同样不得出现在主表。
const FORBIDDEN_STREAM_KEYS: [&str; 5] = ["Prev", "Root", "Info", "ID", "Encrypt"];

/// 交叉引用流主表：`/T` 处须是 `/Type /XRef` 流，其字典（承担 trailer 语义）
/// 不得含 `Prev` 与其他 trailer 专有条目。
fn stream_main_table(document: &Document, offset: u64, issues: &mut Vec<LinearizationIssue>) {
    let Some(value) = read_object_at(document, offset) else {
        return;
    };
    let Value::Stream(dict, _) = value else {
        issues.push(LinearizationIssue::MainCrossReferenceOffset {
            offset,
            problem: "the object there is not a stream".to_string(),
        });
        return;
    };
    if dict.get("Type").and_then(|value| value.as_name()) != Some("XRef") {
        issues.push(LinearizationIssue::MainCrossReferenceOffset {
            offset,
            problem: "the stream there is not /Type /XRef".to_string(),
        });
        return;
    }
    let others: Vec<String> = FORBIDDEN_STREAM_KEYS
        .iter()
        .filter(|key| dict.get(key).is_some())
        .map(|key| (*key).to_string())
        .collect();
    if !others.is_empty() {
        issues.push(LinearizationIssue::MainTrailerEntries { keys: others });
    }
}

/// 定长 xref 条目的类型字符（`n`/`f`）；形状不符返回 `None`。
///
/// 7.5.4 的条目固定 20 字节：`0000000000 65535 f `，即 10 位偏移、分隔空白、
/// 5 位代号、分隔空白、类型字符（下标 17）。
fn first_entry_type(bytes: &[u8]) -> Option<u8> {
    let offset_digits = bytes.get(..10)?;
    let generation_digits = bytes.get(11..16)?;
    let kind = *bytes.get(17)?;
    if !offset_digits.iter().all(u8::is_ascii_digit)
        || !generation_digits.iter().all(u8::is_ascii_digit)
    {
        return None;
    }
    if !Lexer::is_whitespace(bytes[10]) || !Lexer::is_whitespace(bytes[16]) {
        return None;
    }
    if kind != b'n' && kind != b'f' {
        return None;
    }
    Some(kind)
}
