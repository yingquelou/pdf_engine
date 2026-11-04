//! F.4.1 页偏移 hint 表写端的单元测试：交错列序落位、声明宽度门、首页 shall、
//! 计数自洽、位预算上限与零填充行为，全部以读端 `PageOffsetHintTable::parse` 回读对拍。

use super::*;
use crate::core::reader::linearization::hints::{PageOffsetHintTable, SharedObjectPosition};

/// 与读端集成脚手架 `page_offset_table_bits` 同值的多页模型：头部十三项
/// 3/700/3/300/6/20/5/50/8/2/4/3/4；两页增量与共享对象列。
fn model() -> PageOffsetTableInput {
    PageOffsetTableInput {
        header: PageOffsetHeader {
            least_objects_per_page: 3,
            first_page_object_location: 700,
            object_count_bits: 3,
            least_page_length: 300,
            page_length_bits: 6,
            least_content_offset: 20,
            content_offset_bits: 5,
            least_content_length: 50,
            content_length_bits: 8,
            shared_ref_count_bits: 2,
            shared_object_id_bits: 4,
            shared_position_numerator_bits: 3,
            shared_position_denominator: 4,
        },
        pages: vec![
            PageOffsetEntry {
                object_count_delta: 0,
                page_length_delta: 10,
                shared_reference_count: 0,
                shared_object_ids: vec![],
                shared_position_numerators: vec![],
                content_offset_delta: 4,
                content_length_delta: 11,
            },
            PageOffsetEntry {
                object_count_delta: 1,
                page_length_delta: 25,
                shared_reference_count: 2,
                shared_object_ids: vec![1, 2],
                shared_position_numerators: vec![2, 0],
                content_offset_delta: 7,
                content_length_delta: 13,
            },
        ],
    }
}

/// 编码产物交回读端 `parse`：头部十三项、逐页七个字段（含项 4/5 向量）、
/// `declared_pages` 与 `bits_consumed` 逐项相等。
#[test]
fn encode_page_offset_table_roundtrips_header_and_interleaved_columns() {
    let input = model();
    let bytes = encode_page_offset_table(&input).unwrap();
    assert_eq!(input.total_bits(), 350);
    assert_eq!(bytes.len(), 44); // 350 位补齐到 44 字节（末字节补 2 个零位）
    let parsed = PageOffsetHintTable::parse(&bytes, 0, input.pages.len() as u64).unwrap();
    assert_eq!(parsed.header, input.header);
    assert_eq!(parsed.pages, input.pages);
    assert_eq!(parsed.declared_pages, 2);
    assert_eq!(parsed.bits_consumed, input.total_bits());
    assert_eq!(parsed.end_byte(), input.end_byte());
    assert!(parsed.width_problems().is_empty());
}

/// 头部 13 项定宽落位：前 36 字节逐项等于表内声明宽度下的按值大端拼装，
/// 证明项序与位宽与读端解析完全一致。
#[test]
fn header_items_land_in_declared_fixed_width_order() {
    let bytes = encode_page_offset_table(&model()).unwrap();
    // 项序 32/32/16/32/16/32/16/32/16/16/16/16/16（35526—35559）。
    let mut expected = Vec::new();
    let push32 = |buf: &mut Vec<u8>, value: u64| {
        buf.extend_from_slice(&(value as u32).to_be_bytes());
    };
    let push16 = |buf: &mut Vec<u8>, value: u64| {
        buf.extend_from_slice(&(value as u16).to_be_bytes());
    };
    push32(&mut expected, 3); // 项 1 leastObjects
    push32(&mut expected, 700); // 项 2 firstPageObjPos
    push16(&mut expected, 3); // 项 3 bits
    push32(&mut expected, 300); // 项 4 leastPageSize
    push16(&mut expected, 6); // 项 5 bits
    push32(&mut expected, 20); // 项 6 leastContentOffset
    push16(&mut expected, 5); // 项 7 bits
    push32(&mut expected, 50); // 项 8 leastContentLength
    push16(&mut expected, 8); // 项 9 bits
    push16(&mut expected, 2); // 项 10 bits
    push16(&mut expected, 4); // 项 11 bits
    push16(&mut expected, 3); // 项 12 bits
    push16(&mut expected, 4); // 项 13 分母值
    assert_eq!(expected.len(), 36);
    assert_eq!(&bytes[..36], expected.as_slice());
}

