//! 页偏移 hint 表（F.4.1，Table F.3/F.4）逐字段解析用例：位级手工装配 →
//! 解析回读 → 断言每项与 a)—g) 的交错列序；宽度越界、页数/计数不可信与截断
//! 一律类型化错误，且不先分配。

use super::super::page_offset::{PageOffsetEntry, PageOffsetHeader, PageOffsetHintTable};
use crate::core::reader::linearization::hints::bits::{BitReader, BitWriter, HintBitError};

/// Table F.3 头部（13 项，288 位）：位宽项 4/6/5/7/3/5/2，分母 4。
fn write_header(w: &mut BitWriter) {
    w.unsigned(3, 32); // 项 1：每页最少对象数
    w.unsigned(1000, 32); // 项 2：首页页对象位置
    w.unsigned(4, 16); // 项 3：对象数增量的位宽（Table F.4 项 1）
    w.unsigned(500, 32); // 项 4：最少页长
    w.unsigned(6, 16); // 项 5：页长增量的位宽（项 2）
    w.unsigned(40, 32); // 项 6：内容流起点偏移最小值
    w.unsigned(5, 16); // 项 7：偏移增量的位宽（项 6）
    w.unsigned(60, 32); // 项 8：内容流长度最小值
    w.unsigned(7, 16); // 项 9：长度增量的位宽（项 7）
    w.unsigned(3, 16); // 项 10：共享对象引用数的位宽（项 3）
    w.unsigned(5, 16); // 项 11：共享对象标识的位宽（项 4）
    w.unsigned(2, 16); // 项 12：分子位宽（项 5）
    w.unsigned(4, 16); // 项 13：分母值
}

/// 三页的合法表：F.4.1 a)—g) 的列序逐项写入。
///
/// 交错序（35514—35523）：项 1/2/3/6/7 各是一整列（页序），项 4 与项 5 只对
/// 第二页起的共享对象成列——第二页 2 个引用、第三页 1 个引用，故项 4 列是
/// `[17, 3, 9]`、项 5 列是 `[3, 0, 1]`。
fn three_page_table() -> Vec<u8> {
    let mut w = BitWriter::default();
    write_header(&mut w);
    // a) 项 1（4 位）：三页的对象数增量。
    for delta in [0, 2, 1] {
        w.unsigned(delta, 4);
    }
    // b) 项 2（6 位）：三页的页长增量。
    for delta in [10, 20, 30] {
        w.unsigned(delta, 6);
    }
    // c) 项 3（3 位）：首页 0（shall），第二页 2，第三页 1。
    for count in [0, 2, 1] {
        w.unsigned(count, 3);
    }
    // d) 项 4（5 位）：第二页的标识在前，然后第三页。
    for id in [17, 3, 9] {
        w.unsigned(id, 5);
    }
    // e) 项 5（2 位）：与项 4 同序的分子。
    for numerator in [3, 0, 1] {
        w.unsigned(numerator, 2);
    }
    // f) 项 6（5 位）。
    for delta in [8, 16, 24] {
        w.unsigned(delta, 5);
    }
    // g) 项 7（7 位）。
    for delta in [100, 1, 2] {
        w.unsigned(delta, 7);
    }
    w.bytes
}

#[test]
fn page_offset_table_parses_every_field_of_tables_f3_and_f4() {
    let bytes = three_page_table();
    let table = PageOffsetHintTable::parse(&bytes, 0, 3).expect("well-formed table");
    assert_eq!(
        table.header,
        PageOffsetHeader {
            least_objects_per_page: 3,
            first_page_object_location: 1000,
            object_count_bits: 4,
            least_page_length: 500,
            page_length_bits: 6,
            least_content_offset: 40,
            content_offset_bits: 5,
            least_content_length: 60,
            content_length_bits: 7,
            shared_ref_count_bits: 3,
            shared_object_id_bits: 5,
            shared_position_numerator_bits: 2,
            shared_position_denominator: 4,
        },
        "Table F.3 has 13 fixed-width items and none of them may be dropped"
    );
    assert_eq!(table.declared_pages, 3);
    assert_eq!(table.pages.len(), 3);
    assert!(table.width_problems().is_empty());
    // 首页：增量按 Table F.3 项 1/4/6/8 折算成类型化访问值。
    assert_eq!(
        table.pages[0],
        PageOffsetEntry {
            object_count_delta: 0,
            page_length_delta: 10,
            shared_reference_count: 0,
            shared_object_ids: Vec::new(),
            shared_position_numerators: Vec::new(),
            content_offset_delta: 8,
            content_length_delta: 100,
        }
    );
    assert_eq!(table.pages[0].object_count(&table.header), 3);
    assert_eq!(table.pages[0].page_length(&table.header), 510);
    assert_eq!(table.pages[0].content_offset(&table.header), 48);
    assert_eq!(table.pages[0].content_length(&table.header), 160);
    // 第二页 2 个共享引用、第三页 1 个：列序按 d)/e) 归位，不跨页混排。
    assert_eq!(table.pages[1].shared_reference_count, 2);
    assert_eq!(table.pages[1].shared_object_ids, vec![17, 3]);
    assert_eq!(table.pages[1].shared_position_numerators, vec![3, 0]);
    assert_eq!(table.pages[2].shared_reference_count, 1);
    assert_eq!(table.pages[2].shared_object_ids, vec![9]);
    assert_eq!(table.pages[2].shared_position_numerators, vec![1]);
    assert_eq!(table.pages[2].object_count(&table.header), 4);
    assert_eq!(table.pages[2].page_length(&table.header), 530);
    // 5×32 + 8×16 = 288 位头部，加 3×(4+6+3+5+7) 页列 75 位与 3×(5+2) 共享列 21 位 = 384 位。
    assert_eq!(table.bits_consumed, 384);
    assert_eq!(table.end_byte(), 48);
    assert_eq!(bytes.len(), 48, "the writer pads the tail bits to a byte");
}

