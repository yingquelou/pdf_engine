//! F.4.6 内嵌文件流 hint 表写端**与既有读端往返**的单元测试：编码产物一律交回真
//! 读端 `EmbeddedFileHintTable::parse`（不是写端自己的复读），逐项断言 Table F.11
//! 七项、Table F.12 每组五项、项 5 的标识符列表连同 `shared_object_id_bits`/
//! `start_byte`/`bits_consumed`/`end_byte` 都还原成同构数据。
//!
//! 覆盖的形态（夹具与手工装配器见父模块 [`super`]）：well-formed 两组、零项组
//! （35837—35844 的合法读法）、头部项 5/项 6 声明 0 位的退化列、**文档级**项 5
//! 宽度为 0（页偏移表 Table F.3 项 11 声明 0 位）、全部声明宽度取上界 32 的边界值，
//! 以及尾部残位补零后两张表在流里字节相邻排布。读端已有的**分数位置/越界诊断**
//! 语义不被写端违反：往返产物在 `zero_item_problems`、`first_group_anchor_problem`
//! 与 `shared_index_problems` 三处一律给「无问题」。

use super::{DOCUMENT_ID_BITS, group, hand_assembled, model};
use crate::core::reader::linearization::hints::{EmbeddedFileHeader, EmbeddedFileHintTable};
use crate::core::writer::linearization::hints::embedded_file::{
    EmbeddedFileTableInput, encode_embedded_file_table,
};

/// 编码产物交回真实读端：头部 7 项、每组 5 项、项 5 的标识符列表、
/// `shared_object_id_bits` 与 `bits_consumed` 逐项相等，表长 226 位补齐 29 字节。
#[test]
fn encode_embedded_file_table_roundtrips_tables_f11_and_f12() {
    let input = model();
    let bytes = encode_embedded_file_table(&input).unwrap();
    assert_eq!(bytes, hand_assembled(&input), "位装配对拍");
    assert_eq!(input.bits_per_group(), 25); // 项 4/5/6/7 = 9 + 3 + 11 + 2
    assert_eq!(input.shared_object_id_count(), 4);
    assert_eq!(input.identifier_bits(), 16); // 4 × 文档级 4 位
    assert_eq!(input.total_bits(), 226); // 160 + 2×25 + 16
    assert_eq!(input.byte_len(), 29);
    assert_eq!(bytes.len(), 29);

    let parsed = EmbeddedFileHintTable::parse(&bytes, 0, DOCUMENT_ID_BITS).unwrap();
    assert_eq!(parsed.header, input.header, "Table F.11 七项一项不许丢");
    assert_eq!(parsed.groups, input.groups, "Table F.12 五列逐项归位");
    assert_eq!(parsed.shared_object_id_bits, DOCUMENT_ID_BITS);
    assert_eq!(parsed.start_byte, 0);
    assert_eq!(parsed.bits_consumed, 226);
    assert_eq!(parsed.end_byte(), 29);
    assert!(parsed.groups_match_header());
}

/// 零项组（项 2 与项 3 同时为 0、项 4 > 0）是 35837—35844 明文允许的形态：编码
/// 产物交回读端后条目逐项相等，且读端的三条自检都不给问题。
#[test]
fn zero_item_group_with_only_shared_references_roundtrips() {
    let input = model();
    let bytes = encode_embedded_file_table(&input).unwrap();
    let parsed = EmbeddedFileHintTable::parse(&bytes, 0, DOCUMENT_ID_BITS).unwrap();
    let shared_only = &parsed.groups[1];
    assert_eq!(
        [
            shared_only.object_number,
            shared_only.object_count,
            shared_only.group_length,
            shared_only.shared_reference_count,
        ],
        [320, 0, 0, 3],
        "零项组的四项原值逐项还原"
    );
    assert_eq!(shared_only.shared_object_ids, vec![1, 0, 2]);
    assert!(shared_only.zero_item_problems().is_empty());
    assert!(
        parsed.zero_item_problems().is_empty(),
        "全表无互斥 shall 违规"
    );
    // 第一组（项 2 = 2）由头部项 1 锚定：301 落在闭区间 [300, 301]（35792）。
    assert_eq!(parsed.first_group_anchor_problem(), None);
    // 项 5 的标识符落进 F.4.2 共享对象表的 3 个条目范围（下标自 0 起）。
    assert!(parsed.shared_index_problems(3).is_empty());
    assert_eq!(parsed.header.group_count, 2);
}

