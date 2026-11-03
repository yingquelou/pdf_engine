//! 内嵌文件流 hint 表（F.4.6，Table F.11 头部 + Table F.12 每组条目）的位级用例。
//!
//! 覆盖点：头部 7 项逐项归位与 160 位定宽、Table F.12 的**项主序（转置）**读取
//! （35797—35799，本家族最易写错处，故同时反证「逐组连续」会读歪）、每个项边界
//! 的截断拒绝、声明位宽越出 0—32 的门（头部项 4—7 与文档级 Table F.3 项 11）、
//! 组数 0 的形态、项 2/项 3 的两条合法零形态与非法形态、项 4 与项 5 列表长度的
//! 一致性、不可信大计数（组数、标识符总数）在**任何分配之前**被位预算挡下，以及
//! `end_byte`/派生核对与键名册自证。

use super::super::embedded_file::{
    EMBEDDED_FILE_HINT_KEY, EmbeddedFileGroupEntry, EmbeddedFileHeader, EmbeddedFileHintTable,
    check_embedded_file_semantics,
};
use super::super::extended::EXTENDED_HINT_KEYS;
use super::super::generic::GENERIC_HINT_KEYS;
use super::super::page_offset::MAX_UNEVIDENCED_ITEMS;
use super::super::{HintRange, HintTablePosition, LinearizationHints, STANDARD_HINT_KEYS};
use crate::core::reader::linearization::hints::bits::{BitWriter, HintBitError};
use crate::core::reader::linearization::issues::LinearizationIssue;

/// Table F.11 项 1—3（各 32 位，35822—35824）。
const FIRST_OBJECT_NUMBER: u64 = 300;
const FIRST_OBJECT_LOCATION: u64 = 31_000;
const GROUP_COUNT: u64 = 2;

/// Table F.11 项 4—7（各 16 位存储，35825—35832）＝Table F.12 项 1—4 的列宽。
const WIDTHS: [u64; 4] = [9, 3, 11, 2];

/// 项 5 的宽度来源：**文档级** Table F.3 项 11（35847 的「See Table F.3, item 11」）。
const DOCUMENT_ID_BITS: u32 = 4;

/// Table F.12 的五个「列」，两组各一项（项主序：先把所有组的项 1 读完……）。
const ITEM1: [u64; 2] = [301, 320];
const ITEM2: [u64; 2] = [2, 0];
const ITEM3: [u64; 2] = [600, 0];
const ITEM4: [u64; 2] = [1, 3];

/// 项 5 的扁平列表：组 0 一项、组 1 三项（与 `ITEM4` 之和一致）。
const IDS: [u64; 4] = [2, 1, 0, 2];

/// 完整表的位数：头部 160 + 四列 (9+3+11+2)×2 + 项 5 的 4×4 = 226。
const TABLE_BITS: u64 = 160 + 50 + 16;

/// 写 Table F.11 的 7 项（项 4—7 按 16 位存储，故可写畸形声明值）。
fn header(w: &mut BitWriter, group_count: u64, widths: [u64; 4]) {
    w.unsigned(FIRST_OBJECT_NUMBER, 32);
    w.unsigned(FIRST_OBJECT_LOCATION, 32);
    w.unsigned(group_count, 32);
    for raw in widths {
        w.unsigned(raw, 16);
    }
}

/// 按**项主序（转置）**写 Table F.12 的五列（35797—35799）。
fn item_major(w: &mut BitWriter) {
    header(w, GROUP_COUNT, WIDTHS);
    for (index, bits) in WIDTHS.iter().enumerate() {
        for group in 0..GROUP_COUNT as usize {
            let value = match index {
                0 => ITEM1[group],
                1 => ITEM2[group],
                2 => ITEM3[group],
                _ => ITEM4[group],
            };
            w.unsigned(value, *bits as u32);
        }
    }
    for id in IDS {
        w.unsigned(id, DOCUMENT_ID_BITS);
    }
}

/// 只有表体、无尾字节：供逐项截断使用。
fn exact_table_bytes() -> Vec<u8> {
    let mut w = BitWriter::default();
    item_major(&mut w);
    w.bytes
}

