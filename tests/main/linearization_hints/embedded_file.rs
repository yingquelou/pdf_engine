//! 附录 F.4.6 内嵌文件流 hint 表（Table F.11 头部 + Table F.12 每组条目）的集成
//! 验证：在 [`super`] 的分部装配器上，把键 `B` 的表（头部 3×32 + 4×16 位，再加
//! **项主序**五列）与页偏移表、共享对象表按**真实字节相邻**排进主 hint 流，再从
//! `Document::linearization_hints()` 解析回类型化数据；畸形形态一律给类型化诊断
//! 而非 panic，且**不牵连**其余已解析的表。
//!
//! 另覆盖三条只有整篇文档才看得见的行为：F.4 前言「表互不重叠」对本表体的位置
//! 剔除、项 5 对**文档级** Table F.3 项 11 的硬依赖（页偏移表不可用时本表
//! **不类型化**，只落 `EmbeddedFileIdWidthUnavailable`），以及 Table F.12 项 2/
//! 项 3 互斥 shall 的类型化报告（原值保留、表照常给出）。
//!
//! 本文件是 `tests/main/linearization_hints` 的子模块（同目录即可，无须改
//! `tests/main/mod.rs`），复用父模块的位写手与两轮收敛的布局迭代。

use super::{
    BitWriter, build, hint_entries, hint_payload, page_offset_bytes, page_offset_table_bits,
    shared_object_table_bits,
};
use crate::support::parse_buf;
use pdf_engine::core::reader::linearization::LinearizationIssue;
use pdf_engine::core::reader::linearization::hints::{
    EmbeddedFileGroupEntry, EmbeddedFileHintTable,
};

/// Table F.11 项 1—3（首组首对象号 / 首组首位置 / 组数）。
const HEADER: (u64, u64, u64) = (300, 31_000, 2);

/// Table F.11 项 4—7 的存储值＝Table F.12 项 1—4 的列宽。
const WIDTHS: [u64; 4] = [9, 3, 11, 2];

/// 项 5 的列宽：文档级 Table F.3 项 11（脚手架的页偏移表声明 4 位）。
const DOCUMENT_ID_BITS: u32 = 4;

/// Table F.12 项 1—4 的各列（两组一列，项主序：先读完**所有组**的项 1……）。
const ITEM1: &[u64] = &[301, 320];
const ITEM2: &[u64] = &[2, 0];
const ITEM3: &[u64] = &[600, 0];
const ITEM4: &[u64] = &[1, 3];

/// 合规表的四列。
fn conforming_columns() -> [&'static [u64]; 4] {
    [ITEM1, ITEM2, ITEM3, ITEM4]
}

/// 项 5 的标识符扁平列表（组 0 一项、组 1 三项）。
const IDS: [u64; 4] = [2, 1, 0, 2];

/// 合规表的字节数：160 + (9+3+11+2)×2 + 4×4 = 226 位 → 29 字节。
const B_TABLE_BYTES: usize = 29;

/// 位级装配：头部 7 项 + **项主序**四列（35797—35799）+ 项 5 的标识符列表。
/// `widths` 直接写进 16 位存储字段，故可写畸形声明值。
fn b_table_bits(
    header: (u64, u64, u64),
    widths: [u64; 4],
    columns: &[&[u64]],
    ids: &[u64],
) -> Vec<u8> {
    let mut w = BitWriter::default();
    w.unsigned(header.0, 32);
    w.unsigned(header.1, 32);
    w.unsigned(header.2, 32);
    for raw in widths {
        w.unsigned(raw, 16);
    }
    for (index, bits) in widths.iter().enumerate() {
        for value in columns[index] {
            w.unsigned(*value, *bits as u32);
        }
    }
    for id in ids {
        w.unsigned(*id, DOCUMENT_ID_BITS);
    }
    w.finish()
}

/// 合规的两组表。
fn conforming_bits() -> Vec<u8> {
    b_table_bits(HEADER, WIDTHS, &conforming_columns(), &IDS)
}

/// 把一张（可能畸形的）表补到 [`B_TABLE_BYTES`] 槽位，保证后面的 `S` 起点不动。
fn slotted(table: &[u8]) -> Vec<u8> {
    assert!(
        table.len() <= B_TABLE_BYTES,
        "一张表越过槽位就会与 `S` 的表重叠，用例本身先失效"
    );
    let mut out = table.to_vec();
    out.resize(B_TABLE_BYTES, 0);
    out
}

