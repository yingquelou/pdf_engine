//! 附录 F.4.6 内嵌文件流 hint 表**写端**的集成验证：用 `encode_embedded_file_table`
//! 编码 Table F.11 的 7 项头部（3×32 + 4×16 ＝ 160 位）与 Table F.12 的**项主序**
//! （转置）五列（35797—35799；列宽依次取头部项 4/5/6/7 与**文档级** Table F.3
//! 项 11），与手工位装配**逐字节相等**；再把编出的表插进 [`super`] 的真线性化装配器
//! （页偏移表 → 内嵌文件流表 → 共享对象表，三段字节相邻），从公开 API
//! （`Document::linearization_hints`）解码回来，逐项断言等于编码模型，并核对
//! Table F.2 键 `B` 的取位、`embedded_file_end_byte()` 与 `S` 的最小合法位置。
//!
//! 另钉住三件只有整篇文档才看得见的事：项 5 的列宽**只**来自页偏移表 Table F.3
//! 项 11（写端把它当显式入参，读端从在场的页偏移表取，两侧必须落到同一个值，
//! 35847 的「See Table F.3, item 11」）、头部项 2 是文件偏移仍按 F.4 前言 NOTE 3
//! （35490—35492）修正，以及写端的七类不自洽模型一律零字节拒绝——半成品位流绝不
//! 进装配器。

use super::{
    BitWriter, build, page_offset_bytes, page_offset_table_bits, shared_object_table_bits,
};
use crate::support::parse_buf;
use pdf_engine::core::reader::linearization::hints::{EmbeddedFileGroupEntry, EmbeddedFileHeader};
use pdf_engine::core::writer::linearization::hints::embedded_file::{
    EmbeddedFileEncodeError, EmbeddedFileTableInput, encode_embedded_file_table,
};
use pdf_engine::core::writer::linearization::{HintEncodeError, HintTableEncoder};

/// 合规表长：160 位头部 + 2×(9+3+11+2) 位四列 + 4×4 位项 5 列＝226 位 → 29 字节。
const TABLE_BYTES: usize = 29;

/// 脚手架页偏移表 Table F.3 项 11 声明的**文档级**宽度（本表项 5 的唯一宽度来源）。
const DOCUMENT_ID_BITS: u32 = 4;

/// Table F.11 项 1—3（首组首对象号 / 首组首位置 / 组数），与读端家族夹具同值。
const FIRST_OBJECT_NUMBER: u64 = 300;
const FIRST_OBJECT_LOCATION: u64 = 31_000;

/// Table F.11 项 4—7 的声明宽度（＝Table F.12 项 1—4 的列宽）。
const WIDTHS: [u32; 4] = [9, 3, 11, 2];

/// Table F.11 头部：项 1—3 定宽，项 4—7 取 `WIDTHS`，项 3 即组数。
fn header(group_count: u64) -> EmbeddedFileHeader {
    EmbeddedFileHeader {
        first_object_number: FIRST_OBJECT_NUMBER,
        first_object_location: FIRST_OBJECT_LOCATION,
        group_count,
        object_number_bits: WIDTHS[0],
        group_object_bits: WIDTHS[1],
        group_length_bits: WIDTHS[2],
        shared_ref_count_bits: WIDTHS[3],
    }
}

/// Table F.12 的一个条目：项 4 与项 5 的列表长度一律自洽（项 4＝列表长度）。
fn group(
    object_number: u64,
    object_count: u64,
    group_length: u64,
    shared_object_ids: &[u64],
) -> EmbeddedFileGroupEntry {
    EmbeddedFileGroupEntry {
        object_number,
        object_count,
        group_length,
        shared_reference_count: shared_object_ids.len() as u64,
        shared_object_ids: shared_object_ids.to_vec(),
    }
}

/// well-formed 两组模型：组 0「有对象也有共享引用」，组 1 是项 2 与项 3 同时为 0、
/// 项 4 = 3 的**零项组**（35837—35844 的合法读法）。
fn model() -> EmbeddedFileTableInput {
    EmbeddedFileTableInput {
        header: header(2),
        groups: vec![group(301, 2, 600, &[2]), group(320, 0, 0, &[1, 0, 2])],
        shared_object_id_bits: DOCUMENT_ID_BITS,
    }
}