/// 解析 `exact_table_bytes()`（起点 0、文档级宽度 4）。
fn well_formed() -> EmbeddedFileHintTable {
    EmbeddedFileHintTable::parse(&exact_table_bytes(), 0, DOCUMENT_ID_BITS)
        .expect("well-formed embedded file stream hint table")
}

/// 手工构造一张表（只关心语义核对时用，绕开位流）。
fn table_with(groups: Vec<EmbeddedFileGroupEntry>) -> EmbeddedFileHintTable {
    EmbeddedFileHintTable {
        header: EmbeddedFileHeader {
            first_object_number: FIRST_OBJECT_NUMBER,
            first_object_location: FIRST_OBJECT_LOCATION,
            group_count: groups.len() as u64,
            object_number_bits: WIDTHS[0] as u32,
            group_object_bits: WIDTHS[1] as u32,
            group_length_bits: WIDTHS[2] as u32,
            shared_ref_count_bits: WIDTHS[3] as u32,
        },
        groups,
        shared_object_id_bits: DOCUMENT_ID_BITS,
        start_byte: 0,
        bits_consumed: EmbeddedFileHeader::HEADER_BITS,
    }
}

fn group(
    object_number: u64,
    object_count: u64,
    group_length: u64,
    shared: Vec<u64>,
) -> EmbeddedFileGroupEntry {
    let shared_reference_count = shared.len() as u64;
    EmbeddedFileGroupEntry {
        object_number,
        object_count,
        group_length,
        shared_reference_count,
        shared_object_ids: shared,
    }
}

#[test]
fn embedded_file_hint_table_reads_the_seven_items_of_table_f11() {
    let table = well_formed();
    assert_eq!(
        table.header.first_object_number, FIRST_OBJECT_NUMBER,
        "项 1"
    );
    assert_eq!(
        table.header.first_object_location, FIRST_OBJECT_LOCATION,
        "项 2"
    );
    assert_eq!(table.header.group_count, GROUP_COUNT, "项 3");
    assert_eq!(table.header.object_number_bits, 9, "项 4");
    assert_eq!(table.header.group_object_bits, 3, "项 5");
    assert_eq!(table.header.group_length_bits, 11, "项 6");
    assert_eq!(table.header.shared_ref_count_bits, 2, "项 7");
    assert_eq!(
        table.shared_object_id_bits, DOCUMENT_ID_BITS,
        "项 5 的文档级宽度原样保留"
    );
    assert_eq!(table.start_byte, 0, "起点＝Table F.2 键 B 的值");
    assert_eq!(table.bits_consumed, TABLE_BITS, "位数按声明宽度实算");
    assert_eq!(table.end_byte(), 29, "226 位按 div_ceil(8) 折成 29 字节");
    assert!(table.groups_match_header(), "条目数恒等于头部项 3");
}

#[test]
fn embedded_file_hint_table_reads_table_f12_in_item_major_transposed_order() {
    let table = well_formed();
    assert_eq!(table.groups.len(), GROUP_COUNT as usize);
    assert_eq!(
        table.groups[0],
        EmbeddedFileGroupEntry {
            object_number: 301,
            object_count: 2,
            group_length: 600,
            shared_reference_count: 1,
            shared_object_ids: vec![2],
        },
        "第一组的项 1—5 取自各列的第一格"
    );
    assert_eq!(
        table.groups[1],
        EmbeddedFileGroupEntry {
            object_number: 320,
            object_count: 0,
            group_length: 0,
            shared_reference_count: 3,
            shared_object_ids: vec![1, 0, 2],
        },
        "第二组的项 1—5 取自各列的第二格（项主序）"
    );

    // 反证：同一组数据若按「逐组连续」写，位流就不是转置序，第二组的项 1 必然错位。
    let mut w = BitWriter::default();
    header(&mut w, GROUP_COUNT, WIDTHS);
    for group in 0..GROUP_COUNT as usize {
        w.unsigned(ITEM1[group], WIDTHS[0] as u32);
        w.unsigned(ITEM2[group], WIDTHS[1] as u32);
        w.unsigned(ITEM3[group], WIDTHS[2] as u32);
        w.unsigned(ITEM4[group], WIDTHS[3] as u32);
    }
    for id in IDS {
        w.unsigned(id, DOCUMENT_ID_BITS);
    }
    w.unsigned(0, 32);
    let wrong = EmbeddedFileHintTable::parse(&w.bytes, 0, DOCUMENT_ID_BITS)
        .expect("the group-major stream is still within the bit budget");
    assert_eq!(
        wrong.groups[0].object_number, 301,
        "转置序的第一格与逐组序相同"
    );
    assert_ne!(
        wrong.groups[1].object_number, 320,
        "逐组连续不是 35797—35799 要求的序，读端按列消费故第二格必然错位"
    );
}