/// 页偏移表 → 键 `B` 的表 → 共享对象表，三段字节相邻（F.4 前言：各表以字节边界
/// 开始且互不重叠，故相邻位置即后一张表的下界）。
fn b_payload(table: &[u8]) -> Vec<u8> {
    let mut payload = page_offset_table_bits(0, 2);
    payload.extend_from_slice(table);
    payload.extend(shared_object_table_bits());
    payload
}

/// `B` 与 `S` 的键位条目：`B` 紧跟页偏移表，`S` 紧跟 `B` 的槽位。
fn b_entries() -> String {
    format!(
        " /B {} /S {}",
        page_offset_bytes(),
        page_offset_bytes() + B_TABLE_BYTES
    )
}

#[test]
fn handbuilt_linearized_file_exposes_the_embedded_file_hint_table() {
    let table = conforming_bits();
    assert_eq!(table.len(), B_TABLE_BYTES, "226 位折成 29 字节的槽位");
    let document = parse_buf(&build(&b_payload(&table), &b_entries()));
    let info = document
        .linearization()
        .expect("no I/O error")
        .expect("recognized as linearized");
    assert!(info.is_consistent(), "structure issues: {:?}", info.issues);

    let hints = document
        .linearization_hints()
        .expect("no I/O error")
        .expect("hint stream present");
    assert!(hints.is_consistent(), "hint issues: {:?}", hints.issues);

    let start = page_offset_bytes() as u64;
    assert_eq!(hints.position_of("B"), Some(start), "键 B 的取位");
    let parsed = hints
        .embedded_file
        .as_ref()
        .unwrap_or_else(|| panic!("键 B 须给出类型化表；issues: {:?}", hints.issues));
    // Table F.11 的 7 项逐项归位。
    assert_eq!(parsed.header.first_object_number, 300, "项 1 首组首对象号");
    assert_eq!(
        parsed.header.first_object_location, 31_000,
        "项 2 首组首位置"
    );
    assert_eq!(parsed.header.group_count, 2, "项 3 组数");
    assert_eq!(parsed.header.object_number_bits, 9, "项 4");
    assert_eq!(parsed.header.group_object_bits, 3, "项 5");
    assert_eq!(parsed.header.group_length_bits, 11, "项 6");
    assert_eq!(parsed.header.shared_ref_count_bits, 2, "项 7");
    assert_eq!(
        parsed.shared_object_id_bits, DOCUMENT_ID_BITS,
        "项 5 的宽度取文档级 Table F.3 项 11"
    );
    assert_eq!(parsed.start_byte, start);
    assert_eq!(
        parsed.bits_consumed, 226,
        "头部 160 + 四列 50 + 项 5 的 16 位"
    );
    assert_eq!(parsed.end_byte(), start + 29);
    assert_eq!(hints.embedded_file_end_byte(), Some(start + 29));
    assert!(parsed.groups_match_header(), "条目数恒等于头部项 3");

    // **项主序（转置）**的归位结果：第一组取各列的第一格，第二组取第二格。
    assert_eq!(
        parsed.groups,
        vec![
            EmbeddedFileGroupEntry {
                object_number: 301,
                object_count: 2,
                group_length: 600,
                shared_reference_count: 1,
                shared_object_ids: vec![2],
            },
            EmbeddedFileGroupEntry {
                object_number: 320,
                object_count: 0,
                group_length: 0,
                shared_reference_count: 3,
                shared_object_ids: vec![1, 0, 2],
            },
        ],
        "项 1 列读完才读项 2 列，逐组连续会读歪"
    );
    assert!(parsed.zero_item_problems().is_empty(), "两组的零项形态合法");
    assert_eq!(
        parsed.first_group_anchor_problem(),
        None,
        "301 落在 [300, 301] 内"
    );

    // 各家互不重叠：页偏移表末字节即 `B` 的起点，`B` 的末字节即 `S` 的起点。
    assert_eq!(hints.page_offset_end_byte(), Some(start));
    assert_eq!(hints.position_of("S"), Some(parsed.end_byte()));
    assert_eq!(
        hints.stream_bytes,
        start as usize + B_TABLE_BYTES + shared_object_table_bits().len()
    );
    assert!(hints.page_offset.is_some());
    assert!(hints.shared_object.is_some());
    assert!(hints.thumbnail.is_none());
    assert!(hints.generic.is_empty());
    assert!(hints.extended.is_empty());

    // 头部项 2 是文件偏移，按 F.4 前言 NOTE 3 修正。
    assert!(parsed.header.first_object_location > hints.primary.offset);
    assert_eq!(
        hints.adjust_position(parsed.header.first_object_location),
        parsed.header.first_object_location + hints.primary.length,
        "内嵌文件流表的组首位置同样要吃 NOTE 3 的修正"
    );
    assert_eq!(document.page_count().expect("page count"), 2);
    assert!(document.warnings().is_empty());
}