/// 零宽声明形态：项 5 与项 6 都为 0 ⇒ 每组的项 2/项 3 只能是 0（35837—35844 的
/// 合法读法），两组都退化成「只有共享对象引用」；表长只剩 160 + 2×(9+2) 位。
#[test]
fn zero_object_and_length_widths_roundtrip_as_shared_only_groups() {
    let mut input = model();
    input.header.group_object_bits = 0;
    input.header.group_length_bits = 0;
    input.groups = vec![group(5, 0, 0, &[0, 1]), group(9, 0, 0, &[3])];
    input.header.group_count = 2;
    assert_eq!(input.bits_per_group(), 11); // 9 + 0 + 0 + 2
    assert_eq!(input.total_bits(), 160 + 22 + 12);
    let bytes = encode_embedded_file_table(&input).unwrap();
    assert_eq!(bytes, hand_assembled(&input));
    assert_eq!(bytes.len(), 25); // 194 位 → 补齐 25 字节
    let parsed = EmbeddedFileHintTable::parse(&bytes, 0, DOCUMENT_ID_BITS).unwrap();
    assert_eq!(parsed.groups, input.groups);
    assert!(
        parsed
            .groups
            .iter()
            .all(|entry| entry.zero_item_problems().is_empty())
    );
    assert_eq!(parsed.bits_consumed, 194);
}

/// 文档级项 5 宽度为 0（页偏移表 Table F.3 项 11 声明 0 位）：每组的项 4 须为 0、
/// 项 5 的列表为空，列一个位也不占，表长恒 160 + 组数 × 列宽之和。
#[test]
fn zero_document_width_roundtrips_groups_without_shared_references() {
    let mut input = model();
    input.shared_object_id_bits = 0;
    input.groups = vec![group(301, 2, 600, &[]), group(320, 1, 300, &[])];
    input.header.group_count = 2;
    assert_eq!(input.identifier_bits(), 0);
    assert_eq!(input.total_bits(), 210); // 160 + 2×25，项 5 列不占位
    let bytes = encode_embedded_file_table(&input).unwrap();
    assert_eq!(bytes, hand_assembled(&input));
    assert_eq!(bytes.len(), 27);
    let parsed = EmbeddedFileHintTable::parse(&bytes, 0, 0).unwrap();
    assert_eq!(parsed.groups, input.groups);
    assert_eq!(parsed.shared_object_id_bits, 0);
    assert_eq!(parsed.bits_consumed, 210);
}

/// 声明宽度的上界（0—32）都能往返：四列各 32 位 + 文档级 32 位，各项取 `2^32-1`
/// 边界值仍逐项相等（头部项 4—7 按 16 位存储，32 装得下）。
#[test]
fn the_top_of_every_declared_width_roundtrips() {
    let edge = (1u64 << 32) - 1;
    let input = EmbeddedFileTableInput {
        header: EmbeddedFileHeader {
            first_object_number: edge,
            first_object_location: edge,
            group_count: 1,
            object_number_bits: 32,
            group_object_bits: 32,
            group_length_bits: 32,
            shared_ref_count_bits: 32,
        },
        groups: vec![group(edge, edge, edge, &[edge])],
        shared_object_id_bits: 32,
    };
    assert_eq!(input.bits_per_group(), 128);
    assert_eq!(input.total_bits(), 160 + 128 + 32);
    let bytes = encode_embedded_file_table(&input).unwrap();
    assert_eq!(bytes, hand_assembled(&input));
    assert_eq!(bytes.len(), 40); // 320 位整
    let parsed = EmbeddedFileHintTable::parse(&bytes, 0, 32).unwrap();
    assert_eq!(parsed.header, input.header);
    assert_eq!(parsed.groups, input.groups, "u64 边界值不被截断");
    assert_eq!(parsed.groups[0].shared_object_ids, vec![edge]);
    assert_eq!(parsed.bits_consumed, 320);
}

/// 尾部残位补零到字节边界（226 位 → 29 字节，末字节只剩 6 个零位）：读端只消费
/// 前面的位，补零不影响往返，且两张表可字节相邻排布（35481—35483）。
#[test]
fn residual_bits_are_zero_padded_and_tables_tile_the_stream() {
    let input = model();
    let bytes = encode_embedded_file_table(&input).unwrap();
    assert_eq!(input.total_bits() % 8, 2);
    assert_eq!(bytes.last().unwrap() & 0b0011_1111, 0, "只用了 2 位");

    let mut stream = bytes.clone();
    stream.extend_from_slice(&encode_embedded_file_table(&input).unwrap());
    stream.push(0xAA); // 下一张表的起点仍在
    let second = EmbeddedFileHintTable::parse(&stream, 29, DOCUMENT_ID_BITS).unwrap();
    assert_eq!(second.start_byte, 29);
    assert_eq!(second.header, input.header);
    assert_eq!(second.groups, input.groups);
    assert_eq!(second.bits_consumed, 226);
    assert_eq!(second.end_byte(), 58);
    assert_eq!(stream[58], 0xAA);
}