#[test]
fn embedded_file_hint_table_rejects_truncation_at_every_item_boundary() {
    let full = exact_table_bytes();
    // 头部 32/32/32/16/16/16/16 与四列（18/6/22/4 位）、项 5 的每个标识符。
    let boundaries: [u64; 15] = [
        0, 32, 64, 96, 112, 128, 144, 160, 178, 184, 206, 210, 214, 218, 222,
    ];
    for bits in boundaries {
        let end = usize::try_from(bits.div_ceil(8)).expect("boundary inside the fixture");
        let problem = EmbeddedFileHintTable::parse(&full[..end], 0, DOCUMENT_ID_BITS)
            .expect_err("every item boundary short of the full table must be rejected");
        assert!(
            matches!(problem, HintBitError::ShortBuffer { .. }),
            "截到 {bits} 位应给 ShortBuffer，实际 {problem:?}"
        );
    }
}

#[test]
fn embedded_file_hint_table_rejects_declared_widths_above_the_gate() {
    for slot in 0..4 {
        let mut widths = WIDTHS;
        // 16 位存储字段容得下 33，正是要测的畸形声明值（35524—35525 的 0—32 之外）。
        widths[slot] = 33;
        let mut w = BitWriter::default();
        header(&mut w, GROUP_COUNT, widths);
        // 各列不必真读满：拒绝发生在读取列之前，这里只塞点占位字节。
        for value in ITEM1 {
            w.unsigned(value, 32);
        }
        let problem = EmbeddedFileHintTable::parse(&w.bytes, 0, DOCUMENT_ID_BITS)
            .expect_err("Table F.11 项 4—7 越出 0—32 必须类型化拒绝");
        assert_eq!(problem, HintBitError::InvalidWidth { bits: 33 });
    }
}

#[test]
fn embedded_file_hint_table_accepts_the_boundary_width_of_thirty_two() {
    let mut w = BitWriter::default();
    header(&mut w, 0, [32, 32, 32, 32]);
    let table = EmbeddedFileHintTable::parse(&w.bytes, 0, 32).expect("32 是门内边界");
    assert_eq!(
        table.header.shared_ref_count_bits, 32,
        "边界值保留为声明宽度"
    );
    assert!(table.groups.is_empty(), "项 3 为 0 ⇒ 没有条目");
    assert_eq!(table.bits_consumed, 160, "头部恒 3×32 + 4×16 位");
}

#[test]
fn embedded_file_hint_table_rejects_the_document_width_above_the_gate() {
    let problem = EmbeddedFileHintTable::parse(&exact_table_bytes(), 0, 33)
        .expect_err("项 5 的宽度同样受 0—32 门约束");
    assert_eq!(problem, HintBitError::InvalidWidth { bits: 33 });
    let problem = EmbeddedFileHintTable::parse(&exact_table_bytes(), 0, 64)
        .expect_err("64 位是原语上限，但 Table F.3 的门在 32");
    assert_eq!(problem, HintBitError::InvalidWidth { bits: 64 });
}

