//! F.4.1 页偏移 hint 表**写端**的集成验证：用 `HintTableEncoder` 编码 Table F.3
//! 头部 + Table F.4 交错列，与 `support.rs` 手工位装配**逐字节相等**；再注入
//! [`super`] 的真线性化装配器，从公开 API（`Document::linearization_hints`）
//! 解码回来，逐项（含增量访问器与跨页派生：对象号链、页位置链、分数位置类别
//! 与主 hint 流修正）断言等于编码模型。另有一条负例：超出声明宽度的增量值在
//! 写出任何字节之前被类型化拒绝。

use super::*;
use pdf_engine::core::reader::linearization::hints::{
    PageOffsetEntry, PageOffsetHeader, SharedObjectPosition,
};
use pdf_engine::core::writer::linearization::{
    HintEncodeError, HintTableEncoder, PageOffsetTableInput,
};

/// 与 `support::page_offset_table_bits(0, 2)` 逐项同值的编码模型：头部十三项
/// 3/700/3/300/6/20/5/50/8/2/4/3/4，两页增量与共享对象列。
fn encoded_model() -> PageOffsetTableInput {
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

/// 写端编码与手工位装配逐字节相等（交错列序 a)—g) 的终极对拍），且经真线性化
/// 文件全链路解码后 Table F.3/F.4 每个字段都等于编码模型。
#[test]
fn writer_page_offset_table_matches_handassembly_and_roundtrips_every_field() {
    let input = encoded_model();
    let encoded = HintTableEncoder::new()
        .encode_page_offset_table(&input)
        .expect("a conforming model must encode");
    assert_eq!(encoded, page_offset_table_bits(0, 2));
    assert_eq!(encoded.len(), page_offset_bytes());
    assert_eq!(input.total_bits(), 350);

    // 注入分部装配器：页偏移表在流偏移 0（F.4 前言），其后紧跟共享对象表，
    // `/S` 定位恰好取本表补齐后的字节长。
    let mut payload = encoded.clone();
    payload.extend(shared_object_table_bits());
    let entries = format!(" /S {}", encoded.len());
    let bytes = build(&payload, &entries);
    let document = parse_buf(&bytes);
    let hints = document
        .linearization_hints()
        .expect("no I/O error")
        .expect("hint stream present");
    assert!(hints.is_consistent(), "hint issues: {:?}", hints.issues);

    let table = hints.page_offset.as_ref().expect("stream offset 0");
    assert_eq!(table.header, input.header);
    assert_eq!(table.pages, input.pages);
    assert_eq!(table.declared_pages, PAGE_COUNT as u64);
    assert_eq!(table.bits_consumed, input.total_bits());
    assert_eq!(table.end_byte(), encoded.len() as u64);
    assert_eq!(hints.page_offset_end_byte(), Some(table.end_byte()));
    assert_eq!(hints.position_of("S"), Some(table.end_byte()));
    assert_eq!(hints.stream_bytes, payload.len());
    assert!(table.width_problems().is_empty());
    assert!(
        hints.shared_object.is_some(),
        "F.4.1 不挤掉紧随其后的共享对象表"
    );

    // 增量列的类型化访问器（Table F.4 项 1/2/6/7 加头部最小值）逐项等于模型。
    assert_eq!(table.pages[0].object_count(&table.header), 3);
    assert_eq!(table.pages[1].object_count(&table.header), 4);
    assert_eq!(table.pages[0].page_length(&table.header), 310);
    assert_eq!(table.pages[1].page_length(&table.header), 325);
    assert_eq!(table.pages[0].content_offset(&table.header), 24);
    assert_eq!(table.pages[1].content_offset(&table.header), 27);
    assert_eq!(table.pages[0].content_length(&table.header), 61);
    assert_eq!(table.pages[1].content_length(&table.header), 63);

    // 识别与解码都不改变普通路径：两页可读、零告警。
    assert_eq!(document.page_count().expect("page count"), 2);
    assert!(document.warnings().is_empty());
}

/// 跨页派生（slice 128 口径）在解码后的表上仍与模型逐项一致：对象号链、
/// 页位置累加、主 hint 流修正与分数位置类别。
#[test]
fn writer_page_offset_table_drives_reader_derived_accumulation_and_fractions() {
    let input = encoded_model();
    let encoded = HintTableEncoder::new()
        .encode_page_offset_table(&input)
        .unwrap();
    let mut payload = encoded;
    payload.extend(shared_object_table_bits());
    let bytes = build(&payload, &format!(" /S {}", input.end_byte()));
    let document = parse_buf(&bytes);
    let hints = document
        .linearization_hints()
        .expect("no I/O error")
        .expect("hint stream present");
    let table = hints.page_offset.as_ref().expect("stream offset 0");

    // 项 1 的对象号链：首页取参数字典 `/O`（夹具里是对象 7），第二页恒 1。
    assert_eq!(hints.first_page_object, Some(7));
    assert_eq!(hints.page_first_object_number(0), Some(7));
    assert_eq!(hints.page_first_object_number(1), Some(1));
    assert_eq!(hints.page_first_object_number(2), None);
    assert_eq!(table.second_group_max_object(), Some(4));

    // 项 2 的页位置累加（hint 表空间）与主 hint 流修正（F.4 前言 NOTE 3）。
    assert_eq!(table.page_start_byte(700, 0), Some(700));
    assert_eq!(table.page_start_byte(700, 1), Some(1_010));
    assert_eq!(table.page_end_byte(700, 0), Some(1_010));
    assert_eq!(table.page_span(700, 1), Some((1_010, 325)));
    assert_eq!(hints.page_start_byte(1), Some(hints.adjust_position(1_010)));
    assert_eq!(hints.page_end_byte(1), Some(hints.adjust_position(1_335)));

    // 项 5 + 头部项 13 的分数位置类别（分子 0/2、分母 4 都落内容流前段）。
    assert_eq!(table.shared_reference_total(), 2);
    assert_eq!(
        hints.shared_object_position(1, 0),
        Some(SharedObjectPosition::ContentStreamFraction {
            numerator: 2,
            denominator: 4
        })
    );
    assert_eq!(
        hints.shared_object_position(1, 1),
        Some(SharedObjectPosition::ContentStreamFraction {
            numerator: 0,
            denominator: 4
        })
    );
    assert_eq!(hints.shared_object_position(0, 0), None);
}

/// 负例：增量值超出表头声明宽度（读端会截断高位、往返不再相等）即类型化拒绝，
/// `Result::Err` 意味着零字节产出——半成品位流绝不外流。
#[test]
fn writer_rejects_delta_over_declared_width_with_zero_bytes() {
    let mut input = encoded_model();
    // 项 2 增量的声明宽度是 6 位（头部项 5），64 恰好越界一格。
    input.pages[1].page_length_delta = 1u64 << input.header.page_length_bits;
    assert_eq!(
        HintTableEncoder::new().encode_page_offset_table(&input),
        Err(HintEncodeError::ValueTooWide { value: 64, bits: 6 })
    );
    // 首页项 3 非零违反 shall（35579—35581）同样拒绝，零字节。
    let mut input = encoded_model();
    input.pages[0].shared_reference_count = 1;
    assert!(matches!(
        HintTableEncoder::new().encode_page_offset_table(&input),
        Err(HintEncodeError::FirstPageSharedRefs { count: 1, .. })
    ));
}
