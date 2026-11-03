//! generic hint table（F.4.4，Table F.9）的位级解析用例：四项定宽 32 位逐项
//! 归位、表长恒 128 位、空组与 32 位满声明值的 saturating 行为，以及截断／
//! 起点越界／声明位宽越出 0—32 三类的类型化拒绝——一律返回错误，不 panic。

use super::super::generic::{GENERIC_HINT_KEYS, GenericHintTable};
use super::super::page_offset::MAX_DECLARED_WIDTH;
use crate::core::reader::linearization::hints::bits::{BitWriter, HintBitError};

/// Table F.9 的四项（32/32/32/32）：组首对象号 30、位置 6000、对象数 3、
/// 组长度 900 字节。
fn group_bits(w: &mut BitWriter) {
    w.unsigned(30, 32); // 项 1：组内第一个对象的对象号
    w.unsigned(6000, 32); // 项 2：组内第一个对象的位置
    w.unsigned(3, 32); // 项 3：组内对象数
    w.unsigned(900, 32); // 项 4：组长度（字节）
}

/// 一张 generic 表 + 后续字节：`start` 之前的填充不影响解析结果，用于验证
/// 表确实按 Table F.2 键给出的位置起读。
fn table_after_padding(padding: usize) -> Vec<u8> {
    let mut w = BitWriter::default();
    for index in 0..padding as u64 {
        w.unsigned(index, 8);
    }
    group_bits(&mut w);
    w.bytes
}

#[test]
fn generic_hint_table_parses_the_four_items_of_table_f9_in_order() {
    let mut w = BitWriter::default();
    group_bits(&mut w);
    let table = GenericHintTable::parse(&w.bytes, 0, "O").expect("well-formed generic table");
    assert_eq!(table.key, "O", "身份须跟着解析结果走，否则无从按键取组");
    assert_eq!(table.first_object_number, 30);
    assert_eq!(table.first_object_location, 6000);
    assert_eq!(table.object_count, 3);
    assert_eq!(table.group_length, 900);
    assert_eq!(table.start_byte, 0);
    // 项 1 + 项 3 的闭区间：对象号 30、31、32。
    assert_eq!(table.object_range(), Some((30, 32)));
    // 16 字节整：本表不消费后续任何字节。
    assert_eq!(table.bits_consumed, 128);
    assert_eq!(w.bytes.len(), 16);
}

#[test]
fn generic_hint_table_start_byte_chains_the_next_table_end() {
    // 表可以落在流里任意字节偏移（F.4 前言：各表以字节边界开始，35482—35483），
    // 起点即 Table F.2 键的值；末字节 = 起点 + 16。
    for (padding, start) in [(0usize, 0u64), (44, 44), (124, 124)] {
        let data = table_after_padding(padding);
        let table =
            GenericHintTable::parse(&data, start as usize, "I").expect("table at given start");
        assert_eq!(table.start_byte, start);
        assert_eq!(
            table.end_byte(),
            start + 16,
            "Table F.9 长 128 位 = 16 字节"
        );
        assert_eq!(table.first_object_number, 30);
        assert_eq!(table.first_object_location, 6000);
        assert_eq!(table.object_count, 3);
        assert_eq!(table.group_length, 900);
    }
}

#[test]
fn generic_hint_table_with_zero_objects_degrades_to_no_object_range() {
    // 项 3 声明 0 个对象：原值保留（F.4.4 无「组须至少一个对象」的 shall），
    // 只有推出的号区间退回 None。
    let mut w = BitWriter::default();
    w.unsigned(7, 32);
    w.unsigned(800, 32);
    w.unsigned(0, 32);
    w.unsigned(0, 32);
    let table = GenericHintTable::parse(&w.bytes, 0, "L").expect("zero-object group");
    assert_eq!(table.object_count, 0);
    assert_eq!(table.group_length, 0);
    assert_eq!(table.object_range(), None);
    assert_eq!(table.bits_consumed, 128);
}

