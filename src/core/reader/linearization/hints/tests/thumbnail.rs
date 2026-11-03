//! 缩略图 hint 表（F.4.3，Table F.7/F.8）逐字段解析用例：位级手工装配 →
//! 解析回读 → 断言 12 项头部与 a)—c) 三列的列序；零宽字段、截断的每一个前缀、
//! 不可信条目数与越界位宽一律类型化结果，且**先验预算再分配**。

use super::super::page_offset::MAX_UNEVIDENCED_ITEMS;
use super::super::thumbnail::{ThumbnailEntry, ThumbnailHeader, ThumbnailHintTable};
use crate::core::reader::linearization::hints::bits::{BitWriter, HintBitError};

/// Table F.7 头部 12 项（9×32 + 3×16 = 336 位整）。
///
/// 项 1—3 定值 30/6000/页数，项 5/7 定值 120/2（两个增量折算的最小值），项
/// 9—12 定值 40/7000/3/900（**声明而未建模**的缩略图共享对象区，须原样保留）；
/// 三个位宽项 4/6/8 由参数驱动，默认 3/5/4。
fn write_header(w: &mut BitWriter, pages: u64, gap_bits: u64, length_bits: u64, object_bits: u64) {
    w.unsigned(30, 32); // 项 1：首张缩略图的对象号
    w.unsigned(6000, 32); // 项 2：首张缩略图的位置
    w.unsigned(pages, 32); // 项 3：有缩略图的页数＝条目数
    w.unsigned(gap_bits, 16); // 项 4：Table F.8 项 1 的位宽
    w.unsigned(120, 32); // 项 5：缩略图最少长度
    w.unsigned(length_bits, 16); // 项 6：Table F.8 项 3 的位宽
    w.unsigned(2, 32); // 项 7：缩略图最少对象数
    w.unsigned(object_bits, 16); // 项 8：Table F.8 项 2 的位宽
    w.unsigned(40, 32); // 项 9：缩略图共享对象区首对象号
    w.unsigned(7000, 32); // 项 10：该区首对象位置
    w.unsigned(3, 32); // 项 11：该区对象数
    w.unsigned(900, 32); // 项 12：该区长度
}

/// 三个条目的合法表：F.4.3 a)—c) 的列序（项 1 列 → 项 2 列 → 项 3 列）。
///
/// 336 位头部 + 3×(3+4+5) = 372 位，补齐 47 字节。页号按项 1 的定义是
/// 0、2、7（条目之间分别隔着 0/1/4 页无缩略图）。
fn three_entry_table() -> Vec<u8> {
    let mut w = BitWriter::default();
    write_header(&mut w, 3, 3, 5, 4);
    for gap in [0u64, 1, 4] {
        w.unsigned(gap, 3); // a) 项 1（3 位）
    }
    for objects in [1u64, 2, 3] {
        w.unsigned(objects, 4); // b) 项 2（4 位）
    }
    for length in [7u64, 9, 31] {
        w.unsigned(length, 5); // c) 项 3（5 位）
    }
    w.bytes
}

/// 头部三项位宽全为 0、条目数仍为 `entries` 的表：条目不消费任何位。
fn zero_width_table(entries: u64) -> Vec<u8> {
    let mut w = BitWriter::default();
    write_header(&mut w, entries, 0, 0, 0);
    w.bytes
}

