//! F.4.5 extended generic hint 表**写端**的集成验证：用 `encode_extended_table`
//! 编码 Table F.10 的七项（项 1—5 各 32 位、项 6 的 16 位组内位宽、项 7 起按该
//! 位宽逐位紧密排列的标识符列），与手工位装配**逐字节相等**；再把三张表（Table
//! F.2 键 `V`/`C`/`R`）注入 [`super`] 的真线性化装配器，从公开 API
//! （`Document::linearization_hints`）解码回来，逐项断言等于编码模型，并核对
//! 表与表之间字节相邻、`/S` 的位置恰落在最后一张表的末字节。
//!
//! 另钉住两条只有整篇文档才看得见的行为：F.4.5「shall begin with the same
//! entries as in a generic hint table」（35779—35780）在 < 1.5 的文档里让同一串
//! 字节被读端按 Table F.9 的四项消化（写端不判版本，见模块头），以及项 6 与文档级
//! Table F.3 项 11 的**派生**一致性核对——写端不做跨表拒绝，越界由读端给诊断。
//!
//! 本文件是 `tests/main/linearization_hints` 的子模块（同目录即可，无须改
//! `tests/main/mod.rs`），复用父模块的位写手与两轮收敛的布局迭代。

use super::{
    BitWriter, build, build_at_version, page_offset_bytes, page_offset_table_bits,
    shared_object_table_bits,
};
use crate::support::parse_buf;
use pdf_engine::core::reader::linearization::LinearizationIssue;
use pdf_engine::core::types::PdfVersion;
use pdf_engine::core::writer::linearization::HintEncodeError;
use pdf_engine::core::writer::linearization::hints::extended::{
    ExtendedHintEncodeError, ExtendedHintTableInput, encode_extended_table,
};

/// F.4.5 的 Table F.2 键（名册 35332—35333 的 `V`、35336—35337 的 `C`、
/// 35379—35380 的 `R`，按名册序），`seed` 让各项在断言里逐项归位。
const EXTENDED_KEYS: [(&str, u64); 3] = [("V", 0), ("C", 1), ("R", 2)];

/// 项 6 的组内位宽与项 5 的引用数（与 `support` 的页偏移表项 11 = 4 位同宽，
/// 故合规形态不触发跨表一致性诊断）。
const ID_BITS: u32 = 4;
const REFERENCES: u64 = 3;

/// 合规形态的表长：176 位定宽头部 + 3 × 4 位标识符 = 188 位 → 24 字节整。
const TABLE_BYTES: usize = 24;

/// 项 1—4 的期望值（组首对象号 / 组首位置 / 组内对象数 / 组长度）。
fn expected_items(seed: u64) -> (u64, u64, u64, u64) {
    (200 + seed, 21_000 + seed, 3 + seed, 700 + seed)
}

/// 项 7 起的标识符列（共享对象 hint 表自 0 起的顺序下标，故一律落在 `ID_BITS` 内）。
fn expected_ids(seed: u64) -> Vec<u64> {
    vec![seed, seed + 1, 15]
}

/// 编码模型：`id_bits` 驱动项 6（跨表一致性用例要把它推得比文档级值大）。
fn model(key: &'static str, seed: u64, id_bits: u32) -> ExtendedHintTableInput {
    let (number, location, objects, length) = expected_items(seed);
    ExtendedHintTableInput {
        key,
        first_object_number: number,
        first_object_location: location,
        object_count: objects,
        group_length: length,
        shared_reference_count: REFERENCES,
        shared_object_id_bits: id_bits,
        shared_object_ids: expected_ids(seed),
    }
}

/// 手工按 Table F.10 项序装配（与读端家族集成夹具同形），用于逐字节对拍。
fn hand_assembled(input: &ExtendedHintTableInput) -> Vec<u8> {
    let mut w = BitWriter::default();
    w.unsigned(input.first_object_number, 32); // 项 1：组内第一个对象的对象号
    w.unsigned(input.first_object_location, 32); // 项 2：组内第一个对象的位置
    w.unsigned(input.object_count, 32); // 项 3：组内对象数
    w.unsigned(input.group_length, 32); // 项 4：对象组的长度（字节）
    w.unsigned(input.shared_reference_count, 32); // 项 5：共享对象引用数
    w.unsigned(u64::from(input.shared_object_id_bits), 16); // 项 6：组内标识符位宽
    for id in &input.shared_object_ids {
        w.unsigned(*id, input.shared_object_id_bits); // 项 7 起：逐个共享对象标识符
    }
    w.finish()
}