/// 三页 + 分数位置四类取值（0、d-1、d、d+1）：解码后读端类型化类别逐项归位。
#[test]
fn fractional_positions_roundtrip_as_typed_categories() {
    let mut input = model();
    input.pages.push(PageOffsetEntry {
        object_count_delta: 2,
        page_length_delta: 63,
        shared_reference_count: 4,
        shared_object_ids: vec![0, 1, 2, 3],
        shared_position_numerators: vec![0, 3, 4, 5],
        content_offset_delta: 0,
        content_length_delta: 0,
    });
    input.header.shared_object_id_bits = 8;
    input.header.shared_ref_count_bits = 3; // 计数 4 需 3 位
    let bytes = encode_page_offset_table(&input).unwrap();
    let parsed = PageOffsetHintTable::parse(&bytes, 0, 3).unwrap();
    assert_eq!(parsed.pages, input.pages);
    let third = 2usize;
    assert_eq!(
        parsed.shared_object_position(third, 0),
        Some(SharedObjectPosition::ContentStreamFraction {
            numerator: 0,
            denominator: 4
        })
    );
    assert_eq!(
        parsed.shared_object_position(third, 1),
        Some(SharedObjectPosition::ContentStreamFraction {
            numerator: 3,
            denominator: 4
        })
    );
    assert_eq!(
        parsed.shared_object_position(third, 2),
        Some(SharedObjectPosition::BeforeTrailingObjects {
            numerator: 4,
            denominator: 4
        })
    );
    assert_eq!(
        parsed.shared_object_position(third, 3),
        Some(SharedObjectPosition::AfterTrailingObjects {
            numerator: 5,
            denominator: 4
        })
    );
}

/// 增量列的类型化访问器（项 1/2/6/7 加头部最小值）在往返后仍与模型一致。
#[test]
fn derived_accessors_match_the_model_after_roundtrip() {
    let input = model();
    let bytes = encode_page_offset_table(&input).unwrap();
    let parsed = PageOffsetHintTable::parse(&bytes, 0, 2).unwrap();
    assert_eq!(parsed.pages[0].object_count(&parsed.header), 3);
    assert_eq!(parsed.pages[1].object_count(&parsed.header), 4);
    assert_eq!(parsed.pages[0].page_length(&parsed.header), 310);
    assert_eq!(parsed.pages[1].page_length(&parsed.header), 325);
    assert_eq!(parsed.pages[0].content_offset(&parsed.header), 24);
    assert_eq!(parsed.pages[1].content_offset(&parsed.header), 27);
    assert_eq!(parsed.pages[0].content_length(&parsed.header), 61);
    assert_eq!(parsed.pages[1].content_length(&parsed.header), 63);
    assert_eq!(parsed.second_group_max_object(), Some(4));
    assert_eq!(parsed.shared_reference_total(), 2);
    assert_eq!(parsed.page_start_byte(700, 1), Some(1_010));
    assert_eq!(parsed.page_end_byte(700, 1), Some(1_335));
}

