//! extended generic hint table（F.4.5，Table F.10）的位级解析用例：项 1—6 逐项
//! 归位、项 7 的标识符列按**组内声明宽度**（项 6）解包、变长表长的 `div_ceil`
//! 折算，以及三类不可信输入的硬拒——项 6 越出 0—32、项 5 × 项 6 越过流尾、
//! 项 5 是不可信 32 位大计数。一律返回类型化错误，不 panic、不做不可界分配。

use super::super::extended::{
    EXTENDED_HINT_KEYS, ExtendedHintTable, group_hint_keys, uses_extended_format,
};
use super::super::generic::GENERIC_HINT_KEYS;
use super::super::page_offset::MAX_UNEVIDENCED_ITEMS;
use crate::core::reader::linearization::hints::bits::{BitWriter, HintBitError};
use crate::core::types::PdfVersion;

/// Table F.10 项 1—6 的定宽合计（位）：4×32 + 32 + 16（35808—35814）。
const HEAD_BITS: u64 = 176;

/// 项 1—4 的固定声明值（组首对象号 / 组首位置 / 组内对象数 / 组长度），
/// 让断言只关心项 5—7。
const GROUP_ITEMS: (u64, u64, u64, u64) = (120, 24_000, 4, 1_200);

/// 写项 1—6（`raw_width` 直接进项 6 的 16 位存储字段，故可写畸形声明值）再按
/// `id_bits` 位逐项写 `ids` 列（项 7 起）。
fn extended_raw(w: &mut BitWriter, count: u64, raw_width: u64, id_bits: u32, ids: &[u64]) {
    let (number, location, objects, length) = GROUP_ITEMS;
    w.unsigned(number, 32); // 项 1：组内第一个对象的对象号
    w.unsigned(location, 32); // 项 2：组内第一个对象的位置
    w.unsigned(objects, 32); // 项 3：组内对象数
    w.unsigned(length, 32); // 项 4：对象组的长度（字节）
    w.unsigned(count, 32); // 项 5：共享对象引用数（35812）
    w.unsigned(raw_width, 16); // 项 6：组内标识符位宽（35813—35814）
    for id in ids {
        w.unsigned(*id, id_bits); // 项 7 起：逐个共享对象标识符
    }
}

/// well-formed 形态：项 6 的存储值就是解包列用的宽度。
fn extended_bits(w: &mut BitWriter, count: u64, id_bits: u32, ids: &[u64]) {
    extended_raw(w, count, u64::from(id_bits), id_bits, ids);
}

/// 一张 well-formed 的 extended 表（项 6 = 5 位、三个标识符）+ `padding` 填充。
fn table_after_padding(padding: usize) -> Vec<u8> {
    let mut w = BitWriter::default();
    for index in 0..padding as u64 {
        w.unsigned(index, 8);
    }
    extended_bits(&mut w, 3, 5, &[1, 31, 0]);
    w.bytes
}

#[test]
fn extended_hint_table_parses_the_seven_items_of_table_f10_in_order() {
    let mut w = BitWriter::default();
    // 项 6 取 5 位（非平凡宽度），项 7 三个标识符 1/31/0。
    extended_bits(&mut w, 3, 5, &[1, 31, 0]);
    let table = ExtendedHintTable::parse(&w.bytes, 0, "V").expect("well-formed extended table");
    assert_eq!(table.key, "V", "身份须跟着解析结果走，否则无从按键取组");
    let (number, location, objects, length) = GROUP_ITEMS;
    assert_eq!(table.first_object_number, number, "项 1");
    assert_eq!(table.first_object_location, location, "项 2");
    assert_eq!(table.object_count, objects, "项 3");
    assert_eq!(table.group_length, length, "项 4");
    assert_eq!(table.shared_reference_count, 3, "项 5：共享对象引用数");
    assert_eq!(table.shared_object_id_bits, 5, "项 6：组内标识符位宽");
    assert_eq!(
        table.shared_object_ids,
        vec![1, 31, 0],
        "项 7 起按项 6 解包"
    );
    assert_eq!(
        table.shared_object_ids.len() as u64,
        table.shared_reference_count,
        "成功解析的表其列长恒等于项 5"
    );
    // 176 + 3×5 = 191 位 → 24 字节（div_ceil）。
    assert_eq!(table.bits_consumed, HEAD_BITS + 15);
    assert_eq!(table.end_byte(), 24);
    assert_eq!(w.bytes.len(), 24);
    assert_eq!(table.object_range(), Some((number, number + objects - 1)));
}

