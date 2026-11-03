//! 附录 F.4.5 extended generic hint 表的集成验证：在 [`super`] 的分部装配器上，
//! 把三张 Table F.10 的对象组（Table F.2 键 `V`/`C`/`R`，项 1—6 共 176 位再加
//! 项 7 的变长标识符列）与页偏移表、共享对象表按**真实字节相邻**排进主 hint
//! 流，再从 `Document::linearization_hints()` 解析回类型化数据；畸形形态一律给
//! 类型化诊断而非 panic，且**不牵连**其余已解析的表。
//!
//! 另覆盖两条只有整篇文档才看得见的行为：F.4.5「As of PDF 1.5 … shall use the
//! extended generic format」（35781）的版本门（< 1.5 时三键退回 Table F.9 形态）
//! 与项 6 对 Table F.3 项 11 的**派生**一致性核对（非原文 shall）。
//!
//! 本文件是 `tests/main/linearization_hints` 的子模块（同目录即可，无须改
//! `tests/main/mod.rs`），复用父模块的位写手与两轮收敛的布局迭代。

use super::{
    BitWriter, build, build_at_version, page_offset_bytes, page_offset_table_bits,
    shared_object_table_bits,
};
use crate::support::parse_buf;
use pdf_engine::core::reader::linearization::LinearizationIssue;
use pdf_engine::core::reader::linearization::hints::ExtendedHintTable;
use pdf_engine::core::types::PdfVersion;

/// F.4.5 的 Table F.2 键（名册 35332—35333 的 `V`、35336—35337 的 `C`、
/// 35379—35380 的 `R`），`seed` 让各项在断言里逐项归位。
const EXTENDED_KEYS: [(&str, u64); 3] = [("V", 0), ("C", 1), ("R", 2)];

/// well-formed 表的槽位字节数：项 1—6（176 位）+ 3 个 4 位标识符（12 位）
/// = 188 位 → 24 字节。畸形表可以短于此，槽位保证各表起点仍互不重叠。
const EXTENDED_TABLE_BYTES: usize = 24;

/// 项 6 的声明位宽与项 5 的引用数（与脚手架的页偏移表项 11 = 4 位同宽，
/// 故合规形态不触发一致性诊断）。
const ID_BITS: u32 = 4;
const REFERENCES: u64 = 3;

/// 项 1—4 的期望值（组首对象号 / 组首位置 / 组内对象数 / 组长度）。
fn expected_items(seed: u64) -> (u64, u64, u64, u64) {
    (200 + seed, 21_000 + seed, 3 + seed, 700 + seed)
}

/// 项 7 起的标识符列（自 0 起的共享对象 hint 表下标，故一律落在 `id_bits` 内）。
fn expected_ids(seed: u64) -> Vec<u64> {
    vec![seed, seed + 1, 15]
}

/// 一般形态：项 1—4 取 [`expected_items`]，项 5 用 `count`、项 6 用 `id_bits`、
/// 项 7 起逐个写 `ids`（每项 `id_bits` 位）。
fn table_bits(seed: u64, count: u64, id_bits: u32, ids: &[u64]) -> Vec<u8> {
    let (number, location, objects, length) = expected_items(seed);
    let mut w = BitWriter::default();
    w.unsigned(number, 32); // 项 1：组内第一个对象的对象号
    w.unsigned(location, 32); // 项 2：组内第一个对象的位置
    w.unsigned(objects, 32); // 项 3：组内对象数
    w.unsigned(length, 32); // 项 4：对象组的长度（字节）
    w.unsigned(count, 32); // 项 5：共享对象引用数
    w.unsigned(u64::from(id_bits), 16); // 项 6：组内标识符位宽
    for id in ids {
        w.unsigned(*id, id_bits); // 项 7 起：逐个共享对象标识符
    }
    w.finish()
}

/// 合规表（项 5 = [`REFERENCES`]、项 6 = [`ID_BITS`]）。
fn well_formed(seed: u64) -> Vec<u8> {
    table_bits(seed, REFERENCES, ID_BITS, &expected_ids(seed))
}

