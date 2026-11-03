//! 附录 F.4.4 generic hint 表的集成验证：在 [`super`] 的分部装配器上，把五张
//! Table F.9 的对象组（Table F.2 键 `O`/`A`/`E`/`I`/`L`，各 4 项 × 32 位 = 16
//! 字节）与页偏移表、共享对象表按**真实字节相邻**排进主 hint 流，再从
//! `Document::linearization_hints()` 解析回类型化数据；畸形形态一律给类型化
//! 诊断而非 panic。
//!
//! 本文件是 `tests/main/linearization_hints.rs` 的子模块（同目录即可，无须改
//! `tests/main/mod.rs`），复用父模块的位写手与两轮收敛的布局迭代。

use super::{
    BitWriter, build, hint_entries, hint_payload, page_offset_bytes, shared_object_table_bits,
};
use crate::support::parse_buf;
use pdf_engine::core::reader::linearization::LinearizationIssue;
use pdf_engine::core::reader::linearization::hints::GenericHintTable;
use pdf_engine::core::reader::linearization::hints::bits::HintBitError;

/// Table F.9 一张表的字节长：4 项 × 32 位 = 128 位（35801—35805 的定宽）。
const GENERIC_TABLE_BYTES: usize = 16;

/// F.4.4 用 generic 形态承载的五个 Table F.2 键，按名册里的键序排列；`seed`
/// 让四项在断言里可逐项归位。
const GENERIC_KEYS: [(&str, u64); 5] = [("O", 0), ("A", 1), ("E", 2), ("I", 3), ("L", 4)];

/// 项 1/2/3/4 的期望值（组首对象号 / 组首位置 / 组内对象数 / 组长度）。
fn expected_items(seed: u64) -> (u64, u64, u64, u64) {
    (100 + seed, 20_000 + seed, 2 + seed, 500 + seed)
}

/// 位级装配一张 Table F.9：四项定宽 32 位，顺序严格按表的项号。
fn generic_group_bits(w: &mut BitWriter, seed: u64) {
    let (number, location, objects, length) = expected_items(seed);
    w.unsigned(number, 32); // 项 1：组内第一个对象的对象号
    w.unsigned(location, 32); // 项 2：组内第一个对象的位置
    w.unsigned(objects, 32); // 项 3：组内对象数
    w.unsigned(length, 32); // 项 4：对象组的长度（字节）
}

/// 五张 generic 表首尾相接的字节串（80 字节）。
fn five_generic_tables_bits() -> Vec<u8> {
    let mut w = BitWriter::default();
    for (_, seed) in GENERIC_KEYS {
        generic_group_bits(&mut w, seed);
    }
    w.finish()
}

/// 页偏移表 → 五张 generic 表 → 共享对象表，三段字节相邻（F.4 前言：各表以
/// 字节边界开始且互不重叠，故相邻位置即后一张表的下界）。
fn generic_payload(tables: &[u8]) -> Vec<u8> {
    let mut payload = page_offset_table();
    payload.extend_from_slice(tables);
    payload.extend(shared_object_table_bits());
    payload
}

/// 页偏移表的字节（恒在流偏移 0，长度即后一张表的最小合法位置）。
fn page_offset_table() -> Vec<u8> {
    use super::page_offset_table_bits;
    page_offset_table_bits(0)
}

/// Table F.2 键位条目：五个 generic 键依次紧跟页偏移表，`S` 紧跟最后一张表。
fn generic_entries(tables_len: usize) -> String {
    let mut entries = String::new();
    for (index, (key, _)) in GENERIC_KEYS.iter().enumerate() {
        entries.push_str(&format!(
            " /{} {}",
            key,
            page_offset_bytes() + index * GENERIC_TABLE_BYTES
        ));
    }
    let shared = page_offset_bytes() + tables_len;
    format!("{entries} /S {shared}")
}