#[test]
fn page_offset_table_parses_at_nonzero_byte_start() {
    // F.4 前言：表以字节边界开始（`start` 由调用方给出，本家族恒 0）。
    let mut bytes = vec![0xAAu8; 5];
    bytes.extend(three_page_table());
    let table = PageOffsetHintTable::parse(&bytes, 5, 3).expect("aligned start");
    assert_eq!(table.header.least_objects_per_page, 3);
    assert_eq!(table.pages[1].shared_object_ids, vec![17, 3]);
    assert_eq!(table.bits_consumed, 384, "bits count from the table start");
}

#[test]
fn zero_widths_decode_as_zero_deltas_without_consuming_bits() {
    // Table F.3 项 3/5/7/9 全 0（每页同对象数、同页长……）时不占位，
    // 类型化访问值退回头部最小值（35524—35525 明确 0 合法）。
    let mut w = BitWriter::default();
    w.unsigned(7, 32);
    w.unsigned(0, 32);
    w.unsigned(0, 16);
    w.unsigned(111, 32);
    w.unsigned(0, 16);
    w.unsigned(1, 32);
    w.unsigned(0, 16);
    w.unsigned(222, 32);
    w.unsigned(0, 16);
    w.unsigned(0, 16); // 项 10：引用数列也 0 位
    w.unsigned(0, 16);
    w.unsigned(0, 16);
    w.unsigned(1, 16);
    let bytes = w.bytes;
    let table = PageOffsetHintTable::parse(&bytes, 0, 4).expect("zero widths");
    assert_eq!(table.pages.len(), 4);
    assert_eq!(table.bits_consumed, 288, "no per-page bits at all");
    assert!(table.width_problems().is_empty());
    for entry in &table.pages {
        assert_eq!(entry.shared_reference_count, 0);
        assert_eq!(entry.object_count(&table.header), 7);
        assert_eq!(entry.page_length(&table.header), 111);
        assert_eq!(entry.content_offset(&table.header), 1);
        assert_eq!(entry.content_length(&table.header), 222);
    }
}

#[test]
fn every_table_field_is_unsigned_never_sign_extended() {
    // 项 1/2/4/6/8 是 32 位、位宽项是 16 位，全部按无符号读：最高位为 1 的
    // 图案不得变成负值（对比同一图案的带符号读法）。
    let mut w = BitWriter::default();
    w.unsigned(u32::MAX as u64, 32);
    w.unsigned(0, 32);
    w.unsigned(1, 16);
    let bytes = w.bytes;
    let mut reader = BitReader::new(&bytes);
    assert_eq!(reader.unsigned(32).expect("32 bits"), u32::MAX as u64);
    let mut signed_reader = BitReader::new(&bytes);
    assert_eq!(
        signed_reader.signed(32).expect("32 bits"),
        -2_147_483_647,
        "the same pattern read as the signed primitive is negative"
    );
    // 16 位位宽项 0x8000 同样是无符号 32768（不是 −32768）。
    let mut width_reader = BitReader::new(&[0x80, 0x00]);
    assert_eq!(width_reader.unsigned(16).expect("16 bits"), 32_768);
}