#[test]
fn absent_embedded_file_key_is_none_rather_than_an_empty_table() {
    // 只有 `S` 的载荷（脚手架的既有形态）：`B` 标「Present only if embedded file
    // streams exist」，缺键既不给位置、也不给诊断，字段退回 `None` 而非空表。
    let payload = hint_payload(0, 2);
    let document = parse_buf(&build(&payload, &hint_entries()));
    let hints = document
        .linearization_hints()
        .expect("no I/O error")
        .expect("hint stream present");
    assert!(hints.is_consistent(), "{:#?}", hints.issues);
    assert!(hints.embedded_file.is_none(), "缺键不是空表");
    assert_eq!(hints.position_of("B"), None, "缺键 B 不应有位置");
    assert_eq!(hints.embedded_file_end_byte(), None);
    assert_eq!(
        hints
            .table_positions
            .iter()
            .map(|entry| entry.key.as_str())
            .collect::<Vec<_>>(),
        vec!["S"]
    );
    assert!(hints.page_offset.is_some());
    assert!(hints.shared_object.is_some());
}

#[test]
fn embedded_file_parser_is_reachable_from_the_public_api_with_the_same_bits() {
    // 装配层拿到的解析结果与直接调用公开入口逐项相等：同一份流、同一个起点。
    let table = conforming_bits();
    let payload = b_payload(&table);
    let start = page_offset_bytes();
    let from_stream = EmbeddedFileHintTable::parse(&payload, start, DOCUMENT_ID_BITS)
        .expect("Table F.11/F.12 at `B`");
    let document = parse_buf(&build(&payload, &b_entries()));
    let hints = document
        .linearization_hints()
        .expect("no I/O error")
        .expect("hint stream present");
    assert_eq!(hints.embedded_file.as_ref(), Some(&from_stream));
    // 公开模型的字段可直接读回（写端未实现，故这里只做读端往返）。
    assert_eq!(from_stream.groups[1].shared_object_ids, vec![1, 0, 2]);
    assert_eq!(from_stream.header.shared_ref_count_bits, 2);
}

#[test]
fn embedded_file_table_truncated_at_the_stream_end_is_a_typed_issue() {
    // 把 `B` 的表放到流尾并只写头部加两列半：项主序的第三列越过流尾 ⇒ 整表拒绝，
    // 其余表照旧。
    let mut payload = hint_payload(0, 2);
    let shared_end = payload.len();
    let truncated = b_table_bits(HEADER, WIDTHS, &[ITEM1, ITEM2, &[], &[]], &[]);
    assert_eq!(truncated.len(), 23, "181 位：头部 + 项 1/项 2 两列");
    payload.extend_from_slice(&truncated);
    let entries = format!(" /S {} /B {}", page_offset_bytes(), shared_end);
    let document = parse_buf(&build(&payload, &entries));
    let hints = document
        .linearization_hints()
        .expect("no I/O error")
        .expect("hint stream present");
    assert!(
        hints.issues.iter().any(|issue| matches!(
            issue,
            LinearizationIssue::EmbeddedFileHintTableCorrupt { problem }
                if problem.contains("bit stream truncated")
        )),
        "{:#?}",
        hints.issues
    );
    assert!(hints.embedded_file.is_none(), "读不满不给半截表");
    assert!(hints.page_offset.is_some());
    assert!(hints.shared_object.is_some());
    assert!(hints.extended.is_empty());
}