#[test]
fn handbuilt_linearized_file_exposes_five_generic_hint_tables() {
    let tables = five_generic_tables_bits();
    let payload = generic_payload(&tables);
    let entries = generic_entries(tables.len());
    let bytes = build(&payload, &entries);
    let document = parse_buf(&bytes);
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

    // 五个键全部按 Table F.2 条目取位，且相邻排布：后一张表的位置＝前一张的末字节。
    assert_eq!(hints.generic.len(), 5, "≥2 个对象组：本文件给出五组");
    let mut previous_end = page_offset_bytes() as u64;
    for (index, (key, seed)) in GENERIC_KEYS.iter().enumerate() {
        let start = (page_offset_bytes() + index * GENERIC_TABLE_BYTES) as u64;
        assert_eq!(previous_end, start, "generic 表按名册键序字节相邻");
        assert_eq!(
            hints.position_of(key),
            Some(start),
            "Table F.2 键 {key} 的取位"
        );
        let table = hints.generic_group_of(key).unwrap_or_else(|| {
            panic!(
                "key {key} shall yield a typed object group; issues: {:?}",
                hints.issues
            )
        });
        let (number, location, objects, length) = expected_items(*seed);
        // 四项与位序逐项对应（项号 1→2→3→4，各 32 位、高位在前）。
        assert_eq!(table.key, *key);
        assert_eq!(table.start_byte, start);
        assert_eq!(table.first_object_number, number, "项 1 组首对象号");
        assert_eq!(table.first_object_location, location, "项 2 组首位置");
        assert_eq!(table.object_count, objects, "项 3 组内对象数");
        assert_eq!(table.group_length, length, "项 4 组长度（字节）");
        assert_eq!(table.bits_consumed, 128, "Table F.9 恒 4×32 位");
        assert_eq!(table.object_range(), Some((number, number + objects - 1)));
        let end = start + GENERIC_TABLE_BYTES as u64;
        assert_eq!(table.end_byte(), end);
        assert_eq!(hints.generic_end_byte(key), Some(end));
        previous_end = end;
    }

    // 最后一张 generic 表的末字节恰是 `S` 的合法起点：三家互不重叠。
    assert_eq!(hints.position_of("S"), Some(previous_end));
    assert_eq!(hints.stream_bytes, payload.len());
    // 前两家与 `T` 不受牵连：`T` 缺失是合法形态，缩略图表为 `None` 且无诊断。
    assert!(hints.page_offset.is_some());
    assert!(hints.shared_object.is_some());
    assert!(hints.thumbnail.is_none());
    assert!(hints.generic_group_of("T").is_none());

    // 组内位置是文件偏移，按 F.4 前言 NOTE 3 修正（不大于主流起点者补流长）。
    let table = hints
        .generic_group_of("E")
        .expect("named destination group");
    let adjusted = hints.adjust_position(table.first_object_location);
    assert!(table.first_object_location > hints.primary.offset);
    assert_eq!(
        adjusted,
        table.first_object_location + hints.primary.length,
        "generic 表的项 2 同样要吃 NOTE 3 的修正"
    );

    assert_eq!(document.page_count().expect("page count"), 2);
    assert!(document.warnings().is_empty());
}

#[test]
fn absent_generic_keys_are_none_rather_than_empty_groups() {
    // 只有 `S` 的载荷（`hint_entries`/`hint_payload` 的既有形态）：F.4.4 的五键
    // 全是条件存在，缺键既不给位置、也不给诊断，取组退回 `None` 而非空组。
    let payload = hint_payload(0);
    let entries = hint_entries();
    let bytes = build(&payload, &entries);
    let document = parse_buf(&bytes);
    let hints = document
        .linearization_hints()
        .expect("no I/O error")
        .expect("hint stream present");
    assert!(hints.is_consistent(), "{:#?}", hints.issues);
    assert!(hints.generic.is_empty(), "{:?}", hints.generic);
    for (key, _) in GENERIC_KEYS {
        assert_eq!(hints.position_of(key), None, "缺键 {key} 不应有位置");
        assert!(hints.generic_group_of(key).is_none(), "缺键 {key} 不是空组");
        assert_eq!(hints.generic_end_byte(key), None);
    }
    // 位置表里只列出现过的键。
    assert_eq!(
        hints
            .table_positions
            .iter()
            .map(|entry| entry.key.as_str())
            .collect::<Vec<_>>(),
        vec!["S"]
    );
    assert!(hints.shared_object.is_some());
}

#[test]
fn zero_object_generic_group_is_kept_without_an_issue() {
    // 项 3 声明 0 个对象：F.4.4 没有「组须至少一个对象」的 shall，故原值保留、
    // 只让推出的号区间退回 `None`，且不产生任何诊断。
    let mut w = BitWriter::default();
    w.unsigned(400, 32); // 项 1
    w.unsigned(30_000, 32); // 项 2
    w.unsigned(0, 32); // 项 3：空组
    w.unsigned(0, 32); // 项 4
    let tables = w.finish();
    assert_eq!(tables.len(), GENERIC_TABLE_BYTES);
    let payload = generic_payload(&tables);
    // 只声明 `I` 一个键，其余四键缺席（各自的合法形态）。
    let start = page_offset_bytes();
    let entries = format!(" /I {start} /S {}", start + tables.len());
    let document = parse_buf(&build(&payload, &entries));
    let hints = document
        .linearization_hints()
        .expect("no I/O error")
        .expect("hint stream present");
    assert!(hints.is_consistent(), "{:#?}", hints.issues);
    assert_eq!(hints.generic.len(), 1);
    let table = hints.generic_group_of("I").expect("declared empty group");
    assert_eq!(table.object_count, 0);
    assert_eq!(table.group_length, 0);
    assert_eq!(table.object_range(), None, "空组没有对象号区间");
    assert_eq!(table.first_object_number, 400);
    assert_eq!(hints.position_of("O"), None);
    assert!(hints.shared_object.is_some());
}

