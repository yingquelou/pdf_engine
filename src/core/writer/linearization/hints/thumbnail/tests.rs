//! F.4.3 缩略图 hint 表写端的单元测试：Table F.7 十二项的定宽落位、Table F.8
//! 三列按 a)—c) 的**列主序**打包（项 1 列 → 项 2 列 → 项 3 列，宽度依次取头部
//! 项 4/8/6）、增量与页号派生值在解码后仍等于模型，以及四类拒绝（声明宽度越出
//! 0—32、条目向量空、项 3 与向量不自洽、宽度全 0 的列长上限）与位级溢出全部在
//! 写出第一位之前给出、零字节产出。对拍一律交回真实读端
//! `ThumbnailHintTable::parse`，期望位图案由读端测试装配用的
//! [`BitWriter`] 手工拼装。

use super::*;
use crate::core::reader::linearization::hints::ThumbnailHintTable;
use crate::core::reader::linearization::hints::bits::BitWriter;

/// 与读端单元夹具 `three_entry_table` 逐项同值的模型：头部 30/6000/3 条目/
/// 项 4＝3 位/最少长度 120/项 6＝5 位/最少对象数 2/项 8＝4 位/共享区
/// 40/7000/3/900；三条目间隔 0/1/4 页、对象增量 1/2/3、长度增量 7/9/31。
fn model() -> ThumbnailTableInput {
    ThumbnailTableInput {
        header: ThumbnailHeader {
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
        entries: (0..3)
            .map(|index| ThumbnailEntry {
                preceding_pages_without_thumbnail: [0u64, 1, 4][index],
                object_count_delta: [1u64, 2, 3][index],
                image_length_delta: [7u64, 9, 31][index],
            })
            .collect(),
    }
}

/// 一个 `count` 条全 0 增量的条目向量（零宽列与上限用例的夹具）。
fn zero_entries(count: usize) -> Vec<ThumbnailEntry> {
    vec![
        ThumbnailEntry {
            preceding_pages_without_thumbnail: 0,
            object_count_delta: 0,
            image_length_delta: 0,
        };
        count
    ]
}

/// 手工按 Table F.7/F.8 的项序与列序装配位图案（F.4.3 a)—c)，35740—35743）。
fn hand_assembled(input: &ThumbnailTableInput) -> Vec<u8> {
    let mut w = BitWriter::default();
    let h = &input.header;
    w.unsigned(h.first_thumbnail_object_number, 32); // 项 1：首张缩略图的对象号
    w.unsigned(h.first_thumbnail_object_location, 32); // 项 2：首张缩略图的位置
    w.unsigned(h.pages_with_thumbnails, 32); // 项 3：有缩略图的页数＝条目数
    w.unsigned(u64::from(h.gap_count_bits), 16); // 项 4：Table F.8 项 1 的位宽
    w.unsigned(h.least_image_length, 32); // 项 5：缩略图最少长度
    w.unsigned(u64::from(h.image_length_bits), 16); // 项 6：Table F.8 项 3 的位宽
    w.unsigned(h.least_object_count, 32); // 项 7：缩略图最少对象数
    w.unsigned(u64::from(h.object_count_bits), 16); // 项 8：Table F.8 项 2 的位宽
    w.unsigned(h.first_shared_object_number, 32); // 项 9：共享区首对象号
    w.unsigned(h.first_shared_object_location, 32); // 项 10：共享区首对象位置
    w.unsigned(h.shared_object_count, 32); // 项 11：共享区对象数
    w.unsigned(h.shared_section_length, 32); // 项 12：共享区长度
    for entry in &input.entries {
        w.unsigned(entry.preceding_pages_without_thumbnail, h.gap_count_bits); // a)
    }
    for entry in &input.entries {
        w.unsigned(entry.object_count_delta, h.object_count_bits); // b)
    }
    for entry in &input.entries {
        w.unsigned(entry.image_length_delta, h.image_length_bits); // c)
    }
    w.bytes
}

/// 编码产物交回真实读端：头部 12 项、条目向量、`declared_entries` 与
/// `bits_consumed` **整体**相等，表长 372 位补齐 47 字节。
#[test]
fn encode_thumbnail_table_roundtrips_tables_f7_and_f8() {
    let input = model();
    let bytes = encode_thumbnail_table(&input).unwrap();
    assert_eq!(bytes, hand_assembled(&input));
    assert_eq!(input.bits_per_entry(), 12); // 项 4/8/6 = 3 + 4 + 5
    assert_eq!(input.total_bits(), 372); // 336 位头部 + 3×12 位
    assert_eq!(input.byte_len(), 47);
    assert_eq!(bytes.len(), 47);
    let parsed = ThumbnailHintTable::parse(&bytes, 0).unwrap();
    assert_eq!(parsed.header, input.header, "Table F.7 十二项一项不许丢");
    assert_eq!(parsed.entries, input.entries, "Table F.8 三列逐项归位");
    assert_eq!(parsed.declared_entries, 3);
    assert_eq!(parsed.start_byte, 0);
    assert_eq!(parsed.bits_consumed, 372);
    assert!(parsed.width_problems().is_empty());
    assert_eq!(
        parsed,
        ThumbnailHintTable {
            header: input.header,
            start_byte: 0,
            declared_entries: 3,
            entries: input.entries,
            bits_consumed: 372,
        }
    );
}

/// 头部 12 项定宽落位：逐项等于 32/32/32/16/32/16/32/16/32/32/32/32 的按值大端
/// 拼装，证明项序与位宽与读端解析完全一致（35717—35735、35760—35762）。
#[test]
fn header_items_land_in_the_table_f7_fixed_width_order() {
    let input = model();
    let bytes = encode_thumbnail_table(&input).unwrap();
    let mut expected = Vec::new();
    for value in [
        input.header.first_thumbnail_object_number,
        input.header.first_thumbnail_object_location,
        input.header.pages_with_thumbnails,
    ] {
        expected.extend_from_slice(&(value as u32).to_be_bytes());
    }
    let push16 = |buf: &mut Vec<u8>, value: u32| {
        buf.extend_from_slice(
            &u16::try_from(value)
                .expect("16-bit width field")
                .to_be_bytes(),
        );
    };
    push16(&mut expected, input.header.gap_count_bits); // 项 4（16 位存储）
    expected.extend_from_slice(&(input.header.least_image_length as u32).to_be_bytes());
    push16(&mut expected, input.header.image_length_bits); // 项 6
    expected.extend_from_slice(&(input.header.least_object_count as u32).to_be_bytes());
    push16(&mut expected, input.header.object_count_bits); // 项 8
    for value in [
        input.header.first_shared_object_number,
        input.header.first_shared_object_location,
        input.header.shared_object_count,
        input.header.shared_section_length,
    ] {
        expected.extend_from_slice(&(value as u32).to_be_bytes());
    }
    assert_eq!(expected.len(), 42, "头部 336 位＝42 字节整");
    assert_eq!(&bytes[..42], expected.as_slice());
    assert_eq!(&bytes[0..4], &30u32.to_be_bytes()); // 项 1
    assert_eq!(&bytes[4..8], &6000u32.to_be_bytes()); // 项 2
    assert_eq!(&bytes[8..12], &3u32.to_be_bytes()); // 项 3
    assert_eq!(&bytes[12..14], &3u16.to_be_bytes()); // 项 4 是 16 位存储
    assert_eq!(&bytes[14..18], &120u32.to_be_bytes()); // 项 5
    assert_eq!(&bytes[26..34], &[0, 0, 0, 40, 0, 0, 27, 88]); // 项 9/10（7000）
    assert_eq!(&bytes[34..42], &[0, 0, 0, 3, 0, 0, 3, 132]); // 项 11/12（900）
}

/// Table F.8 的三列是**列主序**（a) 项 1 全部条目 → b) 项 2 全部条目 → c) 项 3
/// 全部条目，35740—35743）：与按条目交错（条目内逐项 1/2/3 相邻）的装配明显不同，
/// 故列序写错必然被对拍抓住。夹具值：间隔 0/1/4（3 位）、对象增量 1/2/3（4 位）、
/// 长度增量 7/9/31（5 位）。
#[test]
fn per_page_columns_follow_the_f43_column_major_order() {
    let input = model();
    let bytes = encode_thumbnail_table(&input).unwrap();
    assert_eq!(&bytes[42], &0b0000_0110); // a) 列 000 001 1|0
    assert_eq!(&bytes[43], &0b0000_1001); // a) 列尾 + b) 列 0001 001|
    assert_eq!(&bytes[44], &0b0001_1001); // b) 列 0|0011 + c) 列 00111 的前三位
    assert_eq!(&bytes[45], &0b1101_0011); // c) 列 01001 与 11111 的交错
    assert_eq!(&bytes[46], &0b1111_0000); // c) 列末四位 + 收尾补零的 4 位

    let mut entry_major = BitWriter::default();
    for entry in &input.entries {
        entry_major.unsigned(entry.preceding_pages_without_thumbnail, 3);
        entry_major.unsigned(entry.object_count_delta, 4);
        entry_major.unsigned(entry.image_length_delta, 5);
    }
    assert_ne!(
        &bytes[42..],
        entry_major.bytes.as_slice(),
        "条目内交错是 F.4.3 之外的另一种读法，不得与 a)—c) 的列序混淆"
    );
}

/// 增量列的语义（35768—35773）与项 1 的页号派生（35765—35767）在往返后仍逐项
/// 等于模型：派生值属读端，本模块不复制、只保证编出的增量原样解回。
#[test]
fn derived_values_match_the_model_after_roundtrip() {
    let input = model();
    let bytes = encode_thumbnail_table(&input).unwrap();
    let table = ThumbnailHintTable::parse(&bytes, 0).unwrap();
    assert_eq!(table.declared_entries, table.entries.len() as u64);
    assert_eq!(table.end_byte(), 47);
    for (index, entry) in table.entries.iter().enumerate() {
        assert_eq!(
            entry.object_count(&table.header),
            input.header.least_object_count + input.entries[index].object_count_delta
        );
        assert_eq!(
            entry.image_length(&table.header),
            input.header.least_image_length + input.entries[index].image_length_delta
        );
    }
    assert_eq!(table.entries[0].object_count(&table.header), 3);
    assert_eq!(table.entries[2].image_length(&table.header), 151);
    // 项 1 的定义：条目之间隔着 0/1/4 页无缩略图，故页号 0、2、7。
    assert_eq!(
        (0..4).map(|index| table.page_of(index)).collect::<Vec<_>>(),
        vec![Some(0), Some(2), Some(7), None]
    );
}

/// 项 4/6/8 全为 0：三列一个位也不占，表长恒 336 位（42 字节整），读端按项 3
/// 补出全 0 条目（**待核**里 Table F.8 项 1「(Optional)」的 0 宽读法，写端同形）。
#[test]
fn zero_width_columns_roundtrip_as_zero_entry_values() {
    let input = ThumbnailTableInput {
        header: ThumbnailHeader {
            pages_with_thumbnails: 5,
            gap_count_bits: 0,
            image_length_bits: 0,
            object_count_bits: 0,
            ..model().header
        },
        entries: zero_entries(5),
    };
    let bytes = encode_thumbnail_table(&input).unwrap();
    assert_eq!(input.bits_per_entry(), 0);
    assert_eq!(input.total_bits(), HEADER_BITS);
    assert_eq!(bytes.len(), 42);
    assert_eq!(bytes, hand_assembled(&input));
    let table = ThumbnailHintTable::parse(&bytes, 0).unwrap();
    assert_eq!(table.entries, input.entries);
    assert_eq!(table.bits_consumed, HEADER_BITS);
    assert_eq!(table.end_byte(), 42);
    assert_eq!(table.page_of(4), Some(4), "零宽项 1 即每页都有缩略图");
}

/// 32 位上界与 32 位列宽都在声明范围内：九个 32 位项取 `u32::MAX`、项 4/6/8 取
/// 32、增量取 `u32::MAX` 与 0，共 336 + 2×96 = 528 位 = 66 字节整。
#[test]
fn the_top_of_every_declared_width_roundtrips() {
    let top = u64::from(u32::MAX);
    let input = ThumbnailTableInput {
        header: ThumbnailHeader {
            first_thumbnail_object_number: top,
            first_thumbnail_object_location: top,
            pages_with_thumbnails: 2,
            gap_count_bits: 32,
            least_image_length: top,
            image_length_bits: 32,
            least_object_count: top,
            object_count_bits: 32,
            first_shared_object_number: top,
            first_shared_object_location: top,
            shared_object_count: top,
            shared_section_length: top,
        },
        entries: vec![
            ThumbnailEntry {
                preceding_pages_without_thumbnail: top,
                object_count_delta: top,
                image_length_delta: top,
            },
            ThumbnailEntry {
                preceding_pages_without_thumbnail: 0,
                object_count_delta: 0,
                image_length_delta: 0,
            },
        ],
    };
    let bytes = encode_thumbnail_table(&input).unwrap();
    assert_eq!(input.total_bits(), 528);
    assert_eq!(bytes.len(), 66);
    assert_eq!(&bytes[0..4], &u32::MAX.to_be_bytes()); // 项 1
    assert_eq!(&bytes[12..14], &32u16.to_be_bytes()); // 项 4
    assert_eq!(&bytes[24..26], &32u16.to_be_bytes()); // 项 8
    assert_eq!(&bytes[..26], hand_assembled(&input)[..26].as_ref());
    // 列主序：a) 列两条（`u32::MAX` 与 0）→ b) 列两条 → c) 列两条，各 4 字节。
    let mut columns = Vec::new();
    for _ in 0..3 {
        columns.extend_from_slice(&u32::MAX.to_be_bytes());
        columns.extend_from_slice(&0u32.to_be_bytes());
    }
    assert_eq!(&bytes[42..66], columns.as_slice());
    let table = ThumbnailHintTable::parse(&bytes, 0).unwrap();
    assert_eq!(table.header.gap_count_bits, 32);
    assert_eq!(table.header.least_image_length, top);
    assert_eq!(table.entries, input.entries);
    assert_eq!(table.bits_consumed, 528);
    assert!(table.width_problems().is_empty());
}

/// 声明位宽越出 0—32（35524—35525 的同一措辞类比 35721—35728）即类型化拒绝，
/// 项名与读端 `width_problems` 同形，零字节。
#[test]
fn declared_width_above_32_is_rejected_before_any_byte() {
    for (item, bits) in [("item 4", 33u32), ("item 6", 40), ("item 8", 64)] {
        let mut input = model();
        match item {
            "item 4" => input.header.gap_count_bits = bits,
            "item 6" => input.header.image_length_bits = bits,
            _ => input.header.object_count_bits = bits,
        }
        assert_eq!(
            encode_thumbnail_table(&input),
            Err(ThumbnailEncodeError::WidthOutOfRange {
                item,
                bits: u64::from(bits)
            }),
            "{item} 声明 {bits} 位须被拒绝"
        );
    }
    // 16 位存储的上界同样越界即拒（读端能原值保留，写端不产出该形态）。
    let mut input = model();
    input.header.object_count_bits = u32::from(u16::MAX);
    assert_eq!(
        encode_thumbnail_table(&input),
        Err(ThumbnailEncodeError::WidthOutOfRange {
            item: "item 8",
            bits: u64::from(u16::MAX)
        })
    );
}

/// 35693—35694 的 shall：缩略图区「shall include one or more per-page entries」
/// ——空向量一律先拒（连项 3 也声明 0 的自洽形态也不例外），零字节。
#[test]
fn empty_entry_vector_is_rejected_before_any_byte() {
    for declared in [0u64, 3] {
        let mut input = model();
        input.entries.clear();
        input.header.pages_with_thumbnails = declared;
        assert_eq!(
            encode_thumbnail_table(&input),
            Err(ThumbnailEncodeError::EntriesMissing),
            "项 3 声明 {declared} 也不许产出只有头部的表"
        );
    }
}

/// 项 3 与条目向量必须严格等长：读端按项 3（35720）决定 Table F.8 读几个条目，
/// 多写少写都无从逐项往返，故零字节拒绝（含 32 位计数大得无法成为向量长度的形态）。
#[test]
fn entry_count_mismatch_is_rejected_with_zero_bytes() {
    for declared in [2u64, 4, 1u64 << 32, u64::MAX] {
        let mut input = model();
        input.header.pages_with_thumbnails = declared;
        assert_eq!(
            encode_thumbnail_table(&input),
            Err(ThumbnailEncodeError::EntryCountMismatch {
                declared,
                actual: 3
            }),
            "项 3 声明 {declared} 而向量有 3 条"
        );
    }
}

/// 给定宽度下的数值溢出（读端会截断高位）即类型化拒绝，`Result::Err` 意味着
/// 零字节产出——半成品位流绝不外流。
#[test]
fn value_over_a_declared_width_is_rejected_and_yields_no_bytes() {
    for index in 0..8 {
        let mut input = model();
        match index {
            0 => input.header.first_thumbnail_object_number = 1u64 << 32,
            1 => input.header.first_thumbnail_object_location = 1u64 << 32,
            2 => input.header.least_image_length = 1u64 << 32,
            3 => input.header.least_object_count = 1u64 << 32,
            4 => input.header.first_shared_object_number = 1u64 << 32,
            5 => input.header.first_shared_object_location = 1u64 << 32,
            6 => input.header.shared_object_count = 1u64 << 32,
            _ => input.header.shared_section_length = 1u64 << 32,
        }
        assert_eq!(
            encode_thumbnail_table(&input),
            Err(ThumbnailEncodeError::Bit(HintEncodeError::ValueTooWide {
                value: 1u64 << 32,
                bits: 32
            })),
            "32 位定宽的第 {index} 项越界须被拒绝"
        );
    }
    // 三列增量各自超出头部声明的列宽（8 装不进 3 位、16 装不进 4 位、32 装不进 5 位）。
    for (index, bits) in [(0u64, 3u32), (1, 4), (2, 5)] {
        let mut input = model();
        let slot = match index {
            0 => &mut input.entries[2].preceding_pages_without_thumbnail,
            1 => &mut input.entries[2].object_count_delta,
            _ => &mut input.entries[2].image_length_delta,
        };
        *slot = 1u64 << bits;
        assert_eq!(
            encode_thumbnail_table(&input),
            Err(ThumbnailEncodeError::Bit(HintEncodeError::ValueTooWide {
                value: 1u64 << bits,
                bits
            })),
            "列宽 {bits} 的增量须挡住越界值"
        );
    }
    // 宽度 0 时列只接受值 0。
    let mut input = model();
    input.header.gap_count_bits = 0;
    input.entries[1].preceding_pages_without_thumbnail = 1;
    assert_eq!(
        encode_thumbnail_table(&input),
        Err(ThumbnailEncodeError::Bit(HintEncodeError::ValueTooWide {
            value: 1,
            bits: 0
        }))
    );
}

/// 位预算门的对偶：项 4/6/8 全为 0 时条目不占位、位流给不出条目数的证据，读端
/// 对这类计数设 65 536 绝对上限并拒绝——写端同口径提前拒绝，上限之内照常编码。
#[test]
fn unevidenced_entries_beyond_the_readers_cap_are_rejected() {
    for count in [MAX_UNEVIDENCED_ITEMS + 1, MAX_UNEVIDENCED_ITEMS + 2] {
        let mut input = model();
        input.header.gap_count_bits = 0;
        input.header.image_length_bits = 0;
        input.header.object_count_bits = 0;
        input.header.pages_with_thumbnails = count;
        input.entries = zero_entries(count as usize);
        assert_eq!(
            encode_thumbnail_table(&input),
            Err(ThumbnailEncodeError::UnevidencedEntriesTooLarge {
                count,
                limit: MAX_UNEVIDENCED_ITEMS
            }),
            "宽度全 0 的 {count} 个条目须被拒绝"
        );
    }
    // 上限内通过：表长仍恒 336 位（条目一个位也不占）。
    let mut input = model();
    input.header.gap_count_bits = 0;
    input.header.image_length_bits = 0;
    input.header.object_count_bits = 0;
    input.header.pages_with_thumbnails = MAX_UNEVIDENCED_ITEMS;
    input.entries = zero_entries(MAX_UNEVIDENCED_ITEMS as usize);
    let bytes = encode_thumbnail_table(&input).unwrap();
    assert_eq!(bytes.len(), 42);
    let table = ThumbnailHintTable::parse(&bytes, 0).unwrap();
    assert_eq!(table.entries.len(), MAX_UNEVIDENCED_ITEMS as usize);
    assert_eq!(table.declared_entries, MAX_UNEVIDENCED_ITEMS);
    assert_eq!(table.bits_consumed, HEADER_BITS);
}

/// 尾部残位补零到字节边界（372 位 → 47 字节，末字节只剩 4 个零位）：读端只消费
/// 前面的位，补零不影响往返，且补齐后下一张表仍自字节边界开始（35481—35483）。
#[test]
fn residual_bits_are_zero_padded_and_tables_tile_the_stream() {
    let input = model();
    let bytes = encode_thumbnail_table(&input).unwrap();
    assert_eq!(input.total_bits() % 8, 4);
    assert_eq!(bytes.len(), input.byte_len() as usize);
    assert_eq!(bytes.last().unwrap() & 0b0000_1111, 0);
    // 两张表字节相邻：第二张从自己的起点解出，`end_byte` 链到下一张的下界。
    let mut stream = bytes.clone();
    stream.extend_from_slice(&encode_thumbnail_table(&input).unwrap());
    stream.push(0xAA); // 第三张表的起点仍在
    let second = ThumbnailHintTable::parse(&stream, 47).unwrap();
    assert_eq!(second.start_byte, 47);
    assert_eq!(second.header, input.header);
    assert_eq!(second.entries, input.entries);
    assert_eq!(second.bits_consumed, 372);
    assert_eq!(second.end_byte(), 94);
    assert_eq!(stream[94], 0xAA);
}