#[test]
fn extended_hint_table_start_byte_chains_the_next_table_end() {
    // 表可落在流里任意字节偏移（F.4 前言 35482—35483），起点即 Table F.2 键的值；
    // 与 Table F.9 不同，本表**变长**，故末字节只能按 bits_consumed 折算。
    for padding in [0usize, 44, 124] {
        let data = table_after_padding(padding);
        let table =
            ExtendedHintTable::parse(&data, padding, "R").expect("table at the given start");
        assert_eq!(table.start_byte, padding as u64);
        assert_eq!(table.bits_consumed, HEAD_BITS + 15);
        assert_eq!(
            table.end_byte(),
            padding as u64 + 24,
            "项 6 = 5 位、三项标识符 = 191 位 → 24 字节"
        );
        assert_eq!(table.shared_object_ids, vec![1, 31, 0]);
    }
    // 尾部多出的字节不参与本表：后续表的位置下界正是 end_byte。
    let mut data = table_after_padding(0);
    data.extend_from_slice(&[0xAA; 8]);
    let table = ExtendedHintTable::parse(&data, 0, "C").expect("table with trailing bytes");
    assert_eq!(table.end_byte(), 24);
}

#[test]
fn extended_hint_table_with_zero_width_yields_zero_identifiers_and_consumes_no_bits() {
    // 项 6 = 0：0 位只能表示值 0，故列不占位、标识符全为 0（模块头的**待核**）。
    let mut w = BitWriter::default();
    extended_bits(&mut w, 3, 0, &[]);
    let table = ExtendedHintTable::parse(&w.bytes, 0, "V").expect("zero-width group");
    assert_eq!(table.shared_object_id_bits, 0);
    assert_eq!(table.shared_reference_count, 3, "项 5 的原值一律保留");
    assert_eq!(table.shared_object_ids, vec![0, 0, 0]);
    assert_eq!(table.bits_consumed, HEAD_BITS, "宽度 0 的列消费 0 位");
    assert_eq!(table.end_byte(), 22);
}

#[test]
fn extended_hint_table_with_zero_references_yields_an_empty_identifier_list() {
    // 项 5 = 0：没有共享对象引用，项 7 起没有列；表长恒 176 位 = 22 字节。
    let mut w = BitWriter::default();
    extended_bits(&mut w, 0, 5, &[]);
    let table = ExtendedHintTable::parse(&w.bytes, 0, "C").expect("zero-reference group");
    assert_eq!(table.shared_reference_count, 0);
    assert_eq!(table.shared_object_id_bits, 5);
    assert!(table.shared_object_ids.is_empty());
    assert_eq!(table.bits_consumed, HEAD_BITS);
    assert_eq!(table.end_byte(), 22);
}

#[test]
fn extended_hint_table_with_zero_objects_degrades_to_no_object_range() {
    // 项 3 = 0：F.4.5 没有「组须至少含一个对象」的 shall（该措辞只在 F.4.6 的
    // 35792—35793 出现），故原值保留、只让推出的号区间退回 None。
    let mut w = BitWriter::default();
    w.unsigned(400, 32);
    w.unsigned(30_000, 32);
    w.unsigned(0, 32);
    w.unsigned(0, 32);
    w.unsigned(2, 32);
    w.unsigned(3, 16);
    w.unsigned(2, 3);
    w.unsigned(7, 3);
    let table = ExtendedHintTable::parse(&w.bytes, 0, "R").expect("empty object group");
    assert_eq!(table.object_count, 0);
    assert_eq!(table.group_length, 0);
    assert_eq!(table.object_range(), None);
    assert_eq!(table.shared_object_ids, vec![2, 7]);
}

#[test]
fn extended_hint_table_object_range_accumulates_without_overflow() {
    // 项 1/项 3 都能被读到 32 位最大值：号区间按项 1 +（项 3 − 1）累加。
    let mut wide = BitWriter::default();
    wide.unsigned(u64::from(u32::MAX), 32); // 项 1
    wide.unsigned(0, 32); // 项 2
    wide.unsigned(u64::from(u32::MAX), 32); // 项 3
    wide.unsigned(u64::from(u32::MAX), 32); // 项 4
    wide.unsigned(0, 32); // 项 5：没有引用，表长恒 176 位
    wide.unsigned(0, 16); // 项 6
    let table = ExtendedHintTable::parse(&wide.bytes, 0, "V").expect("well-formed bits");
    assert_eq!(
        table.object_range(),
        Some((u64::from(u32::MAX), 8_589_934_589))
    );
    assert_eq!(table.bits_consumed, HEAD_BITS);
    assert_eq!(wide.bytes.len(), 22);
    // 访问器一律 saturating：字段被写成 `u64` 上限时也只钳位，绝不溢出 panic。
    let mut extreme = table;
    extreme.first_object_number = u64::MAX;
    extreme.object_count = u64::MAX;
    assert_eq!(extreme.object_range(), Some((u64::MAX, u64::MAX)));
    extreme.shared_reference_count = u64::MAX;
    assert_eq!(extreme.object_range(), Some((u64::MAX, u64::MAX)));
}