/// 表 `index` 的起点：紧跟页偏移表（F.4 前言把它钉在流偏移 0），三张表按名册键序
/// 字节相邻，故后一张的起点即前一张补齐后的末字节。
fn slot(index: usize) -> usize {
    page_offset_bytes() + index * TABLE_BYTES
}

/// 三张编码表 + 页偏移表 + 共享对象表，四段字节相邻的载荷与 Table F.2 键位条目。
fn assembled(id_bits: u32) -> (Vec<u8>, String) {
    let mut payload = page_offset_table_bits(0, 2);
    let mut entries = String::new();
    for (index, (key, seed)) in EXTENDED_KEYS.iter().enumerate() {
        let encoded = encode_extended_table(&model(key, *seed, id_bits)).expect("conforming model");
        assert_eq!(encoded.len(), TABLE_BYTES, "键 {key} 的表长");
        assert_eq!(
            payload.len(),
            slot(index),
            "键 {key} 的起点须是上一张表的末字节"
        );
        payload.extend_from_slice(&encoded);
        entries.push_str(&format!(" /{} {}", key, slot(index)));
    }
    entries.push_str(&format!(" /S {}", payload.len()));
    payload.extend(shared_object_table_bits());
    (payload, entries)
}

/// 写端编码与手工位装配逐字节相等（Table F.10 项序与打包宽度的终极对拍），且经
/// 真线性化文件全链路解码后七个字段连同游标都等于编码模型。
#[test]
fn writer_extended_tables_match_handassembly_and_roundtrip_every_item() {
    for (key, seed) in EXTENDED_KEYS {
        let input = model(key, seed, ID_BITS);
        let encoded = encode_extended_table(&input).expect("a conforming model must encode");
        assert_eq!(encoded, hand_assembled(&input), "键 {key} 的位装配对拍");
        assert_eq!(input.total_bits(), 188, "176 位头部 + 3×4 位列");
        assert_eq!(input.byte_len(), TABLE_BYTES as u64);
        assert_eq!(encoded.len(), TABLE_BYTES, "本家族的表长由表内声明算出");
    }

    let (payload, entries) = assembled(ID_BITS);
    let document = parse_buf(&build(&payload, &entries));
    assert_eq!(document.page_count().expect("page count"), 2);
    let hints = document
        .linearization_hints()
        .expect("no I/O error")
        .expect("hint stream present");
    assert!(hints.is_consistent(), "hint issues: {:?}", hints.issues);
    assert_eq!(hints.extended.len(), 3, "三组按名册键序在场");

    for (index, (key, seed)) in EXTENDED_KEYS.iter().enumerate() {
        let input = model(key, *seed, ID_BITS);
        let start = slot(index) as u64;
        assert_eq!(hints.position_of(key), Some(start), "键 {key} 的取位");
        let table = hints
            .extended_group_of(key)
            .unwrap_or_else(|| panic!("key {key} shall yield a typed group"));
        let (number, location, objects, length) = expected_items(*seed);
        assert_eq!(table.key, *key);
        assert_eq!(table.start_byte, start);
        assert_eq!(table.first_object_number, number, "项 1 组首对象号");
        assert_eq!(table.first_object_location, location, "项 2 组首位置");
        assert_eq!(table.object_count, objects, "项 3 组内对象数");
        assert_eq!(table.group_length, length, "项 4 组长度（字节）");
        assert_eq!(table.shared_reference_count, REFERENCES, "项 5 引用数");
        assert_eq!(table.shared_object_id_bits, ID_BITS, "项 6 组内位宽");
        assert_eq!(
            table.shared_object_ids,
            expected_ids(*seed),
            "项 7 起逐项归位"
        );
        assert_eq!(table.bits_consumed, input.total_bits());
        assert_eq!(table.end_byte(), start + TABLE_BYTES as u64);
        assert_eq!(hints.extended_end_byte(key), Some(table.end_byte()));
        assert_eq!(table.object_range(), Some((number, number + objects - 1)));
    }

    // 最后一张 extended 表的末字节恰是 `S` 的合法起点：各家互不重叠。
    assert_eq!(
        hints.position_of("S"),
        Some(slot(EXTENDED_KEYS.len()) as u64)
    );
    assert_eq!(hints.stream_bytes, payload.len());
    assert!(hints.page_offset.is_some());
    assert!(hints.shared_object.is_some());
    assert!(hints.thumbnail.is_none());
    assert!(
        hints.generic.is_empty(),
        "≥ 1.5 时三键不再走 Table F.9 形态"
    );
    // 项 2 是文件偏移，仍按 F.4 前言 NOTE 3 修正。
    let table = hints
        .extended_group_of("R")
        .expect("renditions name tree group");
    assert!(table.first_object_location > hints.primary.offset);
    assert_eq!(
        hints.adjust_position(table.first_object_location),
        table.first_object_location + hints.primary.length
    );
    assert!(document.warnings().is_empty());
}

