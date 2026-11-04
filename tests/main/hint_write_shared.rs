//! 附录 F.4.2 共享对象 hint 表**写端 → 读端**的真实往返集成测试。
//!
//! 用 [`HintTableEncoder`] 把类型化模型编码成位流字节，再喂回既有读端
//! [`SharedObjectHintTable::parse`] 解出，断言表头七项与两条序列的每一组四项都逐字
//! 段相等——这正是本切片「写端产出与读端消费的单元尺寸逐位一致」的验收核心。非法
//! 模型（计数/宽度/签名不自洽）在写出任何字节前返回类型化错误。

use pdf_engine::core::reader::linearization::hints::{
    SharedObjectGroupEntry, SharedObjectHeader, SharedObjectHintTable,
};
use pdf_engine::core::writer::linearization::{HintEncodeError, HintTableEncoder};

/// 表头七项的定宽位数之和：32+32+32+32+16+32+16 = 192 位 = 24 字节。
const HEADER_BYTES: usize = 24;

/// 造一张自洽的共享对象 hint 表：首页 2 组、共享对象区 1 组，宽度/计数/标志全一致。
fn sample_table(seed: u64) -> SharedObjectHintTable {
    let md5: [u8; 16] = std::array::from_fn(|i| (i as u64 * 3 + seed) as u8);
    SharedObjectHintTable {
        header: SharedObjectHeader {
            first_object_number: 500 + seed,
            first_object_location: 3_000 + seed,
            first_page_entries: 2,
            total_entries: 3,
            group_object_bits: 6,
            least_group_length: 16,
            group_length_bits: 10,
        },
        first_page_groups: vec![
            SharedObjectGroupEntry {
                length_delta: 100 + seed,
                signature_present: true,
                signature: Some(md5),
                object_count_minus_one: 3,
            },
            SharedObjectGroupEntry {
                length_delta: 200 + seed,
                signature_present: false,
                signature: None,
                object_count_minus_one: 5,
            },
        ],
        shared_section_groups: vec![SharedObjectGroupEntry {
            length_delta: 300 + seed,
            signature_present: true,
            signature: Some(md5),
            object_count_minus_one: 1,
        }],
    }
}

/// 期望总位数：表头 192 位，加每组（长度增量 + 1 位标志 + 对象数）与置位签名者 128 位。
fn expected_bits(table: &SharedObjectHintTable) -> u64 {
    let mut bits = 192u64;
    for group in table
        .first_page_groups
        .iter()
        .chain(table.shared_section_groups.iter())
    {
        bits += u64::from(table.header.group_length_bits);
        bits += 1;
        bits += u64::from(table.header.group_object_bits);
        if group.signature_present {
            bits += 128;
        }
    }
    bits
}

/// 编码 → 读端解析，表头与两条序列逐字段相等，且整结构相等。
#[test]
fn hint_write_shared_roundtrips_every_field_through_the_reader() {
    let original = sample_table(7);
    let bytes = HintTableEncoder::new()
        .encode_shared_object_table(&original)
        .expect("valid shared table must encode");
    assert_eq!(bytes.len(), (expected_bits(&original) as usize).div_ceil(8));

    let parsed =
        SharedObjectHintTable::parse(&bytes, 0).expect("encoded bytes must parse back at byte 0");
    assert_eq!(parsed.header, original.header);
    assert_eq!(parsed.first_page_groups, original.first_page_groups);
    assert_eq!(parsed.shared_section_groups, original.shared_section_groups);
    assert_eq!(parsed, original);
}

/// 两张共享对象表首尾相邻排进主 hint 流；各自从正确字节偏移解析回原模型，互不越界。
#[test]
fn hint_write_shared_tables_tile_the_main_stream_and_decode_at_their_offsets() {
    let first = sample_table(0);
    let second = sample_table(1);

    let mut stream = HintTableEncoder::new()
        .encode_shared_object_table(&first)
        .unwrap();
    let first_len = stream.len();
    stream.extend_from_slice(
        &HintTableEncoder::new()
            .encode_shared_object_table(&second)
            .unwrap(),
    );

    assert_eq!(SharedObjectHintTable::parse(&stream, 0).unwrap(), first);
    assert_eq!(
        SharedObjectHintTable::parse(&stream, first_len).unwrap(),
        second
    );
}

/// 表头计数与实际组向量不符：写出任何字节前返回 [`HintEncodeError::EntryCountMismatch`]。
#[test]
fn hint_write_shared_rejects_inconsistent_entry_counts_with_zero_bytes() {
    let mut table = sample_table(0);
    table.header.total_entries = 42;
    let result = HintTableEncoder::new().encode_shared_object_table(&table);
    assert!(matches!(
        result,
        Err(HintEncodeError::EntryCountMismatch { .. })
    ));
}

/// 签名标志与 MD5 载荷不同现：写出任何字节前返回 [`HintEncodeError::SignatureFlagMismatch`]。
#[test]
fn hint_write_shared_rejects_signature_flag_mismatch_with_zero_bytes() {
    let mut table = sample_table(0);
    table.first_page_groups[1].signature_present = true;
    let result = HintTableEncoder::new().encode_shared_object_table(&table);
    assert!(matches!(
        result,
        Err(HintEncodeError::SignatureFlagMismatch { group: 1 })
    ));
}

/// 组字段超出表头声明宽度即拒：绝不写出会被读端截断的高位（往返恒等的前提）。
#[test]
fn hint_write_shared_rejects_group_field_over_declared_width() {
    let mut table = sample_table(0);
    table.first_page_groups[0].length_delta = 1u64 << table.header.group_length_bits;
    assert!(matches!(
        HintTableEncoder::new().encode_shared_object_table(&table),
        Err(HintEncodeError::ValueTooWide { .. })
    ));
}

/// 空序列共享对象表只落 24 字节表头，读端解析回空的两条序列。
#[test]
fn hint_write_shared_encodes_empty_sequences_as_header_only() {
    let mut table = sample_table(0);
    table.header.first_page_entries = 0;
    table.header.total_entries = 0;
    table.first_page_groups.clear();
    table.shared_section_groups.clear();
    let bytes = HintTableEncoder::new()
        .encode_shared_object_table(&table)
        .unwrap();
    assert_eq!(bytes.len(), HEADER_BYTES);
    assert_eq!(SharedObjectHintTable::parse(&bytes, 0).unwrap(), table);
}