#[test]
fn truncated_generic_table_is_a_typed_issue_not_a_panic() {
    // 截在 `O` 的 16 字节表体内（1/8/15 字节）：本键读不满 4×32 位，给
    // GenericHintTableCorrupt；恰好截在 `O` 的末字节（16 字节）时本表完整，
    // 读不满的责任转给下一张表 `A`（其起点恰在流尾）。
    let tables = five_generic_tables_bits();
    let full = generic_payload(&tables);
    for (cut, key) in [(1usize, "O"), (8, "O"), (15, "O"), (16, "A")] {
        let prefix_len = page_offset_bytes() + cut;
        let document = parse_buf(&build(&full[..prefix_len], &generic_entries(tables.len())));
        let hints = document
            .linearization_hints()
            .expect("no I/O error")
            .expect("hint stream present");
        assert!(
            hints.issues.iter().any(|issue| matches!(
                issue,
                LinearizationIssue::GenericHintTableCorrupt { key: failed, problem }
                    if failed == key && problem.contains("bit stream truncated")
            )),
            "cut {cut}: {:#?}",
            hints.issues
        );
        assert!(
            hints.generic_group_of(key).is_none(),
            "cut {cut}: 读不满 4×32 位不给空组、也不猜字节"
        );
        assert!(hints.page_offset.is_some(), "页偏移表不受牵连");
        assert_eq!(document.page_count().expect("page count"), 2);
    }
}

#[test]
fn position_inside_a_generic_table_is_a_typed_issue() {
    // `A` 声称的位置落进 `O` 的 16 字节表体内：表互不重叠，故该键没有可信的
    // 解析起点，须从位置表里剔除。
    let tables = five_generic_tables_bits();
    let payload = generic_payload(&tables);
    let o_start = page_offset_bytes();
    let inside = o_start + 4;
    let entries = format!(" /O {o_start} /A {inside} /S {}", o_start + tables.len());
    let document = parse_buf(&build(&payload, &entries));
    let hints = document
        .linearization_hints()
        .expect("no I/O error")
        .expect("hint stream present");
    assert!(
        hints.issues.iter().any(|issue| matches!(
            issue,
            LinearizationIssue::HintTablePositionInvalid { key, problem }
                if key == "A" && problem.contains("inside the generic hint table of key O")
        )),
        "{:#?}",
        hints.issues
    );
    assert_eq!(hints.position_of("A"), None, "落进表体里的位置须被剔除");
    assert!(hints.generic_group_of("A").is_none());
    let table = hints.generic_group_of("O").expect("本表仍按 `O` 正常解析");
    assert_eq!(table.start_byte, o_start as u64);
    assert_eq!(table.end_byte(), (o_start + GENERIC_TABLE_BYTES) as u64);
    // 剔除不牵连后续家族：`S` 落在两张表之外，照常解析。
    assert!(hints.shared_object.is_some());
    assert!(hints.page_offset.is_some());
}

#[test]
fn declared_item_width_beyond_32_is_rejected_by_the_public_parser() {
    // 位数类条目的硬约束（35524—35525）：0—32。generic 家族自身是定宽 32 位，
    // 故这条门只由**声明宽度**入口触及（F.4.5/F.4.6 的不可信声明宽度同走此门）。
    let tables = five_generic_tables_bits();
    for bits in [33u32, 40, 64, u32::MAX] {
        assert_eq!(
            GenericHintTable::parse_with_item_width(&tables, 0, "O", bits),
            Err(HintBitError::InvalidWidth {
                bits: u64::from(bits)
            }),
            "声明位宽 {bits} 须类型化拒绝，不做未定义行为"
        );
    }
    // 32 位是 Table F.9 的定宽，与分发器走的是同一条路径。
    let wide = GenericHintTable::parse_with_item_width(&tables, 0, "O", 32).expect("declared 32");
    let norm = GenericHintTable::parse(&tables, 0, "O").expect("Table F.9 shape");
    assert_eq!(wide, norm);
    // 装配层拿到的仍是定宽 32 位的解析结果：声明位宽门不改变任何既有行为。
    let payload = generic_payload(&tables);
    let document = parse_buf(&build(&payload, &generic_entries(tables.len())));
    let hints = document
        .linearization_hints()
        .expect("no I/O error")
        .expect("hint stream present");
    assert!(hints.is_consistent(), "{:#?}", hints.issues);
    assert_eq!(hints.generic[0].bits_consumed, 128);
    // 分发器走的就是同一个公开入口：对同一份流、同一个起点直接解析，结果逐项
    // 相等（起点取 Table F.2 键 `O` 的值，即页偏移表的末字节）。
    let from_stream =
        GenericHintTable::parse(&payload, page_offset_bytes(), "O").expect("Table F.9 at `O`");
    assert_eq!(hints.generic[0], from_stream);
}
