//! 附录 F.4.3 缩略图 hint 表**写端**的集成验证：用 `encode_thumbnail_table`
//! 编码 Table F.7 的 12 项头部（9×32 + 3×16 ＝ 336 位）与 Table F.8 的三列增量
//! （宽度依次取头部项 4/8/6），与手工位装配**逐字节相等**；再把编出的表插进
//! [`super`] 的真线性化装配器（页偏移表 → 缩略图表 → 共享对象表，三段字节相邻），
//! 从公开 API（`Document::linearization_hints`）解码回来，逐项断言等于编码模型，
//! 并核对 Table F.2 键 `T` 的取位、`thumbnail_end_byte()` 与 `S` 的最小合法位置。
//!
//! 另钉住两件事：项 1 的页号派生（35765—35697 区间里的「how many pages without
//! a thumbnail image lie between the previous entry's page and this page」）在
//! 全链路上仍逐项等于模型，以及写端的四类不自洽模型（项 3 与向量不符、声明宽度
//! 越出 0—32、增量超出列宽、空向量）一律零字节拒绝——半成品位流绝不进装配器。

use super::{
    BitWriter, build, page_offset_bytes, page_offset_table_bits, shared_object_table_bits,
};
use crate::support::parse_buf;
use pdf_engine::core::reader::linearization::hints::{ThumbnailEntry, ThumbnailHeader};
use pdf_engine::core::writer::linearization::HintEncodeError;
use pdf_engine::core::writer::linearization::hints::thumbnail::{
    ThumbnailEncodeError, ThumbnailTableInput, encode_thumbnail_table,
};

/// 合规表长：336 位头部 + 2×(2+3+6) 位＝358 位 → 补齐 45 字节。
const TABLE_BYTES: usize = 45;

/// Table F.7 头部的声明值（项 1/2/5/7/9/10/11/12 定值，项 3/4/6/8 由参数驱动）。
fn header(entries: u64, gap_bits: u32, length_bits: u32, object_bits: u32) -> ThumbnailHeader {
    ThumbnailHeader {
        first_thumbnail_object_number: 11,
        first_thumbnail_object_location: 900,
        pages_with_thumbnails: entries,
        gap_count_bits: gap_bits,
        least_image_length: 400,
        image_length_bits: length_bits,
        least_object_count: 1,
        object_count_bits: object_bits,
        first_shared_object_number: 50,
        first_shared_object_location: 9000,
        shared_object_count: 4,
        shared_section_length: 1500,
    }
}

/// 两页都有缩略图的合规模型（项 1 恒 0，故页号 0 与 1）：对象增量 1/2、
/// 长度增量 5/63，三列宽度依次 2/3/6 位。
fn model() -> ThumbnailTableInput {
    ThumbnailTableInput {
        header: header(2, 2, 6, 3),
        entries: vec![
            ThumbnailEntry {
                preceding_pages_without_thumbnail: 0,
                object_count_delta: 1,
                image_length_delta: 5,
            },
            ThumbnailEntry {
                preceding_pages_without_thumbnail: 0,
                object_count_delta: 2,
                image_length_delta: 63,
            },
        ],
    }
}

/// 手工按 Table F.7/F.8 的项序与 a)—c) 列序装配（与读端家族集成夹具同形），
/// 用于逐字节对拍。
fn hand_assembled(input: &ThumbnailTableInput) -> Vec<u8> {
    let mut w = BitWriter::default();
    let h = &input.header;
    w.unsigned(h.first_thumbnail_object_number, 32); // 项 1
    w.unsigned(h.first_thumbnail_object_location, 32); // 项 2
    w.unsigned(h.pages_with_thumbnails, 32); // 项 3
    w.unsigned(u64::from(h.gap_count_bits), 16); // 项 4
    w.unsigned(h.least_image_length, 32); // 项 5
    w.unsigned(u64::from(h.image_length_bits), 16); // 项 6
    w.unsigned(h.least_object_count, 32); // 项 7
    w.unsigned(u64::from(h.object_count_bits), 16); // 项 8
    w.unsigned(h.first_shared_object_number, 32); // 项 9
    w.unsigned(h.first_shared_object_location, 32); // 项 10
    w.unsigned(h.shared_object_count, 32); // 项 11
    w.unsigned(h.shared_section_length, 32); // 项 12
    for entry in &input.entries {
        w.unsigned(entry.preceding_pages_without_thumbnail, h.gap_count_bits); // a) 项 1
    }
    for entry in &input.entries {
        w.unsigned(entry.object_count_delta, h.object_count_bits); // b) 项 2
    }
    for entry in &input.entries {
        w.unsigned(entry.image_length_delta, h.image_length_bits); // c) 项 3
    }
    w.finish()
}