#[test]
fn embedded_file_huge_group_count_is_rejected_before_allocating() {
    // 头部项 3 是不可信 32 位计数：声明 100 万组、每组项 1—4 共 25 位 → 需 2500 万
    // 位，远超本表实际占位。分配前按位预算封顶 → 类型化诊断，不挂死、不 OOM。
    let table = slotted(&b_table_bits(
        (300, 31_000, 1_000_000),
        WIDTHS,
        &conforming_columns(),
        &IDS,
    ));
    let document = parse_buf(&build(&b_payload(&table), &b_entries()));
    let hints = document
        .linearization_hints()
        .expect("no I/O error")
        .expect("hint stream present");
    assert!(
        hints.issues.iter().any(|issue| matches!(
            issue,
            LinearizationIssue::EmbeddedFileHintTableCorrupt { problem }
                if problem.contains("bit stream truncated")
        )),
        "{:#?}",
        hints.issues
    );
    assert!(hints.embedded_file.is_none());
    assert!(hints.page_offset.is_some());
    assert!(hints.shared_object.is_some(), "本键坏不牵连别家");
}

#[test]
fn embedded_file_over_range_declared_width_is_a_typed_issue() {
    // 头部项 4 声明 33 位：越出位数类条目的 0—32（35524—35525）→ 类型化拒绝；
    // 拒绝发生在读取各列之前，故本表实际占位无关紧要。
    let table = slotted(&b_table_bits(
        HEADER,
        [33, 3, 11, 2],
        &[ITEM1, &[], &[], &[]],
        &[],
    ));
    let document = parse_buf(&build(&b_payload(&table), &b_entries()));
    let hints = document
        .linearization_hints()
        .expect("no I/O error")
        .expect("hint stream present");
    assert!(
        hints.issues.iter().any(|issue| matches!(
            issue,
            LinearizationIssue::EmbeddedFileHintTableCorrupt { problem }
                if problem.contains("field width 33")
        )),
        "{:#?}",
        hints.issues
    );
    assert!(hints.embedded_file.is_none());
    assert!(hints.page_offset.is_some());
    assert!(hints.shared_object.is_some());
}

#[test]
fn embedded_file_shared_only_group_shall_is_a_derived_typed_issue() {
    // 单组、项 2 = 0 而项 3 = 600：违反 35837—35840 的 shall（项 2 为 0 时项 3
    // 须为 0、项 4 须大于 0）。原值一律保留、表照常给出，只加类型化报告。
    let shared_only_columns: [&[u64]; 4] = [&[301], &[0], &[600], &[1]];
    let table = slotted(&b_table_bits(
        (300, 31_000, 1),
        WIDTHS,
        &shared_only_columns,
        &[2],
    ));
    let document = parse_buf(&build(&b_payload(&table), &b_entries()));
    let hints = document
        .linearization_hints()
        .expect("no I/O error")
        .expect("hint stream present");
    assert!(
        hints.issues.iter().any(|issue| matches!(
            issue,
            LinearizationIssue::EmbeddedFileSharedOnlyGroupViolation {
                group: 0,
                object_count: 0,
                group_length: 600,
                shared_reference_count: 1,
                ..
            }
        )),
        "{:#?}",
        hints.issues
    );
    let parsed = hints.embedded_file.as_ref().expect("畸形的零项仍给出原值");
    assert_eq!(parsed.groups[0].group_length, 600, "原值不折算、不改写");
    assert_eq!(
        parsed.groups[0].shared_object_ids,
        vec![2],
        "项 5 的长度与项 4 一致"
    );
    assert!(!hints.is_consistent());
    // 表体长度只按实消费位数折算：补齐的槽位尾部不属于本表，故 `S` 的起点在其后。
    assert_eq!(parsed.bits_consumed, 160 + 9 + 3 + 11 + 2 + 4);
    assert!(
        parsed.end_byte() < hints.position_of("S").unwrap_or_default(),
        "本表 24 字节、槽位 29 字节：`S` 仍在表体之外，互不重叠"
    );
}