#[test]
fn embedded_file_hint_table_group_count_zero_types_an_empty_table_and_reports_it() {
    let mut w = BitWriter::default();
    header(&mut w, 0, WIDTHS);
    let table = EmbeddedFileHintTable::parse(&w.bytes, 0, DOCUMENT_ID_BITS)
        .expect("0 组仍是 well-formed 的表体（键 B 已声明有表）");
    assert_eq!(table.groups, Vec::new(), "没有组就没有条目");
    assert!(table.groups_match_header(), "项 3 为 0 时条目数也须为 0");
    assert_eq!(table.end_byte(), 20, "只剩 160 位头部");
    let mut issues = Vec::new();
    check_embedded_file_semantics(Some(&table), None, &mut issues);
    assert_eq!(issues, vec![LinearizationIssue::EmbeddedFileGroupCountZero]);
}

#[test]
fn embedded_file_accepts_both_legal_zero_forms_and_flags_the_illegal_ones() {
    // 合法形态一：项 2 与项 3 同为 0，项 4 大于 0（只有共享对象引用）。
    let shared_only = group(400, 0, 0, vec![0, 1]);
    assert!(shared_only.zero_item_problems().is_empty());
    // 合法形态二：项 2、项 3 都非 0。
    let regular = group(301, 2, 600, vec![2]);
    assert!(regular.zero_item_problems().is_empty());
    // 非法：项 2 为 0 而项 3 非 0。
    let bad_item2 = group(301, 0, 600, vec![2]);
    assert_eq!(
        bad_item2.zero_item_problems(),
        vec!["Table F.12 item 2 is 0, so item 3 shall be 0 and item 4 shall be greater than zero"],
        "项 3 非 0 时只有项 2 那条 shall 被违反"
    );
    // 非法：项 3 为 0 而项 2 非 0（对称）。
    let bad_item3 = group(301, 2, 0, vec![2]);
    assert_eq!(bad_item3.zero_item_problems().len(), 1);
    assert!(
        bad_item3.zero_item_problems()[0].starts_with("Table F.12 item 3"),
        "报的是项 3 那条 shall"
    );
    // 非法：三项全 0 ⇒ 两条 shall 同时被违反。
    let all_zero = group(301, 0, 0, Vec::new());
    assert_eq!(all_zero.zero_item_problems().len(), 2);
}

#[test]
fn embedded_file_item_four_equals_the_length_of_the_item_five_list() {
    for entry in well_formed().groups {
        assert_eq!(
            entry.shared_object_ids.len() as u64,
            entry.shared_reference_count,
            "项 5 的列表长度由本组项 4 声明（35848—35850）"
        );
    }
    // 项 4 声明 3 个标识符却只写 1 个：列越过流尾 ⇒ 整表按位预算拒绝，
    // 不留「静默半截列」（35848—35850 的 shall）。
    let mut w = BitWriter::default();
    header(&mut w, 1, [9, 3, 11, 2]);
    w.unsigned(301, 9);
    w.unsigned(2, 3);
    w.unsigned(600, 11);
    w.unsigned(3, 2);
    w.unsigned(1, DOCUMENT_ID_BITS);
    let problem = EmbeddedFileHintTable::parse(&w.bytes, 0, DOCUMENT_ID_BITS)
        .expect_err("项 5 的列长与项 4 不符必须整体拒绝");
    assert!(matches!(problem, HintBitError::ShortBuffer { .. }));
}

#[test]
fn embedded_file_rejects_a_huge_group_count_before_allocating() {
    // 头部声明 2^32-1 组、条目宽度如常，但流里只有头部：位预算先挡下来。
    let mut w = BitWriter::default();
    header(&mut w, u32::MAX as u64, WIDTHS);
    let problem = EmbeddedFileHintTable::parse(&w.bytes, 0, DOCUMENT_ID_BITS)
        .expect_err("不可信组数不得进入分配");
    assert!(matches!(problem, HintBitError::ShortBuffer { .. }));

    // 四列宽度全为 0 时位预算给不出证据，改用绝对上限（库策略，同其余三家）。
    let mut w = BitWriter::default();
    header(&mut w, MAX_UNEVIDENCED_ITEMS + 1, [0, 0, 0, 0]);
    let problem = EmbeddedFileHintTable::parse(&w.bytes, 0, DOCUMENT_ID_BITS)
        .expect_err("0 宽度下的超大组数同样拒绝");
    assert!(matches!(problem, HintBitError::ShortBuffer { .. }));

    // 上限之下的 0 宽度表按声明读取：条目全为 0，绝不零消耗死循环。
    let mut w = BitWriter::default();
    header(&mut w, 3, [0, 0, 0, 0]);
    let table = EmbeddedFileHintTable::parse(&w.bytes, 0, DOCUMENT_ID_BITS).expect("3 组 0 宽度");
    assert_eq!(table.groups.len(), 3);
    assert_eq!(table.bits_consumed, 160, "0 宽度的列不消费位");
    for entry in &table.groups {
        assert_eq!(*entry, group(0, 0, 0, Vec::new()));
    }
}