#[test]
fn extended_hint_table_declared_width_beyond_32_is_a_typed_rejection() {
    // 35524—35525 的 0—32 范围按同一措辞类比项 6（35813—35814）：声明 33 位起
    // 一律类型化拒绝。本表**保留原值没有意义**——项 7 的列长直接由它决定。
    for declared in [33u64, 40, 64, u64::from(u16::MAX)] {
        let mut w = BitWriter::default();
        extended_raw(&mut w, 2, declared, 5, &[1, 2]);
        assert_eq!(
            ExtendedHintTable::parse(&w.bytes, 0, "V"),
            Err(HintBitError::InvalidWidth { bits: declared }),
            "项 6 声明 {declared} 位须被类型化拒绝"
        );
    }
    // 边界内照常读取：32 位是允许的最大声明宽度（本用例的列只有 2×32 位）。
    let mut w = BitWriter::default();
    extended_bits(&mut w, 2, 32, &[1, u64::from(u32::MAX)]);
    let table = ExtendedHintTable::parse(&w.bytes, 0, "V").expect("width at the declared limit");
    assert_eq!(table.shared_object_id_bits, 32);
    assert_eq!(table.shared_object_ids, vec![1, u64::from(u32::MAX)]);
}

#[test]
fn every_truncation_of_an_extended_hint_table_is_a_typed_error() {
    // 项 1—6 与项 7 的列读不满即类型化错误：本家族没有任何「猜字节」的退路，
    // 也不留「静默半截列」。
    let data = table_after_padding(0);
    assert_eq!(data.len(), 24);
    for prefix in 0..data.len() {
        let error = ExtendedHintTable::parse(&data[..prefix], 0, "V")
            .expect_err("a truncated table must not decode");
        assert!(
            matches!(error, HintBitError::ShortBuffer { .. }),
            "prefix {prefix}: {error:?}"
        );
    }
    // 起点恰在流尾与越出流尾都只给类型化错误，不越界切片、不猜字节。
    for start in [data.len(), data.len() + 1, data.len() * 2] {
        assert!(
            matches!(
                ExtendedHintTable::parse(&data, start, "V"),
                Err(HintBitError::ShortBuffer { .. })
            ),
            "start {start} must not decode"
        );
    }
}

#[test]
fn truncated_identifier_column_reports_the_bits_it_still_needed() {
    // 项 1—6 齐全、列只给了 2 个标识符的位并被截到 23 字节：项 5 声明 3 →
    // 类型化拒绝，`needed_bits` 是**整列**的 3×5 = 15 位、剩 8 位。
    let mut w = BitWriter::default();
    extended_bits(&mut w, 3, 5, &[1, 2]);
    assert_eq!(w.bytes.len(), 24);
    assert_eq!(
        ExtendedHintTable::parse(&w.bytes[..23], 0, "C"),
        Err(HintBitError::ShortBuffer {
            needed_bits: 15,
            remaining_bits: 8,
            position_bits: HEAD_BITS,
        })
    );
}