/// 七个声明宽度任一越出 0—32（35524—35525 的 shall）即类型化拒绝、零字节。
#[test]
fn declared_width_above_32_is_rejected_before_any_byte() {
    for index in 0..7 {
        let mut input = model();
        let slot = match index {
            0 => &mut input.header.object_count_bits,
            1 => &mut input.header.page_length_bits,
            2 => &mut input.header.content_offset_bits,
            3 => &mut input.header.content_length_bits,
            4 => &mut input.header.shared_ref_count_bits,
            5 => &mut input.header.shared_object_id_bits,
            _ => &mut input.header.shared_position_numerator_bits,
        };
        *slot = MAX_F3_DECLARED_WIDTH + 1;
        let error =
            encode_page_offset_table(&input).expect_err("a 33-bit declared width must be refused");
        assert_eq!(
            match error {
                HintEncodeError::DeclaredWidthOutOfRange { bits, .. } => Some(bits),
                _ => None,
            },
            Some(33),
            "index {index} must yield DeclaredWidthOutOfRange"
        );
    }
}

/// 增量值超出声明宽度（读端会截断高位）即拒 [`HintEncodeError::ValueTooWide`]，
/// `Result::Err` 意味着零字节产出。
#[test]
fn delta_over_declared_width_is_rejected_with_zero_bytes() {
    let mut input = model();
    input.pages[1].page_length_delta = 1u64 << input.header.page_length_bits; // 64 ≥ 2^6
    assert_eq!(
        encode_page_offset_table(&input),
        Err(HintEncodeError::ValueTooWide { value: 64, bits: 6 })
    );
    let mut input = model();
    input.pages[0].object_count_delta = 1; // 宽度 3 内合法，先把项 13 分母推过 16 位
    input.header.shared_position_denominator = 1u64 << 16;
    assert_eq!(
        encode_page_offset_table(&input),
        Err(HintEncodeError::ValueTooWide {
            value: 65_536,
            bits: 16
        })
    );
}

/// Table F.4 项 3 的首页 shall（35579—35581）：计数非零、或首页携带项 4/5
/// 元素，都拒绝——conforming writer 不产出读端要靠诊断兜底的形态。
#[test]
fn first_page_shared_reference_shall_is_enforced() {
    let mut input = model();
    input.pages[0].shared_reference_count = 3;
    assert_eq!(
        encode_page_offset_table(&input),
        Err(HintEncodeError::FirstPageSharedRefs {
            count: 3,
            ids: 0,
            numerators: 0
        })
    );
    let mut input = model();
    input.pages[0].shared_object_ids = vec![0];
    input.pages[0].shared_position_numerators = vec![1];
    assert!(matches!(
        encode_page_offset_table(&input),
        Err(HintEncodeError::FirstPageSharedRefs {
            count: 0,
            ids: 1,
            numerators: 1
        })
    ));
}

/// 第二页起项 4/5 的元素数须逐项等于项 3 声明值——读端按计数重建向量。
#[test]
fn spread_vectors_must_match_declared_counts() {
    let mut input = model();
    input.pages[1].shared_object_ids.pop(); // 声明 2，实际 1
    assert_eq!(
        encode_page_offset_table(&input),
        Err(HintEncodeError::SharedReferenceSpreadMismatch {
            page: 1,
            declared: 2,
            ids: 1,
            numerators: 2
        })
    );
}

/// 空页向量违反「one or more per-page entries」（35508—35509）。
#[test]
fn empty_page_vector_is_rejected() {
    let mut input = model();
    input.pages.clear();
    assert_eq!(
        encode_page_offset_table(&input),
        Err(HintEncodeError::PageEntriesMissing)
    );
}

