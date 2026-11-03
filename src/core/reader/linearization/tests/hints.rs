//! 文档级 hint 表用例：把位级手工装配的**页偏移 hint 表**（F.4.1，Table F.3/F.4）
//! 与**共享对象 hint 表**（F.4.2，Table F.5/F.6）塞进附录 F 夹具，经
//! `Document::linearization_hints()` 解析回类型化数据，并驱动各类畸形形态
//! 确认只出类型化诊断、不 panic。

use super::Knobs;
use super::{CONTENT_PAYLOAD, open};
use crate::core::reader::linearization::LinearizationIssue;
use crate::core::reader::linearization::hints::bits::BitWriter;

/// 夹具文档的页数（与 Table F.1 `/N` 的缺省值一致）；页偏移表按此成列。
const PAGES: usize = 2;

/// Table F.3 头部（13 项，5×32 + 8×16 = 288 位）+ Table F.4 两页条目按
/// F.4.1 a)—g) 的列序装配。
///
/// 定值项：最少对象数 3、首页页对象位置 700、最少页长 300、内容流起点偏移
/// 最小 20、内容流长度最小 50；位宽项（3/5/7/9/10/11/12）依次 3/6/5/8/2/4/3，
/// 分母（项 13）4。
///
/// `first_page_refs` 驱动 Table F.4 项 3 的首页 shall（合规值 0）：非零时
/// 项 4/5 的列里**不含**首页元素，原值只保留在类型化条目里。
fn page_offset_table_bits(first_page_refs: u64) -> Vec<u8> {
    let mut w = BitWriter::default();
    w.unsigned(3, 32); // 项 1
    w.unsigned(700, 32); // 项 2
    w.unsigned(3, 16); // 项 3：对象数增量的位宽
    w.unsigned(300, 32); // 项 4
    w.unsigned(6, 16); // 项 5：页长增量的位宽
    w.unsigned(20, 32); // 项 6
    w.unsigned(5, 16); // 项 7：内容流起点偏移增量的位宽
    w.unsigned(50, 32); // 项 8
    w.unsigned(8, 16); // 项 9：内容流长度增量的位宽
    w.unsigned(2, 16); // 项 10：共享对象引用数的位宽
    w.unsigned(4, 16); // 项 11：共享对象标识的位宽
    w.unsigned(3, 16); // 项 12：分子位宽
    w.unsigned(4, 16); // 项 13：分母
    // a) 项 1 列（3 位）。
    w.unsigned(0, 3);
    w.unsigned(1, 3);
    // b) 项 2 列（6 位）。
    w.unsigned(10, 6);
    w.unsigned(25, 6);
    // c) 项 3 列（2 位）：首页 0（shall），第二页 2 个引用。
    w.unsigned(first_page_refs, 2);
    w.unsigned(2, 2);
    // d) 项 4 列（4 位）：第二页的两个标识。
    w.unsigned(1, 4);
    w.unsigned(5, 4);
    // e) 项 5 列（3 位）：与项 4 同序的分子。
    w.unsigned(2, 3);
    w.unsigned(0, 3);
    // f) 项 6 列（5 位）。
    w.unsigned(4, 5);
    w.unsigned(7, 5);
    // g) 项 7 列（8 位）。
    w.unsigned(11, 8);
    w.unsigned(13, 8);
    w.bytes
}