#[test]
fn embedded_file_binds_the_identifier_column_before_allocating() {
    // 项 4 用满 32 位宽度声明 ~2^32 个标识符：先按位预算封顶，绝不 with_capacity。
    let mut w = BitWriter::default();
    header(&mut w, 1, [9, 3, 11, 32]);
    w.unsigned(301, 9);
    w.unsigned(2, 3);
    w.unsigned(600, 11);
    w.unsigned(u32::MAX as u64, 32);
    let problem = EmbeddedFileHintTable::parse(&w.bytes, 0, DOCUMENT_ID_BITS)
        .expect_err("超大标识符总数必须在分配前拒绝");
    assert!(matches!(problem, HintBitError::ShortBuffer { .. }));

    // 文档级宽度为 0 时列不占位，改用绝对上限。
    let mut w = BitWriter::default();
    header(&mut w, 1, [9, 3, 11, 32]);
    w.unsigned(301, 9);
    w.unsigned(2, 3);
    w.unsigned(600, 11);
    w.unsigned(MAX_UNEVIDENCED_ITEMS + 1, 32);
    let problem = EmbeddedFileHintTable::parse(&w.bytes, 0, 0).expect_err("0 宽度下的超大列长");
    assert!(matches!(problem, HintBitError::ShortBuffer { .. }));
}

#[test]
fn embedded_file_first_group_anchor_problem_is_derived_and_first_group_only() {
    // well-formed：头部项 1 = 300、第一组项 2 = 2 ⇒ 号区间 [300, 301]，项 1 = 301 在内。
    assert_eq!(well_formed().first_group_anchor_problem(), None);
    // 矛盾：项 1 落在区间外。
    let contradictory = table_with(vec![group(500, 2, 600, vec![0]), group(320, 0, 0, vec![1])]);
    assert_eq!(
        contradictory.first_group_anchor_problem(),
        Some((500, 300, 301)),
        "只核对第一组（Table F.12 不给其余组的组首号）"
    );
    // 项 2 为 0 的合法形态无从判定 ⇒ 退回头 None。
    let shared_only = table_with(vec![group(500, 0, 0, vec![0])]);
    assert_eq!(shared_only.first_group_anchor_problem(), None);
    // 没有条目时无从判定。
    assert_eq!(table_with(Vec::new()).first_group_anchor_problem(), None);
}

#[test]
fn embedded_file_shared_index_problems_use_the_f42_entry_count() {
    let table = well_formed();
    assert_eq!(
        table.shared_index_problems(3),
        Vec::new(),
        "标识符 < 3 都在界内"
    );
    assert_eq!(
        table.shared_index_problems(2),
        vec![(0, 2), (1, 2)],
        "两组都用到标识符 2，越出 2 条的共享对象表；每组只报最大值"
    );
    assert_eq!(
        table.shared_index_problems(1),
        vec![(0, 2), (1, 2)],
        "界内只有下标 0：越界者仍是各组的最大值 2"
    );
    assert_eq!(
        table.shared_index_problems(0).len(),
        2,
        "空表 ⇒ 两组各越界一次"
    );
}