/// 手工按 Table F.11 项序 + Table F.12 **项主序**五列装配（与读端家族集成夹具
/// `conforming_bits` 同形），用于逐字节对拍。
fn hand_assembled(input: &EmbeddedFileTableInput) -> Vec<u8> {
    let mut w = BitWriter::default();
    let h = &input.header;
    w.unsigned(h.first_object_number, 32); // 项 1
    w.unsigned(h.first_object_location, 32); // 项 2
    w.unsigned(h.group_count, 32); // 项 3
    for bits in [
        h.object_number_bits,
        h.group_object_bits,
        h.group_length_bits,
        h.shared_ref_count_bits,
    ] {
        w.unsigned(u64::from(bits), 16); // 项 4/5/6/7
    }
    for entry in &input.groups {
        w.unsigned(entry.object_number, h.object_number_bits); // 项 1 列
    }
    for entry in &input.groups {
        w.unsigned(entry.object_count, h.group_object_bits); // 项 2 列
    }
    for entry in &input.groups {
        w.unsigned(entry.group_length, h.group_length_bits); // 项 3 列
    }
    for entry in &input.groups {
        w.unsigned(entry.shared_reference_count, h.shared_ref_count_bits); // 项 4 列
    }
    for entry in &input.groups {
        for &id in &entry.shared_object_ids {
            w.unsigned(id, input.shared_object_id_bits); // 项 5 列（逐组相接）
        }
    }
    w.finish()
}

/// 页偏移表 → 编码出的内嵌文件流表 → 共享对象表，三段字节相邻的载荷与键位条目
/// （F.4 前言：各表以字节边界开始且互不重叠，35481—35483）。
fn assembled(input: &EmbeddedFileTableInput) -> (Vec<u8>, String) {
    let encoded = encode_embedded_file_table(input).expect("conforming model");
    assert_eq!(encoded.len(), TABLE_BYTES, "表长由本表头部与列宽算出");
    let mut payload = page_offset_table_bits(0, 2);
    payload.extend_from_slice(&encoded);
    let start = page_offset_bytes();
    let entries = format!(
        " /B {} /S {}",
        start,
        start + usize::try_from(input.byte_len()).expect("table fits a usize")
    );
    payload.extend(shared_object_table_bits());
    (payload, entries)
}

