//! 文档级 hint 表用例：把位级手工装配的共享对象 hint 表塞进附录 F 夹具，
//! 经 `Document::linearization_hints()` 解析回类型化数据，并驱动各类
//! 畸形形态确认只出类型化诊断、不 panic。

use super::Knobs;
use super::{CONTENT_PAYLOAD, open};
use crate::core::reader::linearization::LinearizationIssue;
use crate::core::reader::linearization::hints::bits::BitWriter;

/// 表前的占位字节数：F.4 规定 page offset hint 表恒在偏移 0，本切片
/// 不解析它，用 4 个零字节占位；共享对象表由字典 `/S 4` 定位。
const PAGE_OFFSET_PLACEHOLDER: usize = 4;

/// 位级装配一张合法的共享对象 hint 表（Table F.5/F.6）：首页 2 组
/// （首组带 128 位 MD5 签名）、共享区 1 组。
fn shared_table_payload() -> Vec<u8> {
    let mut w = BitWriter::default();
    // Table F.5 头部：32/32/32/32/16/32/16。
    w.unsigned(20, 32); // 项 1：共享对象区首对象号
    w.unsigned(5000, 32); // 项 2：共享对象区首对象位置
    w.unsigned(2, 32); // 项 3：首页条目数
    w.unsigned(3, 32); // 项 4：总条目数
    w.unsigned(8, 16); // 项 5：组对象数位宽
    w.unsigned(100, 32); // 项 6：组最小长度
    w.unsigned(12, 16); // 项 7：组长度位宽
    // F.4.2：项 1—4 的交错是**按序列**各自进行的——先首页 2 组，
    // 再共享区 1 组；每组内顺序为 全部项 1 → 全部项 2 → 标志组的
    // 项 3 签名 → 全部项 4。
    // 序列一（首页 2 组）：项 1 长度增量（12 位）。
    w.unsigned(5, 12);
    w.unsigned(7, 12);
    // 项 2 签名标志（1 位）。
    w.flag(true);
    w.flag(false);
    // 项 3：按标志顺序的 128 位 MD5（0x00..=0x0F）。
    for byte in 0x00u8..=0x0F {
        w.unsigned(byte as u64, 8);
    }
    // 项 4 对象数减一（8 位）。
    w.unsigned(3, 8);
    w.unsigned(0, 8);
    // 序列二（共享区 1 组）：项 1 → 项 2 → 项 4（无签名）。
    w.unsigned(9, 12);
    w.flag(false);
    w.unsigned(1, 8);
    let mut payload = vec![0u8; PAGE_OFFSET_PLACEHOLDER];
    payload.extend_from_slice(&w.bytes);
    payload
}

/// 装配了合法表与 `/S` 定位条目的默认旋钮形态。
fn crafted<'a>(knobs: &mut Knobs<'a>, payload: &'a [u8], entries: &'a str) {
    knobs.hint_payload = Some(payload);
    knobs.hint_entries = entries;
}

#[test]
fn shared_hint_table_decodes_through_document() {
    let payload = shared_table_payload();
    let mut knobs = Knobs::default();
    crafted(&mut knobs, &payload, " /S 4");
    let document = open(&super::build(&knobs), "hint-ok");
    let info = document.linearization().expect("io").expect("linearized");
    assert!(info.is_consistent(), "fixture must stay clean: {info:?}");
    let hints = document.linearization_hints().expect("io").expect("hints");
    assert!(hints.is_consistent(), "unexpected {:#?}", hints.issues);
    assert_eq!(hints.stream_bytes, payload.len());
    assert_eq!(hints.position_of("S"), Some(4));
    let table = hints.shared_object.as_ref().expect("parsed shared table");
    assert_eq!(table.header.first_object_number, 20);
    assert_eq!(table.header.first_object_location, 5000);
    assert_eq!(table.header.first_page_entries, 2);
    assert_eq!(table.header.total_entries, 3);
    assert_eq!(table.header.group_object_bits, 8);
    assert_eq!(table.header.least_group_length, 100);
    assert_eq!(table.header.group_length_bits, 12);
    assert_eq!(table.first_page_groups.len(), 2);
    assert_eq!(table.first_page_groups[0].length_delta, 5);
    assert_eq!(table.first_page_groups[0].object_count_minus_one, 3);
    let signature: Vec<u8> = (0x00u8..=0x0F).collect();
    assert_eq!(
        table.first_page_groups[0].signature,
        signature.as_slice().try_into().ok(),
        "Table F.6 item 3 MD5 must survive the round trip"
    );
    assert_eq!(table.first_page_groups[1].signature, None);
    assert_eq!(table.first_page_groups[1].length_delta, 7);
    assert_eq!(table.shared_section_groups.len(), 1);
    assert_eq!(table.shared_section_groups[0].length_delta, 9);
    // F.4 NOTE 3 的位置修正：不大于主流起点的位置原样，大于则补流长。
    let base = hints.primary.offset;
    assert_eq!(hints.adjust_position(base), base);
    assert_eq!(
        hints.adjust_position(base + 1),
        base + 1 + hints.primary.length
    );
}