/// 页偏移表 → 编码出的缩略图表 → 共享对象表，三段字节相邻的载荷（F.4 前言：
/// 各表以字节边界开始且互不重叠，35481—35483）。
fn assembled(input: &ThumbnailTableInput) -> (Vec<u8>, String) {
    let encoded = encode_thumbnail_table(input).expect("conforming model");
    assert_eq!(encoded.len(), TABLE_BYTES, "表长由本表头部算出");
    let mut payload = page_offset_table_bits(0, 2);
    payload.extend_from_slice(&encoded);
    let start = page_offset_bytes();
    let entries = format!(
        " /T {} /S {}",
        start,
        start + usize::try_from(input.byte_len()).expect("table fits a usize")
    );
    payload.extend(shared_object_table_bits());
    (payload, entries)
}

/// 写端编码与手工位装配逐字节相等（Table F.7 项序与 Table F.8 列序的终极对拍），
/// 且经真线性化文件全链路解码后 12 + 3×2 个字段连同游标都等于编码模型。
#[test]
fn writer_thumbnail_table_matches_handassembly_and_roundtrips_every_item() {
    let input = model();
    let encoded = encode_thumbnail_table(&input).expect("a conforming model must encode");
    assert_eq!(encoded, hand_assembled(&input), "位装配对拍");
    assert_eq!(input.bits_per_entry(), 11); // 项 4/8/6 = 2 + 3 + 6
    assert_eq!(input.total_bits(), 358); // 336 位头部 + 2×11 位
    assert_eq!(input.byte_len(), TABLE_BYTES as u64);
    assert_eq!(encoded.len(), TABLE_BYTES);
    assert_eq!(&encoded[..42], hand_assembled(&input)[..42].as_ref());
    assert_eq!(&encoded[42], &0b0000_0010); // a) 列四个 0 位 + b) 列 001 与 0
    assert_eq!(&encoded[43], &0b1000_0101); // b) 列余下两位 + c) 列的 5（000101）
    assert_eq!(&encoded[44], &0b1111_1100); // c) 列的 63（111111）+ 收尾补零 2 位

    let (payload, entries) = assembled(&input);
    let document = parse_buf(&build(&payload, &entries));
    assert_eq!(document.page_count().expect("page count"), 2);
    let hints = document
        .linearization_hints()
        .expect("no I/O error")
        .expect("hint stream present");
    assert!(hints.is_consistent(), "hint issues: {:?}", hints.issues);

    // 表按 Table F.2 键 `T` 定位：起点即页偏移表的末字节（本表不占流偏移 0）。
    let start = page_offset_bytes() as u64;
    assert_eq!(hints.position_of("T"), Some(start));
    let table = hints.thumbnail.as_ref().expect("encoded thumbnail table");
    assert_eq!(table.start_byte, start);
    assert_eq!(table.header, input.header, "Table F.7 十二项一项不许丢");
    assert_eq!(table.entries, input.entries, "Table F.8 三列逐项归位");
    assert_eq!(table.declared_entries, 2);
    assert_eq!(table.bits_consumed, input.total_bits());
    assert_eq!(table.end_byte(), start + TABLE_BYTES as u64);
    assert_eq!(hints.thumbnail_end_byte(), Some(table.end_byte()));
    assert!(table.width_problems().is_empty());
    assert_eq!(
        [
            table.header.first_thumbnail_object_number,
            table.header.first_thumbnail_object_location,
            table.header.least_image_length,
            table.header.least_object_count,
            table.header.first_shared_object_number,
            table.header.first_shared_object_location,
            table.header.shared_object_count,
            table.header.shared_section_length,
        ],
        [11, 900, 400, 1, 50, 9000, 4, 1500],
        "声明而未建模的缩略图共享对象区（项 9—12）原样保留"
    );

    // 增量列的语义（35768—35773）：项 2 加头部项 7、项 3 加头部项 5。
    assert_eq!(table.entries[0].object_count(&table.header), 2);
    assert_eq!(table.entries[1].object_count(&table.header), 3);
    assert_eq!(table.entries[0].image_length(&table.header), 405);
    assert_eq!(table.entries[1].image_length(&table.header), 463);
    assert_eq!(table.page_of(0), Some(0));
    assert_eq!(table.page_of(1), Some(1));
    assert_eq!(table.page_of(2), None);

    // 相邻两家不受影响：`S` 恰落在本表末字节之后，表互不重叠。
    assert_eq!(hints.position_of("S"), Some(table.end_byte()));
    assert_eq!(hints.stream_bytes, payload.len());
    assert!(hints.page_offset.is_some() && hints.shared_object.is_some());
    assert!(hints.generic.is_empty() && hints.extended.is_empty());
    // 项 2 是文件偏移，仍按 F.4 前言 NOTE 3（35490—35492）修正。
    assert!(table.header.first_thumbnail_object_location > hints.primary.offset);
    assert_eq!(
        hints.adjust_position(table.header.first_thumbnail_object_location),
        table.header.first_thumbnail_object_location + hints.primary.length
    );
    assert!(document.warnings().is_empty());
}