/// Table F.5/F.6 的共享对象 hint 表（首页 2 组，首组带 128 位 MD5 签名；
/// 共享区 1 组；交错**按序列各自**进行）。
fn shared_table_bits() -> Vec<u8> {
    let mut w = BitWriter::default();
    w.unsigned(20, 32); // 项 1：共享对象区首对象号
    w.unsigned(5000, 32); // 项 2：共享对象区首对象位置
    w.unsigned(2, 32); // 项 3：首页条目数
    w.unsigned(3, 32); // 项 4：总条目数（含项 3）
    w.unsigned(8, 16); // 项 5：组对象数位宽
    w.unsigned(100, 32); // 项 6：组最小长度
    w.unsigned(12, 16); // 项 7：组长度位宽
    // 序列一（首页 2 组）：项 1 → 项 2 → 项 3 → 项 4。
    w.unsigned(5, 12);
    w.unsigned(7, 12);
    w.flag(true);
    w.flag(false);
    for byte in 0x00u8..=0x0F {
        w.unsigned(byte as u64, 8);
    }
    w.unsigned(3, 8);
    w.unsigned(0, 8);
    // 序列二（共享区 1 组）：项 1 → 项 2 → 项 4（无签名）。
    w.unsigned(9, 12);
    w.flag(false);
    w.unsigned(1, 8);
    w.bytes
}

/// 页偏移表占用的字节数：F.4 前言把本表钉在流偏移 0，故它后面的第一张表
/// 的最小合法位置就是本值（350 位 → 44 字节）。
fn page_offset_bytes() -> usize {
    page_offset_table_bits(0).len()
}

/// Table F.2 键 `S` 的定位条目原文：紧跟页偏移表之后。
fn hint_entries() -> String {
    format!(" /S {}", page_offset_bytes())
}

/// hint 流载荷：偏移 0 的页偏移表 + 紧随其后的共享对象表。
fn hint_payload(first_page_refs: u64) -> Vec<u8> {
    let mut payload = page_offset_table_bits(first_page_refs);
    payload.extend(shared_table_bits());
    payload
}

/// 装配了合法两张表与 `/S` 定位条目的形态。
fn crafted<'a>(knobs: &mut Knobs<'a>, payload: &'a [u8], entries: &'a str) {
    knobs.hint_payload = Some(payload);
    knobs.hint_entries = entries;
}

#[test]
fn page_offset_table_decodes_through_document() {
    let payload = hint_payload(0);
    let entries = hint_entries();
    let mut knobs = Knobs::default();
    crafted(&mut knobs, &payload, &entries);
    let document = open(&super::build(&knobs), "page-offset-ok");
    let hints = document.linearization_hints().expect("io").expect("hints");
    assert!(hints.is_consistent(), "unexpected {:#?}", hints.issues);
    let table = hints
        .page_offset
        .as_ref()
        .expect("parsed page offset table");
    // F.4 前言：本表恒在流偏移 0，长度即后一张表的最小位置。
    assert_eq!(table.header.least_objects_per_page, 3);
    assert_eq!(table.header.first_page_object_location, 700);
    assert_eq!(table.header.object_count_bits, 3);
    assert_eq!(table.header.least_page_length, 300);
    assert_eq!(table.header.page_length_bits, 6);
    assert_eq!(table.header.least_content_offset, 20);
    assert_eq!(table.header.content_offset_bits, 5);
    assert_eq!(table.header.least_content_length, 50);
    assert_eq!(table.header.content_length_bits, 8);
    assert_eq!(table.header.shared_ref_count_bits, 2);
    assert_eq!(table.header.shared_object_id_bits, 4);
    assert_eq!(table.header.shared_position_numerator_bits, 3);
    assert_eq!(table.header.shared_position_denominator, 4);
    assert_eq!(table.declared_pages, PAGES as u64);
    assert_eq!(table.pages.len(), PAGES);
    // Table F.4 项 1/2/6/7 的增量按头部最小值折算。
    assert_eq!(table.pages[0].object_count_delta, 0);
    assert_eq!(table.pages[0].page_length_delta, 10);
    assert_eq!(table.pages[0].shared_reference_count, 0);
    assert!(table.pages[0].shared_object_ids.is_empty());
    assert_eq!(table.pages[0].content_offset_delta, 4);
    assert_eq!(table.pages[0].content_length_delta, 11);
    assert_eq!(table.pages[0].object_count(&table.header), 3);
    assert_eq!(table.pages[0].page_length(&table.header), 310);
    assert_eq!(table.pages[0].content_offset(&table.header), 24);
    assert_eq!(table.pages[0].content_length(&table.header), 61);
    assert_eq!(table.pages[1].object_count(&table.header), 4);
    assert_eq!(table.pages[1].page_length(&table.header), 325);
    // d)/e) 两列按页归位，不跨页混排。
    assert_eq!(table.pages[1].shared_reference_count, 2);
    assert_eq!(table.pages[1].shared_object_ids, vec![1, 5]);
    assert_eq!(table.pages[1].shared_position_numerators, vec![2, 0]);
    assert_eq!(table.pages[1].content_offset(&table.header), 27);
    assert_eq!(table.pages[1].content_length(&table.header), 63);
    assert_eq!(table.bits_consumed, 350);
    assert_eq!(table.end_byte(), page_offset_bytes() as u64);
    assert_eq!(
        hints.page_offset_end_byte(),
        Some(page_offset_bytes() as u64)
    );
    assert_eq!(hints.position_of("S"), Some(page_offset_bytes() as u64));
    // 位置核对不影响后续家族的解析起点。
    assert!(hints.shared_object.is_some());
    assert_eq!(hints.stream_bytes, payload.len());
}