#[test]
fn position_inside_the_embedded_file_table_is_pruned() {
    // `S` 声称的位置落进 `B` 的 29 字节表体内：表互不重叠，故该键没有可信的解析
    // 起点，须从位置表里剔除；`B` 自身与页偏移表都不受牵连。
    let table = slotted(&conforming_bits());
    let inside = page_offset_bytes() + 12;
    let entries = format!(" /B {} /S {inside}", page_offset_bytes());
    let document = parse_buf(&build(&b_payload(&table), &entries));
    let hints = document
        .linearization_hints()
        .expect("no I/O error")
        .expect("hint stream present");
    assert!(
        hints.issues.iter().any(|issue| matches!(
            issue,
            LinearizationIssue::HintTablePositionInvalid { key, problem }
                if key == "S" && problem.contains("inside the embedded file stream hint table")
        )),
        "{:#?}",
        hints.issues
    );
    assert_eq!(hints.position_of("S"), None, "落进表体里的位置须被剔除");
    assert!(hints.shared_object.is_none());
    let parsed = hints.embedded_file.as_ref().expect("`B` 自身仍按声明解析");
    assert_eq!(
        parsed.end_byte(),
        (page_offset_bytes() + B_TABLE_BYTES) as u64
    );
    // 没有 F.4.2 的条目数就不做下标核对（无从判定，绝不编造边界）。
    assert!(
        !hints.issues.iter().any(|issue| matches!(
            issue,
            LinearizationIssue::EmbeddedFileSharedObjectIndexOutOfRange { .. }
        )),
        "{:#?}",
        hints.issues
    );
    assert!(hints.page_offset.is_some());
}

#[test]
fn embedded_file_shared_index_out_of_range_is_a_typed_issue() {
    // 项 5 的标识符是 F.4.2 共享对象 hint 表里自 0 起的顺序位置（35604），该表在本
    // 文件里只有 3 个条目；声明 5 即越界。表照常给出、原值保留。
    let out_of_range_columns: [&[u64]; 4] = [&[301], &[2], &[600], &[2]];
    let table = slotted(&b_table_bits(
        (300, 31_000, 1),
        WIDTHS,
        &out_of_range_columns,
        &[5, 1],
    ));
    let document = parse_buf(&build(&b_payload(&table), &b_entries()));
    let hints = document
        .linearization_hints()
        .expect("no I/O error")
        .expect("hint stream present");
    assert!(
        hints.issues.iter().any(|issue| matches!(
            issue,
            LinearizationIssue::EmbeddedFileSharedObjectIndexOutOfRange {
                group: 0,
                index: 5,
                entries: 3,
            }
        )),
        "{:#?}",
        hints.issues
    );
    let parsed = hints.embedded_file.as_ref().expect("越界标识符仍给出原值");
    assert_eq!(parsed.groups[0].shared_object_ids, vec![5, 1]);
    assert_eq!(
        parsed.groups[0].shared_reference_count, 2,
        "项 4 与列表长度一致"
    );
}

#[test]
fn embedded_file_without_the_document_identifier_width_is_not_typed() {
    // 项 5 的宽度只有文档级 Table F.3 项 11 一个来源：把页偏移表的项 3 声明成
    // 100 位（越过 64 位原语上限）使本表读坏，键 `B` 的表便**不类型化**，只落
    // EmbeddedFileIdWidthUnavailable——绝不以 0 宽冒充标识符。
    let mut w = BitWriter::default();
    w.unsigned(3, 32); // 项 1
    w.unsigned(700, 32); // 项 2
    w.unsigned(100, 16); // 项 3：畸形的位宽（本题里唯一被改的项）
    for (value, bits) in [
        (300u64, 32u32),
        (6, 16),
        (20, 32),
        (5, 16),
        (50, 32),
        (8, 16),
    ] {
        w.unsigned(value, bits);
    }
    // 项 10—13（各 16 位）：共享引用数 2、文档级标识符宽 4、分子 3、分母 4。
    for value in [2u64, 4, 3, 4] {
        w.unsigned(value, 16);
    }
    w.unsigned(0, 64); // 逐页条目的占位：100 位的列一开工就失败
    let broken_page_offset = w.finish();
    assert_eq!(
        broken_page_offset.len(),
        page_offset_bytes(),
        "槽位与合规页偏移表同宽"
    );
    let mut payload = broken_page_offset;
    payload.extend_from_slice(&slotted(&conforming_bits()));
    payload.extend(shared_object_table_bits());
    let document = parse_buf(&build(&payload, &b_entries()));
    let hints = document
        .linearization_hints()
        .expect("no I/O error")
        .expect("hint stream present");
    assert!(
        hints
            .issues
            .contains(&LinearizationIssue::EmbeddedFileIdWidthUnavailable),
        "{:#?}",
        hints.issues
    );
    assert!(hints.page_offset.is_none(), "本用例的畸形点在页偏移表");
    assert!(
        hints.embedded_file.is_none(),
        "没有文档级宽度就不给类型化表，不给伪 0 标识符"
    );
    assert_eq!(hints.embedded_file_end_byte(), None);
    assert!(hints.shared_object.is_some(), "S 的表照常可读");
}
