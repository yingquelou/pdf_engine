//! 附录 F.4.4 generic hint 表**写端 → 读端**的真实往返集成测试。
//!
//! generic 形态一张表只描述**一个**对象组、四项全定宽 32 位（Table F.9）。本文件用
//! [`HintTableEncoder`] 把 F.4.4 承载的五个 Table F.2 键（`O`/`A`/`E`/`I`/`L`）各编码
//! 成 16 字节，按真实字节相邻排进主 hint 流，再喂回既有读端
//! [`GenericHintTable::parse`] 从各键偏移解出，断言四项数值逐一相等，并验证定宽
//! 32 位的溢出拒绝。

use pdf_engine::core::reader::linearization::hints::GenericHintTable;
use pdf_engine::core::writer::linearization::{HintEncodeError, HintTableEncoder};

/// Table F.9 单项定宽（位）与整表字节长（4 × 32 位 = 128 位 = 16 字节）。
const ITEM_BITS: u32 = 32;
const TABLE_BYTES: usize = 16;
/// 恰好越过 32 位定宽边界的值（读端会截断、写端须拒绝）。
const OVER_32_BIT: u64 = 1u64 << 32;

/// F.4.4 承载 generic 形态的五个 Table F.2 键（读端名册里的键序）。
const GENERIC_KEYS: [&str; 5] = ["O", "A", "E", "I", "L"];

/// 造一张合法的 generic 表（四项在 32 位内、游标元数据按其在流中的偏移回填）。
fn generic_group(key: &'static str, seed: u64, start_byte: usize) -> GenericHintTable {
    GenericHintTable {
        key,
        first_object_number: 100 + seed,
        first_object_location: 20_000 + seed,
        object_count: 2 + seed,
        group_length: 500 + seed,
        start_byte: start_byte as u64,
        bits_consumed: 128,
    }
}

/// 单张 generic 表恒 16 字节、无残位；读端解析后四项数值相等。
#[test]
fn hint_write_generic_table_is_sixteen_bytes_and_roundtrips_all_four_items() {
    let original = generic_group("O", 0, 0);
    let bytes = HintTableEncoder::new()
        .encode_generic_table(&original)
        .expect("in-range generic group must encode");
    assert_eq!(bytes.len(), TABLE_BYTES);
    let parsed = GenericHintTable::parse(&bytes, 0, "O").unwrap();
    assert_eq!(parsed.first_object_number, original.first_object_number);
    assert_eq!(parsed.first_object_location, original.first_object_location);
    assert_eq!(parsed.object_count, original.object_count);
    assert_eq!(parsed.group_length, original.group_length);
    assert_eq!(parsed.bits_consumed, 128);
}

/// 五键 generic 表首尾相邻排进主流（80 字节），各自从键偏移解析回原组、四项全等。
#[test]
fn hint_write_generic_five_keys_tile_the_stream_and_decode_per_key() {
    let mut stream = Vec::new();
    let mut originals = Vec::new();
    for (index, key) in GENERIC_KEYS.iter().enumerate() {
        let offset = stream.len();
        let group = generic_group(key, index as u64, offset);
        let bytes = HintTableEncoder::new()
            .encode_generic_table(&group)
            .unwrap();
        stream.extend_from_slice(&bytes);
        originals.push(group);
    }
    assert_eq!(stream.len(), TABLE_BYTES * GENERIC_KEYS.len());

    for (index, key) in GENERIC_KEYS.iter().enumerate() {
        let parsed = GenericHintTable::parse(&stream, index * TABLE_BYTES, key).unwrap();
        assert_eq!(
            (
                parsed.first_object_number,
                parsed.first_object_location,
                parsed.object_count,
                parsed.group_length
            ),
            (
                originals[index].first_object_number,
                originals[index].first_object_location,
                originals[index].object_count,
                originals[index].group_length
            ),
            "key {key} round-trip mismatch"
        );
        assert_eq!(parsed.start_byte, originals[index].start_byte);
    }
}

/// 四项任一超出定宽 32 位即类型化拒绝、产出零字节（读端 `unsigned(32)` 会截断高位）。
#[test]
fn hint_write_generic_rejects_a_value_above_the_32_bit_fixed_width() {
    for field in 0..4 {
        let mut group = generic_group("A", 0, 0);
        match field {
            0 => group.first_object_number = OVER_32_BIT,
            1 => group.first_object_location = OVER_32_BIT,
            2 => group.object_count = OVER_32_BIT,
            _ => group.group_length = OVER_32_BIT,
        }
        assert!(
            matches!(
                HintTableEncoder::new().encode_generic_table(&group),
                Err(HintEncodeError::ValueTooWide {
                    value: OVER_32_BIT,
                    bits: ITEM_BITS
                })
            ),
            "field {field} above the 32-bit fixed width must be refused"
        );
    }
}

/// 32 位满值（`u32::MAX`）在边界内可编码回读；`1<<32` 越界即拒——定宽边界各测一次。
#[test]
fn hint_write_generic_encodes_top_of_range_but_refuses_one_above_it() {
    let top = GenericHintTable {
        key: "I",
        first_object_number: u32::MAX as u64,
        first_object_location: 0,
        object_count: u32::MAX as u64,
        group_length: 0,
        start_byte: 0,
        bits_consumed: 128,
    };
    let bytes = HintTableEncoder::new().encode_generic_table(&top).unwrap();
    assert_eq!(bytes.len(), TABLE_BYTES);
    assert_eq!(GenericHintTable::parse(&bytes, 0, "I").unwrap(), top);
}