/// 三张表按 [`EXTENDED_TABLE_BYTES`] 槽位排布。
fn slotted(tables: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    for table in tables {
        assert!(
            table.len() <= EXTENDED_TABLE_BYTES,
            "一张表越过槽位就会与后一张表重叠，用例本身先失效"
        );
        out.extend_from_slice(table);
        out.resize(out.len() + EXTENDED_TABLE_BYTES - table.len(), 0);
    }
    out
}

/// 页偏移表 → 三张 extended 表 → 共享对象表，三段字节相邻（F.4 前言：各表以
/// 字节边界开始且互不重叠，故相邻位置即后一张表的下界）。
fn extended_payload(tables: &[u8]) -> Vec<u8> {
    let mut payload = page_offset_table_bits(0);
    payload.extend_from_slice(tables);
    payload.extend(shared_object_table_bits());
    payload
}

/// 表槽位起点的字节偏移（`index` 对应 [`EXTENDED_KEYS`] 的下标）。
fn slot(index: usize) -> usize {
    page_offset_bytes() + index * EXTENDED_TABLE_BYTES
}

/// Table F.2 键位条目：`V`/`C`/`R` 依次紧跟页偏移表，`S` 紧跟最后一张表。
fn extended_entries() -> String {
    format!(
        " /V {} /C {} /R {} /S {}",
        slot(0),
        slot(1),
        slot(2),
        slot(3)
    )
}

/// 合规三张表的载荷与键位（多数用例共用）。
fn conforming() -> (Vec<u8>, String) {
    let tables = slotted(
        &EXTENDED_KEYS
            .iter()
            .map(|(_, seed)| well_formed(*seed))
            .collect::<Vec<_>>(),
    );
    (extended_payload(&tables), extended_entries())
}

#[test]
fn handbuilt_linearized_file_exposes_three_extended_hint_tables() {
    let (payload, entries) = conforming();
    let bytes = build(&payload, &entries);
    let document = parse_buf(&bytes);
    let info = document
        .linearization()
        .expect("no I/O error")
        .expect("recognized as linearized");
    assert!(info.is_consistent(), "structure issues: {:?}", info.issues);
    assert_eq!(
        document.source_version(),
        PdfVersion::V1_5,
        "F.4.5 的版本门按声明版本取"
    );

    let hints = document
        .linearization_hints()
        .expect("no I/O error")
        .expect("hint stream present");
    assert!(hints.is_consistent(), "hint issues: {:?}", hints.issues);

    // 三个键全部按 Table F.2 条目取位，且按槽位相邻排布。
    assert_eq!(hints.extended.len(), 3, "本文件给出三组");
    let mut previous_end = page_offset_bytes() as u64;
    for (index, (key, seed)) in EXTENDED_KEYS.iter().enumerate() {
        let start = slot(index) as u64;
        assert_eq!(previous_end, start, "extended 表按名册键序字节相邻");
        assert_eq!(hints.position_of(key), Some(start), "键 {key} 的取位");
        let table = hints.extended_group_of(key).unwrap_or_else(|| {
            panic!(
                "key {key} shall yield a typed group; issues: {:?}",
                hints.issues
            )
        });
        let (number, location, objects, length) = expected_items(*seed);
        // 项 1—6 与位序逐项对应（项号 1→2→3→4→5→6，高位在前）。
        assert_eq!(table.key, *key);
        assert_eq!(table.start_byte, start);
        assert_eq!(table.first_object_number, number, "项 1 组首对象号");
        assert_eq!(table.first_object_location, location, "项 2 组首位置");
        assert_eq!(table.object_count, objects, "项 3 组内对象数");
        assert_eq!(table.group_length, length, "项 4 组长度（字节）");
        assert_eq!(table.shared_reference_count, REFERENCES, "项 5 引用数");
        assert_eq!(table.shared_object_id_bits, ID_BITS, "项 6 组内位宽");
        // 项 7 起按**项 6 的组内宽度**解包（模块头的待核裁决）。
        assert_eq!(table.shared_object_ids, expected_ids(*seed));
        assert_eq!(table.shared_object_ids.len() as u64, REFERENCES);
        assert_eq!(table.bits_consumed, 188, "176 位头部 + 3×4 位列");
        assert_eq!(table.object_range(), Some((number, number + objects - 1)));
        let end = start + EXTENDED_TABLE_BYTES as u64;
        assert_eq!(table.end_byte(), end);
        assert_eq!(hints.extended_end_byte(key), Some(end));
        previous_end = end;
    }

    // 最后一张 extended 表的末字节恰是 `S` 的合法起点：各家互不重叠。
    assert_eq!(hints.position_of("S"), Some(previous_end));
    assert_eq!(hints.stream_bytes, payload.len());
    // 其余家族不受牵连：`T` 缺失是合法形态，generic 五键同理。
    assert!(hints.page_offset.is_some());
    assert!(hints.shared_object.is_some());
    assert!(hints.thumbnail.is_none());
    assert!(hints.generic.is_empty());
    // 组内位置是文件偏移，按 F.4 前言 NOTE 3 修正（不大于主流起点者补流长）。
    let table = hints
        .extended_group_of("R")
        .expect("renditions name tree group");
    assert!(table.first_object_location > hints.primary.offset);
    assert_eq!(
        hints.adjust_position(table.first_object_location),
        table.first_object_location + hints.primary.length,
        "extended 表的项 2 同样要吃 NOTE 3 的修正"
    );
    assert_eq!(document.page_count().expect("page count"), 2);
    assert!(document.warnings().is_empty());
}