#[test]
fn thumbnail_table_parses_every_field_of_tables_f7_and_f8() {
    let bytes = three_entry_table();
    let table = ThumbnailHintTable::parse(&bytes, 0).expect("well-formed table");
    assert_eq!(
        table.header,
        ThumbnailHeader {
            first_thumbnail_object_number: 30,
            first_thumbnail_object_location: 6000,
            pages_with_thumbnails: 3,
            gap_count_bits: 3,
            least_image_length: 120,
            image_length_bits: 5,
            least_object_count: 2,
            object_count_bits: 4,
            first_shared_object_number: 40,
            first_shared_object_location: 7000,
            shared_object_count: 3,
            shared_section_length: 900,
        },
        "Table F.7 has 12 items and not one of them may be dropped, including the \
         unmodelled thumbnail shared objects section (items 9-12)"
    );
    assert_eq!(table.declared_entries, 3);
    assert_eq!(table.start_byte, 0);
    assert!(table.width_problems().is_empty());
    // a)—c) 三列按页序归位：项 1 列 0/1/4，项 2 列 1/2/3，项 3 列 7/9/31。
    assert_eq!(
        table.entries,
        vec![
            ThumbnailEntry {
                preceding_pages_without_thumbnail: 0,
                object_count_delta: 1,
                image_length_delta: 7,
            },
            ThumbnailEntry {
                preceding_pages_without_thumbnail: 1,
                object_count_delta: 2,
                image_length_delta: 9,
            },
            ThumbnailEntry {
                preceding_pages_without_thumbnail: 4,
                object_count_delta: 3,
                image_length_delta: 31,
            },
        ]
    );
    // Table F.8 项 2/3 是增量：分别加头部项 7/项 5。
    assert_eq!(table.entries[0].object_count(&table.header), 3);
    assert_eq!(table.entries[0].image_length(&table.header), 127);
    assert_eq!(table.entries[2].object_count(&table.header), 5);
    assert_eq!(table.entries[2].image_length(&table.header), 151);
    // 项 1 的定义：条目之间的无缩略图页数（首条目按页号序从页 0 起，35695—35697）。
    assert_eq!(table.page_of(0), Some(0));
    assert_eq!(table.page_of(1), Some(2));
    assert_eq!(table.page_of(2), Some(7));
    assert_eq!(table.page_of(3), None, "越界索引不得 panic");
    // 336 + 36 = 372 位，补齐 47 字节。
    assert_eq!(table.bits_consumed, 372);
    assert_eq!(table.end_byte(), 47);
    assert_eq!(bytes.len(), 47);
}

#[test]
fn thumbnail_table_position_is_relative_to_the_hint_stream_and_absolute_in_end_byte() {
    // Table F.2 键给的是流内字节位置；起点非 0 时字段值不变，而 `end_byte`
    // 要按绝对位置折算（F.4 前言：各表以字节边界开始且互不重叠）。
    let mut bytes = vec![0xAAu8; 5];
    bytes.extend(three_entry_table());
    let table = ThumbnailHintTable::parse(&bytes, 5).expect("table at the declared position");
    assert_eq!(table.start_byte, 5);
    assert_eq!(table.entries[2].image_length_delta, 31);
    assert_eq!(table.bits_consumed, 372);
    assert_eq!(table.end_byte(), 5 + 47);
    // 起点等于流长：一个字节都不剩，按类型化错误处理而非 panic。
    let at_end = ThumbnailHintTable::parse(&bytes, bytes.len()).unwrap_err();
    assert!(
        matches!(at_end, HintBitError::ShortBuffer { .. }),
        "{at_end}"
    );
    let beyond = ThumbnailHintTable::parse(&bytes, bytes.len() + 1).unwrap_err();
    assert!(
        matches!(beyond, HintBitError::ShortBuffer { .. }),
        "{beyond}"
    );
}

#[test]
fn thumbnail_header_fields_decode_unsigned_and_high_order_bit_first() {
    // 0xFFFF_FFFF / 0x8000_0000：32 位字段须按无符号解码（无符号扩展、无符号位）。
    let mut w = BitWriter::default();
    w.unsigned(0xFFFF_FFFF, 32);
    w.unsigned(0x8000_0000, 32);
    w.unsigned(0, 32);
    w.unsigned(1, 16);
    w.unsigned(0xFFFF_FFFF, 32);
    w.unsigned(1, 16);
    w.unsigned(0xFFFF_FFFF, 32);
    w.unsigned(1, 16);
    w.unsigned(0, 32);
    w.unsigned(0, 32);
    w.unsigned(0, 32);
    w.unsigned(0, 32);
    let table = ThumbnailHintTable::parse(&w.bytes, 0).expect("header only");
    assert_eq!(table.header.first_thumbnail_object_number, 4_294_967_295);
    assert_eq!(table.header.first_thumbnail_object_location, 2_147_483_648);
    assert_eq!(table.header.pages_with_thumbnails, 0);
    assert_eq!(table.header.least_image_length, 4_294_967_295);
    assert_eq!(table.header.least_object_count, 4_294_967_295);
    assert!(table.entries.is_empty());
    // 16 位宽度字段同样无符号：项 4/6/8 声明 1 即 Table F.8 各项占 1 位。
    assert_eq!(
        [
            table.header.gap_count_bits,
            table.header.image_length_bits,
            table.header.object_count_bits
        ],
        [1, 1, 1]
    );
}