#[test]
fn increment_accessors_saturate_instead_of_wrapping() {
    // 字段位宽最大 32/16 位，实际值不会溢出 `u64`；访问器仍一律 saturating，
    // 保证将来加宽或畸形头部不可能回绕成小值。
    let header = PageOffsetHeader {
        least_objects_per_page: u64::MAX,
        least_page_length: u64::MAX,
        least_content_offset: u64::MAX,
        least_content_length: u64::MAX,
        first_page_object_location: 0,
        object_count_bits: 0,
        page_length_bits: 0,
        content_offset_bits: 0,
        content_length_bits: 0,
        shared_ref_count_bits: 0,
        shared_object_id_bits: 0,
        shared_position_numerator_bits: 0,
        shared_position_denominator: 0,
    };
    let entry = PageOffsetEntry {
        object_count_delta: 1,
        page_length_delta: u64::MAX,
        shared_reference_count: 0,
        shared_object_ids: Vec::new(),
        shared_position_numerators: Vec::new(),
        content_offset_delta: 5,
        content_length_delta: 9,
    };
    assert_eq!(entry.object_count(&header), u64::MAX);
    assert_eq!(entry.page_length(&header), u64::MAX);
    assert_eq!(entry.content_offset(&header), u64::MAX);
    assert_eq!(entry.content_length(&header), u64::MAX);
}

#[test]
fn declared_width_above_32_is_kept_and_reported() {
    // 35524—35525 说位宽项「have values in the range 0 through 32」；原文没有
    // 规定越界读法，故照声明值继续解析（≤64 位原语上限）并逐项上报。
    let mut w = BitWriter::default();
    write_header_with_widths(&mut w, [40, 33, 0, 0, 0, 0, 0], 65_535);
    w.unsigned(0xFF_FFFF_FFFF, 40); // 项 1 列（1 页，40 位）
    w.unsigned(1, 33); // 项 2 列（1 页，33 位）
    let bytes = w.bytes;
    let table = PageOffsetHintTable::parse(&bytes, 0, 1).expect("over-range widths still parse");
    assert_eq!(table.header.object_count_bits, 40);
    assert_eq!(table.header.page_length_bits, 33);
    assert_eq!(table.header.shared_position_denominator, 65_535);
    assert_eq!(
        table.pages[0].object_count_delta, 0xFF_FFFF_FFFF,
        "40 位全 1"
    );
    assert_eq!(table.pages[0].page_length_delta, 1);
    assert_eq!(
        table.width_problems(),
        vec![("item 3", 40), ("item 5", 33)],
        "every out-of-range width is reported, and item 13 is not a width"
    );
}

/// Table F.3 头部的定形写手：7 个位宽项（项 3/5/7/9/10/11/12）与分母（项 13）
/// 由测试给定，其余定值项取 1（值本身对本用例无意义）。
fn write_header_with_widths(w: &mut BitWriter, widths: [u64; 7], denominator: u64) {
    let [
        object,
        page_length,
        content_offset,
        content_length,
        refs,
        ids,
        numerators,
    ] = widths;
    w.unsigned(1, 32); // 项 1
    w.unsigned(1, 32); // 项 2
    w.unsigned(object, 16); // 项 3
    w.unsigned(1, 32); // 项 4
    w.unsigned(page_length, 16); // 项 5
    w.unsigned(1, 32); // 项 6
    w.unsigned(content_offset, 16); // 项 7
    w.unsigned(1, 32); // 项 8
    w.unsigned(content_length, 16); // 项 9
    w.unsigned(refs, 16); // 项 10
    w.unsigned(ids, 16); // 项 11
    w.unsigned(numerators, 16); // 项 12
    w.unsigned(denominator, 16); // 项 13（分母值）
}

#[test]
fn declared_width_above_the_primitive_limit_is_a_typed_error() {
    // 16 位位宽项可以声明到 65535；超过 64 位原语上限时无从读取，类型化报错。
    let mut w = BitWriter::default();
    write_header_with_widths(&mut w, [100, 0, 0, 0, 0, 0, 0], 1);
    // 页预算按声明宽度记账，故先给出足量位（104 位）让解析走到取项 1 的那一步。
    for _ in 0..13 {
        w.unsigned(0, 8);
    }
    let result = PageOffsetHintTable::parse(&w.bytes, 0, 1);
    assert_eq!(result.err(), Some(HintBitError::InvalidWidth { bits: 100 }));
}