#[test]
fn huge_reference_count_against_a_small_buffer_is_rejected_before_allocating() {
    // 项 5 是不可信 32 位计数、项 6 是不可信 16 位宽度，二者相乘可达 ~2^48：
    // 分配与循环前一律按剩余位封顶，绝不 `Vec::with_capacity(原始计数)`。
    for (count, id_bits) in [
        (u64::from(u32::MAX), 32u32),
        (u64::from(u32::MAX), 16),
        (u64::from(u32::MAX), 1),
        (u64::from(u32::MAX) - 1, 32),
        (2, 32),
    ] {
        let mut w = BitWriter::default();
        extended_bits(&mut w, count, id_bits, &[]);
        let data = w.bytes;
        let error = ExtendedHintTable::parse(&data, 0, "R")
            .expect_err("an uneconomic reference count must not decode");
        let HintBitError::ShortBuffer {
            needed_bits,
            remaining_bits,
            ..
        } = error
        else {
            panic!("count {count} × width {id_bits}: {error:?}");
        };
        assert!(
            needed_bits > remaining_bits,
            "count {count} × width {id_bits}: needed {needed_bits} ≤ remaining {remaining_bits}"
        );
    }
    // 宽度 0 时列不占位，位预算给不出计数证据 → 改用绝对上限（模块头**待核**）。
    let mut w = BitWriter::default();
    extended_bits(&mut w, MAX_UNEVIDENCED_ITEMS + 1, 0, &[]);
    assert!(matches!(
        ExtendedHintTable::parse(&w.bytes, 0, "V"),
        Err(HintBitError::ShortBuffer { .. })
    ));
    // 上限内照旧通过：0 位列 = 全 0 标识符。
    let mut ok = BitWriter::default();
    extended_bits(&mut ok, MAX_UNEVIDENCED_ITEMS, 0, &[]);
    let table = ExtendedHintTable::parse(&ok.bytes, 0, "V").expect("inside the absolute cap");
    assert_eq!(
        table.shared_object_ids.len(),
        MAX_UNEVIDENCED_ITEMS as usize
    );
    assert!(table.shared_object_ids.iter().all(|id| *id == 0));
}

#[test]
fn extended_hint_id_width_comparison_is_a_derived_diagnostic_only() {
    // 文档级 Table F.3 项 11 与组内项 6 的自洽核对：由两处字面定义**推出**，
    // 不是原文的 shall（模块头**待核**）。声明值一律保留、表照常读取。
    let mut w = BitWriter::default();
    extended_bits(&mut w, 2, 5, &[1, 2]);
    let table = ExtendedHintTable::parse(&w.bytes, 0, "V").expect("well-formed bits");
    assert_eq!(table.shared_object_id_bits, 5);
    assert_eq!(
        table.shared_id_width_problem(4),
        Some((5, 4)),
        "组内 5 > 文档 4"
    );
    assert_eq!(table.shared_id_width_problem(5), None, "相等不构成矛盾");
    assert_eq!(table.shared_id_width_problem(32), None);
    assert_eq!(table.shared_id_width_problem(0), Some((5, 0)));
}

#[test]
fn extended_hint_keys_cover_exactly_the_three_f45_tables() {
    // F.4.5 的名册（35781—35785 的三类表 ↔ Table F.2 的 `V`/`C`/`R`）：
    // `B` 属 F.4.6（35786—35788 明确划归），F.4.4 的五键不得混进本家族。
    assert_eq!(EXTENDED_HINT_KEYS, ["V", "C", "R"]);
    for key in EXTENDED_HINT_KEYS {
        assert!(
            !GENERIC_HINT_KEYS.contains(&key),
            "键 {key} 同时落在 F.4.4 与 F.4.5 的名册里"
        );
    }
}

#[test]
fn extended_format_gates_on_pdf_1_5_and_earlier_versions_keep_the_generic_shape() {
    // F.4.5「As of PDF 1.5 … shall use the extended generic format」（35781）：
    // ≥ 1.5 才按 Table F.10 读 `V`/`C`/`R`；< 1.5 时三键并入 Table F.9 路径，
    // 且五键的相对次序与 GENERIC_HINT_KEYS 一致（既有行为不变）。
    assert!(uses_extended_format(PdfVersion::V1_5));
    assert!(uses_extended_format(PdfVersion::V1_6));
    assert!(uses_extended_format(PdfVersion::V1_7));
    assert!(!uses_extended_format(PdfVersion::V1_4));
    assert!(!uses_extended_format(PdfVersion::V1_1));
    assert_eq!(group_hint_keys(PdfVersion::V1_5), &GENERIC_HINT_KEYS[..]);
    let before = group_hint_keys(PdfVersion::V1_4);
    assert_eq!(before.len(), 8, "五键 + V/C/R");
    for key in EXTENDED_HINT_KEYS {
        assert!(before.contains(&key), "{key} 须在 < 1.5 时走 generic 形态");
    }
    // 名册序（Table F.2 的 35326—35338 与 35379—35380）：O A E V I C L R。
    let order = GENERIC_HINT_KEYS
        .iter()
        .map(|key| before.iter().position(|found| *found == *key))
        .collect::<Vec<_>>();
    assert_eq!(order, vec![Some(0), Some(1), Some(2), Some(4), Some(6)]);
    assert_eq!(
        before,
        &["O", "A", "E", "V", "I", "C", "L", "R"],
        "五键相对次序不变，故既有 generic 行为零变更"
    );
}
