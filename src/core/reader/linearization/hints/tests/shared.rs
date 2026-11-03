//! 共享对象 hint 表（F.4.2，Table F.5/F.6）逐字段解析用例：位级手工
//! 装配 → 解析回读 → 断言每个字段；截断与畸形计数一律类型化错误。

use super::super::shared::SharedObjectHintTable;
use crate::core::reader::linearization::hints::bits::{BitWriter, HintBitError};

/// 表头（Table F.5）装配：32/32/32/32/16/32/16 定宽项。
fn write_header(w: &mut BitWriter) {
    w.unsigned(20, 32); // 项 1：共享对象区首对象号
    w.unsigned(5000, 32); // 项 2：共享对象区首对象位置
    w.unsigned(2, 32); // 项 3：首页条目数
    w.unsigned(3, 32); // 项 4：总条目数（含项 3）
    w.unsigned(8, 16); // 项 5：组对象数差值位宽
    w.unsigned(100, 32); // 项 6：组最小长度
    w.unsigned(12, 16); // 项 7：组长度差值位宽
}

/// 一条合法表：首页 2 组（第一组带 MD5 签名）、共享区 1 组。
///
/// F.4.2 规定交错**按序列各自进行**（原文 a)—d) 的「in each sequence」）：
/// 每个序列内先全部项 1（12 位），再全部项 2（1 位标志），再按标志顺序
/// 的 128 位签名，最后全部项 4（8 位）；首页序列之后才是共享区序列。
fn well_formed_table() -> Vec<u8> {
    let mut w = BitWriter::default();
    write_header(&mut w);
    // 序列一（首页 2 组）：项 1。
    w.unsigned(5, 12);
    w.unsigned(7, 12);
    // 项 2 标志。
    w.flag(true);
    w.flag(false);
    // 项 3：仅首页第一组有 128 位 MD5（0x00..0x0F）。
    for byte in 0x00u8..=0x0F {
        w.unsigned(byte as u64, 8);
    }
    // 项 4：对象数减一，3、0。
    w.unsigned(3, 8);
    w.unsigned(0, 8);
    // 序列二（共享区 1 组）：项 1、项 2、项 4（无签名）。
    w.unsigned(9, 12);
    w.flag(false);
    w.unsigned(1, 8);
    w.bytes
}

#[test]
fn shared_object_table_parses_every_field_of_tables_f5_and_f6() {
    let bytes = well_formed_table();
    let table = SharedObjectHintTable::parse(&bytes, 0).expect("well-formed table");
    assert_eq!(
        table.header,
        crate::core::reader::linearization::hints::shared::SharedObjectHeader {
            first_object_number: 20,
            first_object_location: 5000,
            first_page_entries: 2,
            total_entries: 3,
            group_object_bits: 8,
            least_group_length: 100,
            group_length_bits: 12,
        }
    );
    assert_eq!(table.first_page_groups.len(), 2);
    assert_eq!(table.shared_section_groups.len(), 1);
    let signature: Vec<u8> = (0x00u8..=0x0F).collect();
    assert_eq!(
        table.first_page_groups[0],
        crate::core::reader::linearization::hints::shared::SharedObjectGroupEntry {
            length_delta: 5,
            signature_present: true,
            signature: signature.as_slice().try_into().ok(),
            object_count_minus_one: 3,
        }
    );
    assert_eq!(
        table.first_page_groups[1].length_delta, 7,
        "second first-page group length delta"
    );
    assert_eq!(table.first_page_groups[1].signature, None);
    assert_eq!(table.first_page_groups[1].object_count_minus_one, 0);
    assert_eq!(table.shared_section_groups[0].length_delta, 9);
    assert_eq!(
        table.shared_section_groups[0].object_count_minus_one, 1,
        "Table F.6 item 4 stores count minus one"
    );
}

#[test]
fn table_parses_at_nonzero_byte_start() {
    // F.4.2 表自字节边界开始；偏移 3 字节前有 page offset hint 表占位。
    let mut bytes = vec![0u8; 3];
    bytes.extend(well_formed_table());
    let table = SharedObjectHintTable::parse(&bytes, 3).expect("aligned start");
    assert_eq!(table.header.first_object_number, 20);
}

#[test]
fn width_zero_header_fields_decode_as_zero_delta() {
    // Table F.5 项 5/7 为 0 时（全部组同长/同对象数），项 1/项 4 占 0 位、
    // 值为 0（F.4.1 的 0..32 范围声明同样适用于本表的位宽项）。
    let mut w = BitWriter::default();
    w.unsigned(1, 32);
    w.unsigned(2, 32);
    w.unsigned(1, 32);
    w.unsigned(1, 32);
    w.unsigned(0, 16);
    w.unsigned(64, 32);
    w.unsigned(0, 16);
    w.flag(false);
    let bytes = w.bytes;
    let table = SharedObjectHintTable::parse(&bytes, 0).expect("zero widths");
    assert_eq!(table.first_page_groups[0].length_delta, 0);
    assert_eq!(table.first_page_groups[0].object_count_minus_one, 0);
}

#[test]
fn every_truncation_of_a_real_table_is_a_typed_error() {
    let bytes = well_formed_table();
    for prefix in 0..bytes.len() {
        let result = SharedObjectHintTable::parse(&bytes[..prefix], 0);
        assert!(
            matches!(result, Err(HintBitError::ShortBuffer { .. })),
            "prefix {prefix}/{} must fail as typed ShortBuffer, got {result:?}",
            bytes.len()
        );
    }
    // 起点越界同样是类型化错误。
    assert!(matches!(
        SharedObjectHintTable::parse(&bytes, bytes.len() + 1),
        Err(HintBitError::ShortBuffer { .. })
    ));
}

#[test]
fn bogus_entry_count_is_rejected_before_allocating() {
    let mut w = BitWriter::default();
    w.unsigned(0, 32);
    w.unsigned(0, 32);
    w.unsigned(0, 32);
    w.unsigned(0xFFFF_FFFF, 32); // 项 4：远超剩余位的条目数
    w.unsigned(8, 16);
    w.unsigned(0, 32);
    w.unsigned(8, 16);
    let result = SharedObjectHintTable::parse(&w.bytes, 0);
    assert!(matches!(result, Err(HintBitError::ShortBuffer { .. })));
}

#[test]
fn oversized_width_declension_is_a_typed_error() {
    // 项 5 声明 100 位宽（16 位值域内的合法数字、超出 64 位原语上限）。
    let mut bytes = Vec::new();
    let mut head = BitWriter::default();
    head.unsigned(0, 32);
    head.unsigned(0, 32);
    head.unsigned(1, 32);
    head.unsigned(1, 32);
    head.unsigned(100, 16);
    head.unsigned(0, 32);
    head.unsigned(0, 16);
    head.flag(false);
    bytes.extend(head.bytes);
    let result = SharedObjectHintTable::parse(&bytes, 0);
    assert_eq!(result.err(), Some(HintBitError::InvalidWidth { bits: 100 }));
}