/// 写端编码与手工位装配逐字节相等（Table F.11 项序与 Table F.12 列序的终极对拍），
/// 且经真线性化文件全链路解码后 7 + 5×2 个字段连同游标都等于编码模型。
#[test]
fn writer_embedded_file_table_matches_handassembly_and_roundtrips_every_item() {
    let input = model();
    // 门面入口与自由函数必须给出同一批字节。
    let encoder = HintTableEncoder::new();
    let encoded = encoder
        .encode_embedded_file_table(&input)
        .expect("a conforming model must encode");
    assert_eq!(
        encoded,
        encode_embedded_file_table(&input).expect("same model"),
        "门面与自由函数同一实现"
    );
    assert_eq!(encoded, hand_assembled(&input), "位装配对拍");
    assert_eq!(input.bits_per_group(), 25); // 项 4/5/6/7 = 9 + 3 + 11 + 2
    assert_eq!(input.shared_object_id_count(), 4);
    assert_eq!(input.identifier_bits(), 16); // 4 × 文档级 4 位
    assert_eq!(input.total_bits(), 226); // 160 + 2×25 + 16
    assert_eq!(input.byte_len(), TABLE_BYTES as u64);

    let (payload, entries) = assembled(&input);
    let document = parse_buf(&build(&payload, &entries));
    assert_eq!(document.page_count().expect("page count"), 2);
    let hints = document
        .linearization_hints()
        .expect("no I/O error")
        .expect("hint stream present");
    assert!(hints.is_consistent(), "hint issues: {:?}", hints.issues);

    // 表按 Table F.2 键 `B` 定位：起点即页偏移表的末字节（本表不占流偏移 0）。
    let start = page_offset_bytes() as u64;
    assert_eq!(hints.position_of("B"), Some(start));
    let table = hints.embedded_file.as_ref().expect("encoded table");
    assert_eq!(table.start_byte, start);
    assert_eq!(table.header, input.header, "Table F.11 七项一项不许丢");
    assert_eq!(table.groups, input.groups, "Table F.12 五列逐项归位");
    assert_eq!(table.bits_consumed, input.total_bits());
    assert_eq!(table.end_byte(), start + TABLE_BYTES as u64);
    assert_eq!(hints.embedded_file_end_byte(), Some(table.end_byte()));
    assert!(table.groups_match_header(), "条目数恒等于头部项 3");

    // **跨表硬依赖的两端对齐**：写端把项 5 的宽度当显式入参（35847 的「See Table
    // F.3, item 11」），读端从在场的页偏移表取同一个值，两侧必须相等。
    let document_bits = hints
        .page_offset
        .as_ref()
        .expect("page offset table at stream offset 0")
        .header
        .shared_object_id_bits;
    assert_eq!(
        (document_bits, table.shared_object_id_bits),
        (DOCUMENT_ID_BITS, input.shared_object_id_bits),
        "项 5 的列宽只有文档级这一个来源，写端不得另立"
    );

    // 读端既有语义不被写端违反：三条核对一律「无问题」。
    assert!(
        table.zero_item_problems().is_empty(),
        "项 2/项 3 互斥 shall"
    );
    assert_eq!(table.first_group_anchor_problem(), None, "35792 的首组锚点");
    assert!(
        table.shared_index_problems(3).is_empty(),
        "项 5 的标识符是共享对象表自 0 起的下标（本表 3 个条目）"
    );
    assert_eq!(
        table.groups,
        vec![
            EmbeddedFileGroupEntry {
                object_number: 301,
                object_count: 2,
                group_length: 600,
                shared_reference_count: 1,
                shared_object_ids: vec![2],
            },
            EmbeddedFileGroupEntry {
                object_number: 320,
                object_count: 0,
                group_length: 0,
                shared_reference_count: 3,
                shared_object_ids: vec![1, 0, 2],
            },
        ],
        "项主序下第一组取各列第一格、第二组取第二格"
    );

    // 相邻两家不受影响：`S` 恰落在本表末字节之后，表互不重叠。
    assert_eq!(hints.position_of("S"), Some(table.end_byte()));
    assert_eq!(hints.stream_bytes, payload.len());
    assert!(hints.page_offset.is_some() && hints.shared_object.is_some());
    assert!(hints.thumbnail.is_none());
    assert!(hints.generic.is_empty() && hints.extended.is_empty());
    // 头部项 2 是文件偏移，仍按 F.4 前言 NOTE 3（35490—35492）修正。
    assert!(table.header.first_object_location > hints.primary.offset);
    assert_eq!(
        hints.adjust_position(table.header.first_object_location),
        table.header.first_object_location + hints.primary.length
    );
    assert!(document.warnings().is_empty());
}