#[test]
fn first_page_shared_refs_are_preserved_but_not_read_from_the_columns() {
    // Table F.4 项 3「For the first page, this number shall be 0; the next two
    // items start with the second page」：首页声明非零时列里没有它的元素，
    // 原值保留、列不被错位读取（诊断由 [`super`] 层给出）。
    let mut w = BitWriter::default();
    write_header(&mut w);
    for delta in [1, 2] {
        w.unsigned(delta, 4); // 项 1 列
    }
    for _ in 0..2 {
        w.unsigned(0, 6); // 项 2 列
    }
    w.unsigned(3, 3); // 首页：违规的非零值
    w.unsigned(1, 3); // 第二页
    w.unsigned(11, 5); // 项 4 列只含第二页的 1 个引用
    w.unsigned(2, 2); // 项 5 列同上
    w.unsigned(0, 5);
    w.unsigned(0, 5);
    w.unsigned(0, 7);
    w.unsigned(0, 7);
    let bytes = w.bytes;
    let table = PageOffsetHintTable::parse(&bytes, 0, 2).expect("two pages");
    assert_eq!(table.pages[0].shared_reference_count, 3);
    assert!(table.pages[0].shared_object_ids.is_empty());
    assert!(table.pages[0].shared_position_numerators.is_empty());
    assert_eq!(table.pages[1].shared_reference_count, 1);
    assert_eq!(table.pages[1].shared_object_ids, vec![11]);
    assert_eq!(table.pages[1].shared_position_numerators, vec![2]);
}

#[test]
fn every_truncation_of_a_real_table_is_a_typed_error() {
    let bytes = three_page_table();
    for prefix in 0..bytes.len() {
        let result = PageOffsetHintTable::parse(&bytes[..prefix], 0, 3);
        assert!(
            matches!(result, Err(HintBitError::ShortBuffer { .. })),
            "prefix {prefix}/{} must fail as typed ShortBuffer, got {result:?}",
            bytes.len()
        );
    }
    assert!(matches!(
        PageOffsetHintTable::parse(&bytes, bytes.len() + 1, 3),
        Err(HintBitError::ShortBuffer { .. })
    ));
}

#[test]
fn bogus_page_count_is_rejected_before_allocating() {
    // 页数来自不可信的 Table F.1 `/N`：三页的表声明 40 亿页须在进入循环前
    // 就被位预算挡下（不分配、不 panic）。
    let bytes = three_page_table();
    let result = PageOffsetHintTable::parse(&bytes, 0, 4_000_000_000);
    assert!(matches!(result, Err(HintBitError::ShortBuffer { .. })));
    // 位宽全 0（每页 0 位）时表无法自证页数，同样按保守上界拒绝。
    let mut w = BitWriter::default();
    write_header_with_widths(&mut w, [0, 0, 0, 0, 0, 0, 0], 1);
    let result = PageOffsetHintTable::parse(&w.bytes, 0, u64::MAX / 2);
    assert!(matches!(result, Err(HintBitError::ShortBuffer { .. })));
}

#[test]
fn zero_width_shared_columns_cap_the_declared_count() {
    // 项 11/12 位宽同为 0 时每个引用不占位，故引用总数按剩余位数保守封顶；
    // 首页 0、第二页声明 4294967295 必须在任何分配之前被拒。
    let mut w = BitWriter::default();
    // 项 10 给 32 位宽（引用数列），项 11/12 给 0 位宽。
    write_header_with_widths(&mut w, [0, 0, 0, 0, 32, 0, 0], 4);
    w.unsigned(0, 32); // 首页引用数
    w.unsigned(0xFFFF_FFFF, 32); // 第二页引用数（远超剩余位）
    let result = PageOffsetHintTable::parse(&w.bytes, 0, 2);
    assert!(matches!(result, Err(HintBitError::ShortBuffer { .. })));
}

#[test]
fn end_byte_rounds_up_to_the_next_byte_boundary() {
    // 两页、位宽项 3/5/9 各 1 位而 7/10/11/12 为 0 → 288 + 3 列×2 位 = 294 位
    // → 向上取整 37 字节。
    let mut w = BitWriter::default();
    write_header_with_widths(&mut w, [1, 1, 0, 1, 0, 0, 0], 1);
    for _ in 0..2 {
        w.unsigned(1, 1); // 项 1 列
    }
    for _ in 0..2 {
        w.unsigned(0, 1); // 项 2 列
    }
    // 项 3 列（宽度 0）不占位，项 4/5 与项 6 列同理。
    for value in [1, 0] {
        w.unsigned(value, 1); // 项 7 列
    }
    let bytes = w.bytes;
    let table = PageOffsetHintTable::parse(&bytes, 0, 2).expect("two pages");
    assert_eq!(table.bits_consumed, 294);
    assert_eq!(table.end_byte(), 37);
    assert_eq!(bytes.len(), 37);
    assert_eq!(
        table.pages[0].content_length(&table.header),
        2,
        "增量 1 + Table F.3 项 8（1）"
    );
    assert_eq!(
        table.pages[1].content_length(&table.header),
        1,
        "增量 0 → 退回 Table F.3 项 8"
    );
    assert!(table.pages[1].shared_object_ids.is_empty());
    assert_eq!(table.pages[1].shared_reference_count, 0);
}