#[test]
fn absent_extended_keys_are_none_rather_than_empty_groups() {
    // 只有 `S` 的载荷（脚手架的既有形态）：F.4.5 的三键都是条件存在，缺键既
    // 不给位置、也不给诊断，取组退回 `None` 而非空组。
    let payload = super::hint_payload(0);
    let bytes = build(&payload, &super::hint_entries());
    let document = parse_buf(&bytes);
    let hints = document
        .linearization_hints()
        .expect("no I/O error")
        .expect("hint stream present");
    assert!(hints.is_consistent(), "{:#?}", hints.issues);
    assert!(hints.extended.is_empty(), "{:?}", hints.extended);
    for (key, _) in EXTENDED_KEYS {
        assert_eq!(hints.position_of(key), None, "缺键 {key} 不应有位置");
        assert!(
            hints.extended_group_of(key).is_none(),
            "缺键 {key} 不是空组"
        );
        assert_eq!(hints.extended_end_byte(key), None);
    }
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
fn pre_1_5_document_keeps_the_three_keys_on_the_generic_path() {
    // F.4.5 的版本门（35781「As of PDF 1.5」）：声明 1.4 的文件里 `V`/`C`/`R`
    // 按 Table F.9 的四项定宽读，落进 generic 集合，extended 集合为空。
    let mut w = BitWriter::default();
    for (index, _) in EXTENDED_KEYS.iter().enumerate() {
        let (number, location, objects, length) = expected_items(index as u64);
        w.unsigned(number, 32);
        w.unsigned(location, 32);
        w.unsigned(objects, 32);
        w.unsigned(length, 32);
    }
    let tables = w.finish();
    assert_eq!(tables.len(), 3 * 16, "Table F.9 恒 16 字节");
    // 槽位换成 generic 形态的 16 字节：`V`/`C`/`R` 各自的首字节与末字节。
    let generic_slot = |index: usize| page_offset_bytes() + index * 16;
    let mut payload = page_offset_table_bits(0);
    payload.extend_from_slice(&tables);
    payload.extend(shared_object_table_bits());
    let entries = format!(
        " /V {} /C {} /R {} /S {}",
        generic_slot(0),
        generic_slot(1),
        generic_slot(2),
        generic_slot(3)
    );
    let document = parse_buf(&build_at_version("1.4", &payload, &entries));
    assert_eq!(document.source_version(), PdfVersion::V1_4);
    let hints = document
        .linearization_hints()
        .expect("no I/O error")
        .expect("hint stream present");
    assert!(hints.is_consistent(), "{:#?}", hints.issues);
    assert!(
        hints.extended.is_empty(),
        "1.4 不认 extended 形态：{:?}",
        hints.extended
    );
    assert_eq!(hints.generic.len(), 3, "三键都走 Table F.9 形态");
    for (index, (key, _)) in EXTENDED_KEYS.iter().enumerate() {
        let table = hints
            .generic_group_of(key)
            .unwrap_or_else(|| panic!("key {key} shall yield a generic group"));
        let (number, location, objects, length) = expected_items(index as u64);
        assert_eq!(table.first_object_number, number, "键 {key} 的项 1");
        assert_eq!(table.first_object_location, location);
        assert_eq!(table.object_count, objects);
        assert_eq!(table.group_length, length);
        assert_eq!(table.bits_consumed, 128, "generic 形态恒 4×32 位");
        assert_eq!(
            hints.generic_end_byte(key),
            Some((generic_slot(index) + 16) as u64),
            "16 字节的槽位恰是后一张表的起点"
        );
    }
    assert!(hints.page_offset.is_some());
    assert!(hints.shared_object.is_some());
}

#[test]
fn uneconomic_reference_count_is_a_typed_issue_and_the_other_tables_stay_readable() {
    // 项 5 是不可信 32 位计数：声明 100 万个引用、项 6 = 4 位 → 需 400 万位，
    // 远超本表实际占位。分配前按位预算封顶 → ExtendedHintTableCorrupt，
    // 不挂死、不 OOM，且 `C`/`R`/`S` 与页偏移表照旧可读。
    let tables = slotted(&[
        table_bits(0, 1_000_000, ID_BITS, &expected_ids(0)),
        well_formed(1),
        well_formed(2),
    ]);
    let document = parse_buf(&build(&extended_payload(&tables), &extended_entries()));
    let hints = document
        .linearization_hints()
        .expect("no I/O error")
        .expect("hint stream present");
    assert!(
        hints.issues.iter().any(|issue| matches!(
            issue,
            LinearizationIssue::ExtendedHintTableCorrupt { key, problem }
                if key == "V" && problem.contains("bit stream truncated")
        )),
        "{:#?}",
        hints.issues
    );
    assert!(hints.extended_group_of("V").is_none(), "读不满不给半截列");
    assert_eq!(hints.extended.len(), 2, "后两张表不受牵连");
    for (key, seed) in EXTENDED_KEYS[1..].iter().copied() {
        let table = hints
            .extended_group_of(key)
            .expect("the other groups stay readable");
        assert_eq!(table.shared_object_ids, expected_ids(seed));
        assert_eq!(table.shared_reference_count, REFERENCES);
    }
    assert!(hints.page_offset.is_some());
    assert!(hints.shared_object.is_some());
    assert_eq!(document.page_count().expect("page count"), 2);
}

#[test]
fn declared_identifier_width_beyond_32_is_a_typed_issue() {
    // 项 6 声明 40 位：越出 0—32（35524—35525 类比，见 `hints::extended` 模块头）
    // → 类型化拒绝；本键不列条目，其余表照旧。项 7 的列不必写出——拒绝发生在
    // 读取该列之前，故本表仍只占头部的 22 字节。
    let tables = slotted(&[
        table_bits(0, REFERENCES, 40, &[]),
        well_formed(1),
        well_formed(2),
    ]);
    let document = parse_buf(&build(&extended_payload(&tables), &extended_entries()));
    let hints = document
        .linearization_hints()
        .expect("no I/O error")
        .expect("hint stream present");
    assert!(
        hints.issues.iter().any(|issue| matches!(
            issue,
            LinearizationIssue::ExtendedHintTableCorrupt { key, problem }
                if key == "V" && problem.contains("field width 40")
        )),
        "{:#?}",
        hints.issues
    );
    assert!(hints.extended_group_of("V").is_none());
    assert!(hints.extended_group_of("C").is_some());
    assert!(hints.extended_group_of("R").is_some());
    assert!(hints.shared_object.is_some());
}

#[test]
fn group_identifier_width_above_the_document_declaration_is_reported_as_derived_issue() {
    // 项 6（组内 5 位）超过 Table F.3 项 11（文档级 4 位，由脚手架的页偏移表
    // 声明）：这是**派生**一致性诊断、不是原文 shall；表仍按组内宽度读取。
    let tables = slotted(&[
        table_bits(0, REFERENCES, 5, &[1, 17, 31]),
        well_formed(1),
        well_formed(2),
    ]);
    let document = parse_buf(&build(&extended_payload(&tables), &extended_entries()));
    let hints = document
        .linearization_hints()
        .expect("no I/O error")
        .expect("hint stream present");
    assert!(
        hints.issues.iter().any(|issue| matches!(
            issue,
            LinearizationIssue::ExtendedHintIdWidthExceedsDocument {
                key,
                group_bits,
                document_bits
            } if key == "V" && *group_bits == 5 && *document_bits == 4
        )),
        "{:#?}",
        hints.issues
    );
    let table = hints
        .extended_group_of("V")
        .expect("declared width is honoured");
    assert_eq!(table.shared_object_id_bits, 5, "项 6 的原值保留");
    assert_eq!(table.shared_object_ids, vec![1, 17, 31], "按组内 5 位解包");
    assert_eq!(table.bits_consumed, 191);
    // 其余两家与合规形态一样：宽度 4 == 文档级 4，不触发诊断。
    assert_eq!(hints.extended.len(), 3);
    assert!(
        !hints.issues.iter().any(|issue| matches!(
            issue,
            LinearizationIssue::ExtendedHintIdWidthExceedsDocument { key, .. } if key == "C"
        )),
        "{:#?}",
        hints.issues
    );
    assert!(hints.shared_object.is_some());
    assert!(
        hints
            .page_offset
            .as_ref()
            .expect("page offset table parsed")
            .header
            .shared_object_id_bits
            == 4
    );
}

#[test]
fn position_inside_an_extended_table_is_a_typed_issue() {
    // `C` 声称的位置落进 `V` 的 24 字节表体内：表互不重叠，故该键没有可信的
    // 解析起点，须从位置表里剔除；`V`/`R`/`S` 都不受牵连。
    let tables = slotted(
        &EXTENDED_KEYS
            .iter()
            .map(|(_, seed)| well_formed(*seed))
            .collect::<Vec<_>>(),
    );
    let inside = slot(0) + 9;
    let entries = format!(" /V {} /C {inside} /R {} /S {}", slot(0), slot(2), slot(3));
    let document = parse_buf(&build(&extended_payload(&tables), &entries));
    let hints = document
        .linearization_hints()
        .expect("no I/O error")
        .expect("hint stream present");
    assert!(
        hints.issues.iter().any(|issue| matches!(
            issue,
            LinearizationIssue::HintTablePositionInvalid { key, problem }
                if key == "C" && problem.contains("inside the extended generic hint table of key V")
        )),
        "{:#?}",
        hints.issues
    );
    assert_eq!(hints.position_of("C"), None, "落进表体里的位置须被剔除");
    assert!(hints.extended_group_of("C").is_none());
    let table = hints.extended_group_of("V").expect("本表仍按 `V` 正常解析");
    assert_eq!(table.start_byte, slot(0) as u64);
    assert_eq!(table.end_byte(), (slot(0) + EXTENDED_TABLE_BYTES) as u64);
    assert!(hints.extended_group_of("R").is_some());
    assert!(hints.shared_object.is_some());
    assert!(hints.page_offset.is_some());
}

#[test]
fn extended_table_parser_is_reachable_from_the_public_api_with_the_same_bits() {
    // 装配层拿到的解析结果与直接调用公开入口逐项相等：同一份流、同一个起点。
    let (payload, _) = conforming();
    let from_stream = ExtendedHintTable::parse(&payload, slot(1), "C").expect("Table F.10 at `C`");
    let document = parse_buf(&build(&payload, &extended_entries()));
    let hints = document
        .linearization_hints()
        .expect("no I/O error")
        .expect("hint stream present");
    assert_eq!(hints.extended_group_of("C"), Some(&from_stream));
    assert_eq!(
        from_stream.shared_id_width_problem(4),
        None,
        "组内 4 位与文档级 4 位一致"
    );
    assert_eq!(from_stream.shared_id_width_problem(3), Some((4, 3)));
    // 公开模型的字段可直接读回（写端未实现，故这里只做读端往返）。
    assert_eq!(from_stream.key, "C");
    assert_eq!(from_stream.group_length, 701, "项 4 原值可直接读回");
    assert_eq!(from_stream.object_range(), Some((201, 204)));
}