#[test]
fn generic_hint_table_accumulates_the_32_bit_maximum_object_range() {
    // 不可信位流能把项 1/项 3 读到 32 位最大值：号区间按项 1 + (项 3 - 1) 累加，
    // 结果仍精确、不截断（两项各 2^32-1 时末号是 8589934589）。
    let mut w = BitWriter::default();
    w.unsigned(u64::from(u32::MAX), 32);
    w.unsigned(0, 32);
    w.unsigned(u64::from(u32::MAX), 32);
    w.unsigned(u64::from(u32::MAX), 32);
    let table = GenericHintTable::parse(&w.bytes, 0, "E").expect("well-formed bits");
    assert_eq!(table.first_object_number, u64::from(u32::MAX));
    assert_eq!(table.object_count, u64::from(u32::MAX));
    assert_eq!(
        table.object_range(),
        Some((u64::from(u32::MAX), 8_589_934_589))
    );
    // 累加一律 saturating：定宽 32 位读不出这么宽，但访问器不假设来源，
    // 号与数都被写成 `u64` 上限时也只钳位、绝不溢出 panic。
    let extreme = GenericHintTable {
        key: "O",
        first_object_number: u64::MAX,
        first_object_location: 0,
        object_count: u64::MAX,
        group_length: 0,
        start_byte: 0,
        bits_consumed: 128,
    };
    assert_eq!(extreme.object_range(), Some((u64::MAX, u64::MAX)));
}

#[test]
fn generic_hint_table_declared_width_beyond_32_is_a_typed_rejection() {
    // 35524—35525：位数类条目的取值范围是 0—32。声明 33 位起一律类型化拒绝，
    // 不猜默认值、不做未定义行为（后续 F.4.5/F.4.6 的不可信声明宽度同走此门）。
    let data = table_after_padding(0);
    assert_eq!(MAX_DECLARED_WIDTH, 32);
    for bits in [MAX_DECLARED_WIDTH + 1, 40, u32::MAX] {
        assert_eq!(
            GenericHintTable::parse_with_item_width(&data, 0, "A", bits),
            Err(HintBitError::InvalidWidth {
                bits: u64::from(bits)
            }),
            "声明位宽 {bits} 须被类型化拒绝"
        );
    }
    // 边界内照常读取：定宽 32 位是 Table F.9 的形态，8 位只用于验证宽度参数
    // 真的参与读取（四项各占一字节，共 4 字节 32 位）。
    let narrow = GenericHintTable::parse_with_item_width(&data, 0, "A", 8)
        .expect("width inside the declared range");
    assert_eq!(narrow.bits_consumed, 32);
    assert_eq!(narrow.end_byte(), 4);
    assert_eq!(narrow.first_object_number, data[0] as u64);
    assert_eq!(narrow.object_count, data[2] as u64);
    assert!(
        GenericHintTable::parse_with_item_width(&data, 0, "A", MAX_DECLARED_WIDTH).is_ok(),
        "32 位是 Table F.9 的声明定宽"
    );
}

#[test]
fn every_truncation_of_a_generic_hint_table_is_a_typed_error() {
    // 128 位读不满即类型化错误：本家族没有任何「猜字节」的退路。
    let data = table_after_padding(0);
    for prefix in 0..data.len() {
        let error = GenericHintTable::parse(&data[..prefix], 0, "O")
            .expect_err("a truncated group must not decode");
        assert!(
            matches!(error, HintBitError::ShortBuffer { .. }),
            "prefix {prefix}: {error:?}"
        );
    }
    // 起点恰在流尾与越出流尾都只给类型化错误，不越界切片、不猜字节。
    for start in [data.len(), data.len() + 1, data.len() * 2] {
        assert!(
            matches!(
                GenericHintTable::parse(&data, start, "O"),
                Err(HintBitError::ShortBuffer { .. })
            ),
            "start {start} must not decode"
        );
    }
}

#[test]
fn generic_hint_keys_cover_exactly_the_five_f44_tables() {
    // F.4.4 的名单（35752—35756）＝Table F.2 键 O/A/E/I/L；V/C/R 属 F.4.5、
    // B 属 F.4.6，都不得混进本家族。
    assert_eq!(GENERIC_HINT_KEYS, ["O", "A", "E", "I", "L"]);
}