#[test]
fn shared_hint_table_decodes_through_document() {
    let payload = hint_payload(0);
    let entries = hint_entries();
    let mut knobs = Knobs::default();
    crafted(&mut knobs, &payload, &entries);
    let document = open(&super::build(&knobs), "hint-ok");
    let info = document.linearization().expect("io").expect("linearized");
    assert!(info.is_consistent(), "fixture must stay clean: {info:?}");
    let hints = document.linearization_hints().expect("io").expect("hints");
    assert!(hints.is_consistent(), "unexpected {:#?}", hints.issues);
    assert_eq!(hints.stream_bytes, payload.len());
    assert_eq!(hints.position_of("S"), Some(page_offset_bytes() as u64));
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
    let full = hint_payload(0);
    let page_offset_len = page_offset_bytes();
    // 截在页偏移表内：该家族给 PageOffsetTableCorrupt；`/S` 位置超出流长另给
    // HintTablePositionInvalid（由 [`hint_table_position_beyond_stream_is_reported`]
    // 覆盖形态，不断言它）。
    for prefix in 0..page_offset_len {
        let mut knobs = Knobs::default();
        crafted(&mut knobs, &full[..prefix], " /S 9999");
        let document = open(&super::build(&knobs), "hint-trunc-page-offset");
        let hints = document.linearization_hints().expect("io").expect("hints");
        assert!(
            hints
                .issues
                .iter()
                .any(|issue| matches!(issue, LinearizationIssue::PageOffsetTableCorrupt { .. })),
            "prefix {prefix}/{page_offset_len} must yield a typed page offset issue, got {:#?}",
            hints.issues
        );
        assert!(hints.page_offset.is_none());
    }
    // 截在两表之间或共享对象表内：页偏移表完整、共享对象表损坏。
    let entries = hint_entries();
    for prefix in page_offset_len..full.len() {
        let mut knobs = Knobs::default();
        crafted(&mut knobs, &full[..prefix], &entries);
        let document = open(&super::build(&knobs), "hint-trunc-shared");
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
        assert!(hints.page_offset.is_some());
    }
}