/// 退化声明形态（头部项 5/项 6 都为 0）经全链路往返：每组的项 2/项 3 只能是 0
/// （35837—35844 的合法读法），两组都退化成「只有共享对象引用」，编出的表恒
/// 194 位 → 25 字节，后面的 `S` 起点按**实际字节长**推进。
#[test]
fn writer_embedded_file_zero_width_columns_roundtrip_through_the_parser() {
    let mut zero_header = header(2);
    zero_header.group_object_bits = 0;
    zero_header.group_length_bits = 0;
    let input = EmbeddedFileTableInput {
        header: zero_header,
        groups: vec![group(5, 0, 0, &[0, 1]), group(9, 0, 0, &[2])],
        shared_object_id_bits: DOCUMENT_ID_BITS,
    };
    let encoded = encode_embedded_file_table(&input).expect("zero widths are legal declarations");
    assert_eq!(encoded, hand_assembled(&input));
    assert_eq!(input.bits_per_group(), 11); // 9 + 0 + 0 + 2
    assert_eq!(input.total_bits(), 194);
    assert_eq!(encoded.len(), 25, "只有 194 位");

    let mut payload = page_offset_table_bits(0, 2);
    payload.extend_from_slice(&encoded);
    let shared_at = payload.len();
    payload.extend(shared_object_table_bits());
    let entries = format!(" /B {} /S {}", page_offset_bytes(), shared_at);

    let document = parse_buf(&build(&payload, &entries));
    let hints = document
        .linearization_hints()
        .expect("no I/O error")
        .expect("hint stream present");
    assert!(hints.is_consistent(), "hint issues: {:?}", hints.issues);
    let table = hints.embedded_file.as_ref().expect("zero-width table");
    assert_eq!(table.groups, input.groups);
    assert_eq!(
        table.groups,
        vec![group(5, 0, 0, &[0, 1]), group(9, 0, 0, &[2])],
        "退化列照样逐项归位：项 2/项 3 恒为声明的 0，项 5 的标识符仍切回各组"
    );
    assert!(table.zero_item_problems().is_empty());
    assert_eq!(table.bits_consumed, 194);
    assert_eq!(table.end_byte(), (page_offset_bytes() + 25) as u64);
    assert_eq!(hints.position_of("S"), Some(table.end_byte()));
    assert!(
        table.shared_index_problems(3).is_empty(),
        "退化表照样给出可核对的标识符"
    );
    assert!(hints.shared_object.is_some(), "退化表不挤掉相邻家族");
    assert_eq!(document.page_count().expect("page count"), 2);
}

/// 负例：模型不自洽即在写出任何位之前被类型化拒绝，`Result::Err` 意味着零字节
/// 产出——半成品位流绝不进装配器（错误文本点名 Table F.11/F.12 的项号）。
#[test]
fn writer_rejects_inconsistent_embedded_file_models_with_zero_bytes() {
    // 项 3 与组向量不符：读端按项 3（35824）决定每列读几个值。
    let mut input = model();
    input.header.group_count = 3;
    assert_eq!(
        encode_embedded_file_table(&input),
        Err(EmbeddedFileEncodeError::GroupCountMismatch {
            declared: 3,
            actual: 2
        })
    );
    // 项 4 与项 5 的列表长度不符（35848—35850）。
    let mut input = model();
    input.groups[1].shared_object_ids.push(0);
    assert_eq!(
        encode_embedded_file_table(&input),
        Err(EmbeddedFileEncodeError::ReferenceCountMismatch {
            group: 1,
            declared: 3,
            actual: 4
        })
    );
    // **文档级**项 5 宽度越出 Table F.3 的规范字面范围（35524—35525）。
    let mut input = model();
    input.shared_object_id_bits = 33;
    assert_eq!(
        encode_embedded_file_table(&input),
        Err(EmbeddedFileEncodeError::SharedIdWidthOutOfRange { bits: 33 })
    );
    // 项 2/项 3 的互斥 shall（35837—35844）。
    let mut input = model();
    input.groups[0] = group(301, 0, 600, &[2]);
    let error = encode_embedded_file_table(&input).expect_err("非法零项形态");
    assert!(matches!(
        error,
        EmbeddedFileEncodeError::SharedOnlyGroupViolation { group: 0, .. }
    ));
    assert!(
        error.to_string().contains("Table F.12 group 0"),
        "错误文本须点名表号与组序，实际 {error}"
    );
    // 组值超出声明列宽（读端会截断高位，往返不再逐项相等）：512 装不进 9 位。
    let mut input = model();
    input.groups[0].object_number = 512;
    assert_eq!(
        encode_embedded_file_table(&input),
        Err(EmbeddedFileEncodeError::Bit(
            HintEncodeError::ValueTooWide {
                value: 512,
                bits: 9
            }
        ))
    );
    // 空组向量（键 `B` 的条件是「有内嵌文件流」，35381）。
    let mut input = model();
    input.groups.clear();
    input.header.group_count = 0;
    assert_eq!(
        encode_embedded_file_table(&input),
        Err(EmbeddedFileEncodeError::GroupsMissing)
    );
}