/// 项 1 的间隔语义在全链路上照样成立：三条目的间隔 0/1/4 → 页号 0/2/7，且
/// 「Thumbnail images may exist for some pages and not for others」（35696—35697）
/// 不触发任何诊断——F.4.3 没有规定项 1 与字典 `/N` 的核对，读端不猜、写端不补。
#[test]
fn writer_thumbnail_gaps_roundtrip_page_numbers_through_the_parser() {
    let input = ThumbnailTableInput {
        header: header(3, 3, 5, 4),
        entries: [(0u64, 1u64, 7u64), (1, 2, 9), (4, 3, 31)]
            .into_iter()
            .map(|(gap, objects, length)| ThumbnailEntry {
                preceding_pages_without_thumbnail: gap,
                object_count_delta: objects,
                image_length_delta: length,
            })
            .collect(),
    };
    let encoded = encode_thumbnail_table(&input).expect("conforming model");
    assert_eq!(encoded, hand_assembled(&input));
    assert_eq!(input.total_bits(), 372); // 336 + 3×(3+4+5)
    assert_eq!(encoded.len(), 47);

    let mut payload = page_offset_table_bits(0, 2);
    payload.extend_from_slice(&encoded);
    let shared_at = payload.len();
    payload.extend(shared_object_table_bits());
    let entries = format!(" /T {} /S {}", page_offset_bytes(), shared_at);

    let document = parse_buf(&build(&payload, &entries));
    let hints = document
        .linearization_hints()
        .expect("no I/O error")
        .expect("hint stream present");
    assert!(hints.is_consistent(), "hint issues: {:?}", hints.issues);
    let table = hints.thumbnail.as_ref().expect("three-entry table");
    assert_eq!(table.entries, input.entries);
    assert_eq!(table.declared_entries, 3);
    assert_eq!(table.bits_consumed, 372);
    assert_eq!(table.end_byte(), (page_offset_bytes() + 47) as u64);
    assert_eq!(hints.position_of("S"), Some(table.end_byte()));
    assert_eq!(
        (0..4).map(|index| table.page_of(index)).collect::<Vec<_>>(),
        vec![Some(0), Some(2), Some(7), None],
        "项 1 累加出的页号与模型逐项一致"
    );
    assert_eq!(table.entries[2].object_count(&table.header), 4);
    assert_eq!(table.entries[2].image_length(&table.header), 431);
    assert!(hints.page_offset.is_some() && hints.shared_object.is_some());
    assert_eq!(document.page_count().expect("page count"), 2);
}