#[test]
fn hint_table_position_beyond_stream_is_reported() {
    let payload = hint_payload(0);
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
fn hint_table_position_inside_page_offset_table_is_reported() {
    // F.4 前言：表以字节边界开始且按序读取，故 `/S 3` 落进页偏移表体内自相矛盾。
    let payload = hint_payload(0);
    let mut knobs = Knobs::default();
    crafted(&mut knobs, &payload, " /S 3");
    let document = open(&super::build(&knobs), "hint-inside");
    let hints = document.linearization_hints().expect("io").expect("hints");
    assert!(
        hints.issues.iter().any(|issue| matches!(
            issue,
            LinearizationIssue::HintTablePositionInvalid { key, problem }
                if key == "S" && problem.contains("inside the page offset hint table")
        )),
        "{:#?}",
        hints.issues
    );
    // 位置不可信即不再解析该键：不给假的 `S` 损坏诊断，也不给「缺 S」。
    assert!(hints.shared_object.is_none());
    assert!(
        !hints
            .issues
            .iter()
            .any(|issue| matches!(issue, LinearizationIssue::HintTableCorrupt { .. })),
        "{:#?}",
        hints.issues
    );
    assert!(
        !hints
            .issues
            .contains(&LinearizationIssue::SharedHintTableMissing),
        "{:#?}",
        hints.issues
    );
    // 页偏移表本身照常解析。
    assert!(hints.page_offset.is_some());
}

#[test]
fn first_page_shared_ref_count_is_reported() {
    // Table F.4 项 3：首页的共享对象引用数 shall 为 0；原值保留、列不被错位读。
    let payload = hint_payload(3);
    let entries = hint_entries();
    let mut knobs = Knobs::default();
    crafted(&mut knobs, &payload, &entries);
    let document = open(&super::build(&knobs), "hint-first-page-refs");
    let hints = document.linearization_hints().expect("io").expect("hints");
    assert!(
        hints
            .issues
            .contains(&LinearizationIssue::PageOffsetFirstPageSharedRefs { count: 3 }),
        "{:#?}",
        hints.issues
    );
    let table = hints.page_offset.expect("table still decoded");
    assert_eq!(table.pages[0].shared_reference_count, 3);
    assert!(table.pages[0].shared_object_ids.is_empty());
    assert_eq!(table.pages[1].shared_object_ids, vec![1, 5]);
    // 首页的违规值不改变后续家族的可读性。
    assert!(hints.shared_object.is_some());
}

#[test]
fn out_of_range_declared_bit_width_is_reported() {
    // Table F.3 位宽项只允许 0—32；声明 40 位时照声明解析并如实上报。
    let mut w = BitWriter::default();
    for _ in 0..2 {
        w.unsigned(1, 32); // 项 1、项 2
    }
    w.unsigned(40, 16); // 项 3 → 越界
    w.unsigned(1, 32);
    w.unsigned(0, 16);
    w.unsigned(1, 32);
    w.unsigned(0, 16);
    w.unsigned(1, 32);
    w.unsigned(0, 16);
    w.unsigned(0, 16); // 项 10
    w.unsigned(0, 16); // 项 11
    w.unsigned(0, 16); // 项 12
    w.unsigned(7, 16); // 项 13（分母值，不受 0—32 约束）
    for value in [3, 5] {
        w.unsigned(value, 40); // 项 1 列（两页 × 40 位）
    }
    let payload = w.bytes;
    let mut knobs = Knobs::default();
    crafted(&mut knobs, &payload, "");
    let document = open(&super::build(&knobs), "hint-width");
    let hints = document.linearization_hints().expect("io").expect("hints");
    assert!(
        hints.issues.iter().any(|issue| matches!(
            issue,
            LinearizationIssue::PageOffsetWidthOutOfRange { item, bits }
                if *item == "item 3" && *bits == 40
        )),
        "{:#?}",
        hints.issues
    );
    let table = hints.page_offset.expect("over-range width still decoded");
    assert_eq!(table.pages[0].object_count_delta, 3);
    assert_eq!(table.pages[1].object_count_delta, 5);
    assert_eq!(table.header.shared_position_denominator, 7);
    assert!(
        table
            .width_problems()
            .iter()
            .all(|(item, _)| *item != "item 13"),
        "item 13 is a value, not a width"
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
    assert!(hints.page_offset.is_none());
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
    let mut payload = page_offset_table_bits(0);
    payload.extend_from_slice(&w.bytes);
    let entries = hint_entries();
    let mut knobs = Knobs::default();
    crafted(&mut knobs, &payload, &entries);
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
    let payload = hint_payload(0);
    let entries = hint_entries();
    let knobs = Knobs {
        overflow_hint: true,
        hint_payload: Some(&payload),
        hint_entries: &entries,
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
    assert!(hints.page_offset.is_some());
}