/// 位预算门的对偶：a)/b)/c) 三列宽度全 0 且页数超 65 536、或 d)/e) 两列
/// 宽度全 0 且引用总数超 65 536，读端必拒——写端同口径提前拒绝。
#[test]
fn unevidenced_counts_beyond_the_readers_cap_are_rejected() {
    let mut input = model();
    for width in [
        &mut input.header.object_count_bits,
        &mut input.header.page_length_bits,
        &mut input.header.shared_ref_count_bits,
    ] {
        *width = 0;
    }
    input.pages[1].shared_reference_count = 0;
    input.pages[1].shared_object_ids.clear();
    input.pages[1].shared_position_numerators.clear();
    input.pages[1].content_offset_delta = 0;
    input.pages[0].content_offset_delta = 0;
    input.pages[0].content_length_delta = 0;
    input.pages[1].content_length_delta = 0;
    input.pages = vec![input.pages[0].clone(); MAX_UNEVIDENCED_ITEMS as usize + 1];
    assert_eq!(
        encode_page_offset_table(&input),
        Err(HintEncodeError::UnevidencedItemsTooLarge {
            kind: "F.4.1 Table F.4 per-page entries",
            count: MAX_UNEVIDENCED_ITEMS + 1,
            limit: MAX_UNEVIDENCED_ITEMS,
        })
    );
    let mut input = model();
    input.header.shared_object_id_bits = 0;
    input.header.shared_position_numerator_bits = 0;
    input.header.shared_ref_count_bits = 17;
    input.pages[1].shared_reference_count = MAX_UNEVIDENCED_ITEMS + 1;
    input.pages[1].shared_object_ids = vec![0u64; MAX_UNEVIDENCED_ITEMS as usize + 1];
    input.pages[1].shared_position_numerators = vec![0u64; MAX_UNEVIDENCED_ITEMS as usize + 1];
    assert_eq!(
        encode_page_offset_table(&input),
        Err(HintEncodeError::UnevidencedItemsTooLarge {
            kind: "F.4.1 Table F.4 items 4/5 shared references",
            count: MAX_UNEVIDENCED_ITEMS + 1,
            limit: MAX_UNEVIDENCED_ITEMS,
        })
    );
}

/// 全列宽度 0 的最小形态：36 字节整（头部 288 位）、无残位；读端回读相等。
#[test]
fn all_zero_widths_roundtrip_as_a_pure_36_byte_header() {
    let input = PageOffsetTableInput {
        header: PageOffsetHeader {
            least_objects_per_page: 1,
            first_page_object_location: 0,
            object_count_bits: 0,
            least_page_length: 1,
            page_length_bits: 0,
            least_content_offset: 0,
            content_offset_bits: 0,
            least_content_length: 0,
            content_length_bits: 0,
            shared_ref_count_bits: 0,
            shared_object_id_bits: 0,
            shared_position_numerator_bits: 0,
            shared_position_denominator: 0,
        },
        pages: vec![PageOffsetEntry {
            object_count_delta: 0,
            page_length_delta: 0,
            shared_reference_count: 0,
            shared_object_ids: vec![],
            shared_position_numerators: vec![],
            content_offset_delta: 0,
            content_length_delta: 0,
        }],
    };
    let bytes = encode_page_offset_table(&input).unwrap();
    assert_eq!(bytes.len(), 36);
    let parsed = PageOffsetHintTable::parse(&bytes, 0, 1).unwrap();
    assert_eq!(parsed.header, input.header);
    assert_eq!(parsed.pages, input.pages);
}

/// 尾随位非字节边界时补零（F.4 前言位流形态）：读端只消费前 350 位，
/// 剩余 6 位补零不影响解析，且补零字节给下一张表留足字节边界。
#[test]
fn residual_bits_are_zero_padded_to_the_byte_boundary() {
    let input = model();
    let bytes = encode_page_offset_table(&input).unwrap();
    assert_eq!(input.total_bits() % 8, 6);
    assert_eq!(bytes.len(), input.total_bits() as usize / 8 + 1);
    assert_eq!(bytes.last().unwrap() & 0b0000_0011, 0); // 补零的是末尾 2 位
    // 补零字节后再接另一张表：读端从 44 字节处仍能解出邻居。
    let mut stream = bytes.clone();
    stream.extend_from_slice(&[0xAA]);
    let parsed = PageOffsetHintTable::parse(&stream, 0, 2).unwrap();
    assert_eq!(parsed.end_byte(), 44);
    assert_eq!(stream[parsed.end_byte() as usize], 0xAA);
}