#[test]
fn embedded_file_check_semantics_reports_every_typed_finding() {
    // 第一组：项 1—3 齐备但号越出相邻区间、标识符 3 越界；第二组：项 2 为 0 而
    // 项 3 非 0（违反 35837—35840），标识符 4 亦越界。
    let table = table_with(vec![
        group(500, 2, 600, vec![3]),
        group(320, 0, 600, vec![4]),
    ]);
    let mut issues = Vec::new();
    check_embedded_file_semantics(Some(&table), Some(3), &mut issues);
    assert_eq!(
        issues,
        vec![
            LinearizationIssue::EmbeddedFileSharedOnlyGroupViolation {
                group: 1,
                rule: "Table F.12 item 2 is 0, so item 3 shall be 0 and item 4 shall be greater than zero",
                object_count: 0,
                group_length: 600,
                shared_reference_count: 1,
            },
            LinearizationIssue::EmbeddedFileSharedObjectIndexOutOfRange {
                group: 0,
                index: 3,
                entries: 3,
            },
            LinearizationIssue::EmbeddedFileSharedObjectIndexOutOfRange {
                group: 1,
                index: 4,
                entries: 3,
            },
            LinearizationIssue::EmbeddedFileFirstGroupAnchorMismatch {
                object_number: 500,
                run_start: 300,
                run_end: 301,
            },
        ][..],
        "先逐组的零项 shall，再下标越界，最后派生的组首核对"
    );
    // 表缺席 ⇒ 什么都不报（缺键是合法形态，诊断由分派层给）。
    let mut issues = Vec::new();
    check_embedded_file_semantics(None, Some(3), &mut issues);
    assert_eq!(issues, Vec::new());
    // 没有 F.4.2 表 ⇒ 跳过下标核对，其余照报。
    let mut issues = Vec::new();
    check_embedded_file_semantics(Some(&table), None, &mut issues);
    assert_eq!(
        issues
            .iter()
            .filter(|issue| matches!(
                issue,
                LinearizationIssue::EmbeddedFileSharedObjectIndexOutOfRange { .. }
            ))
            .count(),
        0
    );
}

#[test]
fn embedded_file_table_keeps_its_start_and_chains_after_a_prefix() {
    let mut w = BitWriter::default();
    for index in 0..7u64 {
        w.unsigned(index, 8);
    }
    item_major(&mut w);
    w.unsigned(0, 8);
    let table =
        EmbeddedFileHintTable::parse(&w.bytes, 7, DOCUMENT_ID_BITS).expect("表体自前缀之后开始");
    assert_eq!(table.start_byte, 7, "起点＝Table F.2 键 B 的值");
    assert_eq!(table.end_byte(), 7 + 29, "长度按实消费位数折算");

    let hints = LinearizationHints {
        primary: HintRange {
            offset: 0,
            length: 0,
        },
        first_page_object: None,
        stream_bytes: 0,
        table_positions: vec![HintTablePosition {
            key: "B".into(),
            position: 7,
        }],
        page_offset: None,
        shared_object: None,
        thumbnail: None,
        generic: Vec::new(),
        extended: Vec::new(),
        embedded_file: Some(table),
        issues: Vec::new(),
    };
    assert_eq!(hints.embedded_file_end_byte(), Some(36));
    let hints = LinearizationHints {
        embedded_file: None,
        ..hints
    };
    assert_eq!(
        hints.embedded_file_end_byte(),
        None,
        "键缺席／未解析时不外露长度"
    );
}

#[test]
fn embedded_file_hint_key_is_b_and_disjoint_from_the_other_families() {
    assert_eq!(
        EMBEDDED_FILE_HINT_KEY, "B",
        "Table F.2 续栏 35381—35382 自证"
    );
    let roster = STANDARD_HINT_KEYS
        .iter()
        .find(|(key, _)| *key == EMBEDDED_FILE_HINT_KEY)
        .expect("名册须含 B");
    assert!(
        roster.1.contains("F.4.6") && roster.1.contains("embedded file stream"),
        "名册描述须锚到 F.4.6，避免与 generic/extended 的键混用"
    );
    assert!(!GENERIC_HINT_KEYS.contains(&"B"));
    assert!(!EXTENDED_HINT_KEYS.contains(&"B"));
    assert_eq!(STANDARD_HINT_KEYS.len(), 11, "Table F.2 的 11 个键");
}