/// 跨表一致性属**调用方**：项 6 超过文档级 Table F.3 项 11（页偏移表声明 4 位）时
/// 写端照样产出（本入口只看单张表），读端给**派生**诊断且表照常解码。
#[test]
fn writer_extended_table_wider_than_the_document_is_flagged_by_the_reader() {
    let (payload, entries) = assembled(ID_BITS);
    let document = parse_buf(&build(&payload, &entries));
    let hints = document
        .linearization_hints()
        .expect("no I/O error")
        .expect("hint stream present");
    assert!(
        !hints.issues.iter().any(|issue| matches!(
            issue,
            LinearizationIssue::ExtendedHintIdWidthExceedsDocument { .. }
        )),
        "组内 4 位 = 文档 4 位不构成矛盾：{:?}",
        hints.issues
    );

    // 把三张表的项 6 推到 5 位（标识符仍在界内），文档级值不动。
    let (payload, entries) = assembled(ID_BITS + 1);
    let document = parse_buf(&build(&payload, &entries));
    let hints = document
        .linearization_hints()
        .expect("no I/O error")
        .expect("hint stream present");
    let flagged = hints
        .issues
        .iter()
        .filter_map(|issue| match issue {
            LinearizationIssue::ExtendedHintIdWidthExceedsDocument {
                key,
                group_bits,
                document_bits,
            } => Some((key.as_str(), *group_bits, *document_bits)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        flagged,
        vec![("V", 5, 4), ("C", 5, 4), ("R", 5, 4)],
        "逐组一条诊断，携带键名与两个声明值"
    );
    for (key, seed) in EXTENDED_KEYS {
        let table = hints
            .extended_group_of(key)
            .unwrap_or_else(|| panic!("键 {key} 的表不该被派生核对丢弃"));
        assert_eq!(
            table.shared_object_id_bits,
            ID_BITS + 1,
            "项 6 的原值一律保留"
        );
        assert_eq!(
            table.shared_object_ids,
            expected_ids(seed),
            "5 位列照样逐项解出"
        );
    }
    assert!(
        hints.page_offset.is_some() && hints.shared_object.is_some(),
        "派生核对不牵连其余家族"
    );
}

/// F.4.5 的 shall（35779—35780，配 Table F.9 的 35802—35805）在整篇文档层面的可见后果：声明 1.4 时读端把
/// `V`/`C`/`R` 交给 Table F.9 的四项定宽形态，故编码产物**前 16 字节**被原样
/// 解成 generic 组，extended 集合为空（写端不判版本，调用方须声明 ≥ 1.5）。
#[test]
fn writer_extended_tables_fall_back_to_the_generic_shape_below_pdf_1_5() {
    let (payload, entries) = assembled(ID_BITS);
    let document = parse_buf(&build_at_version("1.4", &payload, &entries));
    assert_eq!(document.source_version(), PdfVersion::V1_4);
    let hints = document
        .linearization_hints()
        .expect("no I/O error")
        .expect("hint stream present");
    assert!(
        hints.extended.is_empty(),
        "1.4 不认 extended 形态：{:?}",
        hints.extended
    );
    assert_eq!(hints.generic.len(), 3, "三键都走 Table F.9 形态");
    for (index, (key, seed)) in EXTENDED_KEYS.iter().enumerate() {
        let table = hints
            .generic_group_of(key)
            .unwrap_or_else(|| panic!("key {key} shall yield a generic group"));
        let (number, location, objects, length) = expected_items(*seed);
        assert_eq!(table.first_object_number, number, "键 {key} 的项 1");
        assert_eq!(table.first_object_location, location, "键 {key} 的项 2");
        assert_eq!(table.object_count, objects, "键 {key} 的项 3");
        assert_eq!(table.group_length, length, "键 {key} 的项 4");
        assert_eq!(table.bits_consumed, 128, "generic 形态恒 4×32 位");
        assert_eq!(
            hints.generic_end_byte(key),
            Some((slot(index) + 16) as u64),
            "只吃前 16 字节，项 5—7 留在流里无人认领"
        );
    }
}

/// 负例：模型不自洽即在写出任何位之前被类型化拒绝，`Result::Err` 意味着零字节
/// 产出——半成品位流绝不进装配器。
#[test]
fn writer_rejects_inconsistent_extended_models_with_zero_bytes() {
    // 项 5 与标识符向量不符：读端按项 5 决定项 7 读几个。
    let mut input = model("V", 0, ID_BITS);
    input.shared_object_ids.pop();
    assert_eq!(
        encode_extended_table(&input),
        Err(ExtendedHintEncodeError::ReferenceCountMismatch {
            declared: REFERENCES,
            actual: 2
        })
    );
    // 项 6 越出 0—32（35524—35525 的同一措辞类比 35813—35814）。
    let mut input = model("C", 1, ID_BITS);
    input.shared_object_id_bits = 33;
    assert_eq!(
        encode_extended_table(&input),
        Err(ExtendedHintEncodeError::IdWidthOutOfRange { bits: 33 })
    );
    // 标识符超出组内声明宽度（读端会截断高位，往返不再逐项相等）。
    let mut input = model("R", 2, ID_BITS);
    input.shared_object_ids[2] = 16;
    assert_eq!(
        encode_extended_table(&input),
        Err(ExtendedHintEncodeError::Bit(
            HintEncodeError::ValueTooWide { value: 16, bits: 4 }
        ))
    );
    // `B` 属 F.4.6（35786—35788），不由本编码器产出。
    assert_eq!(
        encode_extended_table(&model("B", 0, ID_BITS)),
        Err(ExtendedHintEncodeError::KeyNotInExtendedRoster { key: "B" })
    );
}

/// 空列（项 5 = 0）与宽度 0 的列（项 6 = 0）都是合法声明形态：写出的表恒 22
/// 字节（176 位定宽头部，列一个位也不占），注入真文件后读端分别给空标识符与全
/// 0 标识符，且**不牵连**相邻的共享对象表——表长由表内声明算出，故后一张表的
/// 起点按实际字节长推进。
#[test]
fn writer_extended_degenerate_columns_roundtrip_through_the_parser() {
    let empty_column = ExtendedHintTableInput {
        shared_reference_count: 0,
        shared_object_ids: vec![],
        ..model("V", 0, ID_BITS)
    };
    let zero_width = ExtendedHintTableInput {
        shared_object_id_bits: 0,
        shared_reference_count: 3,
        shared_object_ids: vec![0, 0, 0],
        ..model("C", 1, 0)
    };
    let normal = model("R", 2, ID_BITS);

    let mut payload = page_offset_table_bits(0, 2);
    let mut entries = String::new();
    let mut starts = Vec::new();
    for input in [&empty_column, &zero_width, &normal] {
        let encoded = encode_extended_table(input).expect("degenerate but conforming");
        assert_eq!(
            encoded.len(),
            if input.key == "R" { TABLE_BYTES } else { 22 }
        );
        starts.push(payload.len());
        payload.extend_from_slice(&encoded);
        let start = *starts.last().expect("just pushed");
        entries.push_str(&format!(" /{} {}", input.key, start));
    }
    entries.push_str(&format!(" /S {}", payload.len()));
    payload.extend(shared_object_table_bits());

    let document = parse_buf(&build(&payload, &entries));
    let hints = document
        .linearization_hints()
        .expect("no I/O error")
        .expect("hint stream present");
    assert!(hints.is_consistent(), "hint issues: {:?}", hints.issues);
    assert_eq!(hints.extended.len(), 3);
    for (index, key) in ["V", "C", "R"].into_iter().enumerate() {
        let table = hints
            .extended_group_of(key)
            .unwrap_or_else(|| panic!("键 {key} 须给出类型化组"));
        assert_eq!(table.start_byte, starts[index] as u64);
        // 项 1—6 恒 176 位；列按**实际条数**追加，空列一条也不占位。
        let ids = table.shared_object_ids.len();
        assert_eq!(
            table.bits_consumed,
            176 + u64::from(table.shared_object_id_bits) * ids as u64
        );
        assert_eq!(
            table.end_byte(),
            (starts[index] + if index == 2 { TABLE_BYTES } else { 22 }) as u64
        );
        assert_eq!(hints.extended_end_byte(key), Some(table.end_byte()));
    }
    assert!(
        hints
            .extended_group_of("V")
            .unwrap()
            .shared_object_ids
            .is_empty()
    );
    assert_eq!(
        hints.extended_group_of("C").unwrap().shared_object_ids,
        vec![0, 0, 0],
        "宽度 0 的列只能表示值 0"
    );
    assert_eq!(
        hints.extended_group_of("R").unwrap().shared_object_ids,
        expected_ids(2)
    );
    assert!(hints.page_offset.is_some() && hints.shared_object.is_some());
    assert_eq!(document.page_count().expect("page count"), 2);
}