/// 退化声明形态（项 4/6/8 全为 0）：条目一个位也不占，编出的表恒 42 字节整，
/// 注入真文件后读端按项 3 补出全 0 增量，派生值即头部两项最小值，且相邻的共享
/// 对象表起点按**实际字节长**推进。
#[test]
fn writer_thumbnail_zero_width_columns_roundtrip_through_the_parser() {
    let input = ThumbnailTableInput {
        header: header(2, 0, 0, 0),
        entries: vec![
            ThumbnailEntry {
                preceding_pages_without_thumbnail: 0,
                object_count_delta: 0,
                image_length_delta: 0,
            };
            2
        ],
    };
    let encoded = encode_thumbnail_table(&input).expect("zero widths are legal declarations");
    assert_eq!(encoded, hand_assembled(&input));
    assert_eq!(input.total_bits(), 336);
    assert_eq!(encoded.len(), 42, "只有 42 字节的头部");

    let mut payload = page_offset_table_bits(0, 2);
    payload.extend_from_slice(&encoded);
    let shared_at = payload.len();
    payload.extend(shared_object_table_bits());
    let entries = format!(" /T {} /S {}", page_offset_bytes(), shared_at);

    let document = parse_buf(&build(&payload, &entries));
    let hints = document
        .linearization_hints()
        .expect("no I/O error")
        .expect("hint stream present");
    assert!(hints.is_consistent(), "hint issues: {:?}", hints.issues);
    let table = hints.thumbnail.as_ref().expect("zero-width table");
    assert_eq!(table.entries.len(), 2);
    assert!(table.entries.iter().all(|entry| *entry
        == ThumbnailEntry {
            preceding_pages_without_thumbnail: 0,
            object_count_delta: 0,
            image_length_delta: 0,
        }));
    assert_eq!(table.bits_consumed, 336);
    assert_eq!(table.end_byte(), (page_offset_bytes() + 42) as u64);
    assert_eq!(hints.position_of("S"), Some(table.end_byte()));
    assert_eq!(table.entries[0].object_count(&table.header), 1);
    assert_eq!(table.entries[1].image_length(&table.header), 400);
    assert_eq!(table.page_of(1), Some(1));
    assert!(hints.shared_object.is_some(), "退化表不挤掉相邻家族");
    assert_eq!(document.page_count().expect("page count"), 2);
}

/// 负例：模型不自洽即在写出任何位之前被类型化拒绝，`Result::Err` 意味着零字节
/// 产出——半成品位流绝不进装配器。
#[test]
fn writer_rejects_inconsistent_thumbnail_models_with_zero_bytes() {
    // 项 3 与条目向量不符：读端按项 3（35720）决定 Table F.8 读几个条目。
    let mut input = model();
    input.header.pages_with_thumbnails = 3;
    assert_eq!(
        encode_thumbnail_table(&input),
        Err(ThumbnailEncodeError::EntryCountMismatch {
            declared: 3,
            actual: 2
        })
    );
    // 声明宽度越出 0—32（35524—35525 的同一措辞类比 35721—35728）。
    let mut input = model();
    input.header.gap_count_bits = 33;
    assert_eq!(
        encode_thumbnail_table(&input),
        Err(ThumbnailEncodeError::WidthOutOfRange {
            item: "item 4",
            bits: 33
        })
    );
    // 增量超出声明列宽（读端会截断高位，往返不再逐项相等）：64 装不进 6 位。
    let mut input = model();
    input.entries[1].image_length_delta = 64;
    assert_eq!(
        encode_thumbnail_table(&input),
        Err(ThumbnailEncodeError::Bit(HintEncodeError::ValueTooWide {
            value: 64,
            bits: 6
        }))
    );
    // 空向量违反「shall include one or more per-page entries」（35693—35694）。
    let mut input = model();
    input.entries.clear();
    input.header.pages_with_thumbnails = 0;
    assert_eq!(
        encode_thumbnail_table(&input),
        Err(ThumbnailEncodeError::EntriesMissing)
    );
}