#[test]
fn shared_hint_table_missing_key_is_reported() {
    // 缺省载荷是不透明字节、字典无 `/S`：Table F.2 的 Required 键缺失。
    let knobs = Knobs::default();
    let document = open(&super::build(&knobs), "hint-no-s");
    let hints = document.linearization_hints().expect("io").expect("hints");
    assert!(
        hints
            .issues
            .contains(&LinearizationIssue::SharedHintTableMissing),
        "{:#?}",
        hints.issues
    );
    assert!(hints.shared_object.is_none());
}

#[test]
fn every_truncation_of_hint_payload_is_a_typed_issue() {
    let full = shared_table_payload();
    // 起点取 `/S 4`：短于 4 字节的载荷会先撞上「表位超出流长」
    // （HintTablePositionInvalid），那是另一类诊断，由
    // [`hint_table_position_beyond_stream_is_reported`] 覆盖。
    for prefix in PAGE_OFFSET_PLACEHOLDER..full.len() {
        let mut knobs = Knobs::default();
        crafted(&mut knobs, &full[..prefix], " /S 4");
        let document = open(&super::build(&knobs), "hint-trunc");
        let hints = document.linearization_hints().expect("io").expect("hints");
        assert!(
            hints.issues.iter().any(|issue| matches!(
                issue,
                LinearizationIssue::HintTableCorrupt { key, .. } if key == "S"
            )),
            "prefix {prefix}/{} must yield a typed corrupt issue, got {:#?}",
            full.len(),
            hints.issues
        );
        assert!(hints.shared_object.is_none());
    }
}

#[test]
fn hint_table_position_beyond_stream_is_reported() {
    let payload = shared_table_payload();
    let mut knobs = Knobs::default();
    crafted(&mut knobs, &payload, " /S 9999");
    let document = open(&super::build(&knobs), "hint-pos");
    let hints = document.linearization_hints().expect("io").expect("hints");
    assert!(
        hints.issues.iter().any(|issue| matches!(
            issue,
            LinearizationIssue::HintTablePositionInvalid { key, .. } if key == "S"
        )),
        "{:#?}",
        hints.issues
    );
    // 键存在但越界：不再叠加「缺 S」假阳性。
    assert!(
        !hints
            .issues
            .contains(&LinearizationIssue::SharedHintTableMissing),
        "{:#?}",
        hints.issues
    );
}

#[test]
fn unreadable_primary_stream_is_reported() {
    // `/H` 指向线性化字典对象（非流）：位流无从取起。
    let knobs = Knobs {
        bad_hint_target: true,
        ..Default::default()
    };
    let document = open(&super::build(&knobs), "hint-target");
    let hints = document.linearization_hints().expect("io").expect("hints");
    assert!(
        hints.issues.iter().any(|issue| matches!(
            issue,
            LinearizationIssue::HintStreamUnreadable { which, .. } if *which == "primary"
        )),
        "{:#?}",
        hints.issues
    );
    assert!(hints.shared_object.is_none());
    assert_eq!(hints.stream_bytes, 0);
}

#[test]
fn header_item4_smaller_than_item3_is_reported() {
    // Table F.5 项 4 须**含**项 3：项 3=5 > 项 4=2 属自相矛盾。
    let mut w = BitWriter::default();
    w.unsigned(0, 32);
    w.unsigned(0, 32);
    w.unsigned(5, 32);
    w.unsigned(2, 32);
    w.unsigned(8, 16);
    w.unsigned(0, 32);
    w.unsigned(4, 16);
    for _ in 0..2 {
        w.unsigned(0, 4);
    }
    for _ in 0..2 {
        w.flag(false);
    }
    for _ in 0..2 {
        w.unsigned(0, 8);
    }
    let mut payload = vec![0u8; PAGE_OFFSET_PLACEHOLDER];
    payload.extend_from_slice(&w.bytes);
    let mut knobs = Knobs::default();
    crafted(&mut knobs, &payload, " /S 4");
    let document = open(&super::build(&knobs), "hint-count");
    let hints = document.linearization_hints().expect("io").expect("hints");
    assert!(
        hints.issues.iter().any(|issue| matches!(
            issue,
            LinearizationIssue::HintTableCorrupt { key, problem }
                if key == "S" && problem.contains("item 4")
        )),
        "{:#?}",
        hints.issues
    );
    // 截断到项 4 后照常给出部分结果（两首页组、零共享区组）。
    let table = hints.shared_object.expect("partial table decoded");
    assert_eq!(table.first_page_groups.len(), 2);
    assert!(table.shared_section_groups.is_empty());
}

#[test]
fn overflow_stream_concatenates_hint_bytes() {
    // F.3.6：溢出 hint 流（解码后）无缝拼接到主流尾部再定位各表。
    let payload = shared_table_payload();
    let knobs = Knobs {
        overflow_hint: true,
        hint_payload: Some(&payload),
        hint_entries: " /S 4",
        ..Default::default()
    };
    let document = open(&super::build(&knobs), "hint-overflow");
    let hints = document.linearization_hints().expect("io").expect("hints");
    assert!(hints.is_consistent(), "{:#?}", hints.issues);
    assert_eq!(
        hints.stream_bytes,
        payload.len() + CONTENT_PAYLOAD.len(),
        "overflow stream bytes must append after the primary table bytes"
    );
    assert!(hints.shared_object.is_some());
}