#[test]
fn zero_width_declared_fields_consume_no_bits() {
    // Table F.7 项 4/6/8 为 0 时三列都不占位（与 F.4.1 的 0 宽字段同口径）：
    // 条目仍按项 3 成列，值全为 0，位消耗停在 336 位。
    let bytes = zero_width_table(5);
    assert_eq!(bytes.len(), 42, "the header alone is 336 bits");
    let table = ThumbnailHintTable::parse(&bytes, 0).expect("zero width table");
    assert_eq!(table.entries.len(), 5);
    assert!(table.entries.iter().all(|entry| *entry
        == ThumbnailEntry {
            preceding_pages_without_thumbnail: 0,
            object_count_delta: 0,
            image_length_delta: 0,
        }));
    assert_eq!(table.bits_consumed, 336);
    assert_eq!(table.end_byte(), 42);
    assert_eq!(table.page_of(4), Some(4), "零宽项 1 即每页都有缩略图");
    assert_eq!(table.entries[0].object_count(&table.header), 2);
    assert_eq!(table.entries[0].image_length(&table.header), 120);
}

#[test]
fn every_truncation_of_the_thumbnail_table_is_a_typed_error() {
    let full = three_entry_table();
    for prefix in 0..full.len() {
        let problem = ThumbnailHintTable::parse(&full[..prefix], 0).unwrap_err();
        assert!(
            matches!(problem, HintBitError::ShortBuffer { .. }),
            "prefix {prefix}/{} must be a typed truncation error, got {problem}",
            full.len()
        );
    }
}

#[test]
fn bogus_entry_count_is_rejected_before_allocation() {
    // 项 3 是 32 位不可信值：位宽非零时按剩余位预算挡住。
    let mut w = BitWriter::default();
    write_header(&mut w, 0xFFFF_FFFF, 3, 5, 4);
    let bytes = w.bytes;
    let problem = ThumbnailHintTable::parse(&bytes, 0).unwrap_err();
    assert!(
        matches!(problem, HintBitError::ShortBuffer { needed_bits, .. } if needed_bits > 4_000_000_000),
        "{problem}"
    );
    // 三项位宽全为 0 时位预算给不出证据：改用绝对上限，绝不按声明值分配。
    let oversized = zero_width_table(0xFFFF_FFFF);
    assert!(
        matches!(
            ThumbnailHintTable::parse(&oversized, 0).unwrap_err(),
            HintBitError::ShortBuffer { .. }
        ),
        "a declared count above the cap must be rejected before allocating"
    );
    // 上限之内（含边界值）仍可解析，且条目数恰等于声明值。
    let at_cap = zero_width_table(MAX_UNEVIDENCED_ITEMS);
    let table = ThumbnailHintTable::parse(&at_cap, 0).expect("count at the library cap");
    assert_eq!(table.entries.len(), MAX_UNEVIDENCED_ITEMS as usize);
    assert_eq!(table.declared_entries, MAX_UNEVIDENCED_ITEMS);
    assert_eq!(table.bits_consumed, 336);
}

#[test]
fn declared_width_above_the_primitive_limit_is_a_typed_error() {
    // 项 4 声明 65 位：超过 64 位原语上限，读不动即类型化错误（位预算给 1 个
    // 条目留了位置，错误出在字段宽度本身）。
    let mut w = BitWriter::default();
    write_header(&mut w, 1, 65, 5, 4);
    for _ in 0..10 {
        w.unsigned(0, 8);
    }
    let problem = ThumbnailHintTable::parse(&w.bytes, 0).unwrap_err();
    assert!(
        matches!(problem, HintBitError::InvalidWidth { bits } if bits == 65),
        "{problem}"
    );
}

#[test]
fn out_of_range_declared_widths_are_kept_and_reported() {
    // 期望范围 0—32 之外、但仍在上限之内：原值保留、按声明宽度继续读取，
    // 只由 width_problems 如实上报（保守：不猜默认值）。
    let mut w = BitWriter::default();
    write_header(&mut w, 2, 40, 5, 4);
    for gap in [0u64, 1] {
        w.unsigned(gap, 40);
    }
    for objects in [1u64, 2] {
        w.unsigned(objects, 4);
    }
    for length in [7u64, 9] {
        w.unsigned(length, 5);
    }
    let table = ThumbnailHintTable::parse(&w.bytes, 0).expect("read as declared");
    assert_eq!(table.header.gap_count_bits, 40);
    assert_eq!(table.entries[1].preceding_pages_without_thumbnail, 1);
    assert_eq!(table.width_problems(), vec![("item 4", 40)]);
    // 项 6/8 同时越界时逐项上报，顺序按表内项号。
    let mut w = BitWriter::default();
    write_header(&mut w, 0, 33, 64, 33);
    let other = ThumbnailHintTable::parse(&w.bytes, 0).expect("header only");
    assert_eq!(
        other.width_problems(),
        vec![("item 4", 33), ("item 6", 64), ("item 8", 33)]
    );
}
