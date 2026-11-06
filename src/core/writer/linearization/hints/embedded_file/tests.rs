//! F.4.6 内嵌文件流 hint 表写端单元测试的**公共底座**：Table F.11 头部七项的定宽
//! 落位、Table F.12 的**项主序（转置）**五列（35797—35850，本家族最易写错处，故
//! 同时反证「逐组连续」写出的字节与产物不同），以及七类拒绝（文档级与头部声明宽度
//! 越界、组缺失、组数不符、项 4 与项 5 不符、项 2/项 3 互斥 shall、越宽数值、
//! 宽度全 0 的读端计数上限）全部在写出第一位之前给出。
//!
//! 与既有读端的**往返**用例住在 [`roundtrip`] 子模块（同批夹具与手工装配器由本
//! 文件提供），两侧合计把「模型 → 位流 → 读端 `EmbeddedFileHintTable::parse`」的
//! 每一环钉住。

use super::*;
use crate::core::reader::linearization::hints::EmbeddedFileHintTable;
use crate::core::reader::linearization::hints::bits::BitWriter;

/// 与既有读端的往返用例（夹具与手工装配器即本文件的 `model`/`hand_assembled`）。
mod roundtrip;

/// Table F.11 项 1—3（首组首对象号 / 首组首位置 / 组数），与读端家族夹具同值。
const FIRST_OBJECT_NUMBER: u64 = 300;
const FIRST_OBJECT_LOCATION: u64 = 31_000;
/// Table F.11 项 4—7（＝Table F.12 项 1—4 的列宽）。
const OBJECT_NUMBER_BITS: u32 = 9;
const GROUP_OBJECT_BITS: u32 = 3;
const GROUP_LENGTH_BITS: u32 = 11;
const SHARED_REF_COUNT_BITS: u32 = 2;
/// 项 5 的宽度来源：**文档级** Table F.3 项 11（35847 的「See Table F.3, item 11」）。
const DOCUMENT_ID_BITS: u32 = 4;

/// well-formed 两组模型：组 0 是「有对象也有共享引用」的正常形态，组 1 是
/// 项 2 与项 3 同时为 0、项 4 = 3 的**零项组**（35837—35844 的合法读法）。
pub(super) fn model() -> EmbeddedFileTableInput {
    EmbeddedFileTableInput {
        header: header(2),
        groups: vec![group(301, 2, 600, &[2]), group(320, 0, 0, &[1, 0, 2])],
        shared_object_id_bits: DOCUMENT_ID_BITS,
    }
}

/// Table F.11 头部：项 1—3 定宽，项 4—7 用四列声明宽度。
pub(super) fn header(group_count: u64) -> EmbeddedFileHeader {
    EmbeddedFileHeader {
        first_object_number: FIRST_OBJECT_NUMBER,
        first_object_location: FIRST_OBJECT_LOCATION,
        group_count,
        object_number_bits: OBJECT_NUMBER_BITS,
        group_object_bits: GROUP_OBJECT_BITS,
        group_length_bits: GROUP_LENGTH_BITS,
        shared_ref_count_bits: SHARED_REF_COUNT_BITS,
    }
}

/// Table F.12 的一个条目：项 4 与项 5 的列表长度一律自洽（项 4＝列表长度）。
pub(super) fn group(
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

/// Table F.11 头部七项（35820—35832）：项 1—3 各 32 位、项 4—7 各 16 位。
fn write_f11(w: &mut BitWriter, h: &EmbeddedFileHeader) {
    w.unsigned(h.first_object_number, 32); // 项 1：首组第一个对象的对象号
    w.unsigned(h.first_object_location, 32); // 项 2：首组第一个对象的位置
    w.unsigned(h.group_count, 32); // 项 3：本表引用的组数
    w.unsigned(u64::from(h.object_number_bits), 16); // 项 4：F.12 项 1 的列宽
    w.unsigned(u64::from(h.group_object_bits), 16); // 项 5：F.12 项 2 的列宽
    w.unsigned(u64::from(h.group_length_bits), 16); // 项 6：F.12 项 3 的列宽
    w.unsigned(u64::from(h.shared_ref_count_bits), 16); // 项 7：F.12 项 4 的列宽
}

/// 手工按 Table F.11 项序 + Table F.12 **项主序**五列装配，用于逐字节对拍。
pub(super) fn hand_assembled(input: &EmbeddedFileTableInput) -> Vec<u8> {
    let mut w = BitWriter::default();
    let h = &input.header;
    write_f11(&mut w, h);
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
    // 读端测试位写手新建字节再置位，故残位天然为零（等价于写端的补零收尾）。
    w.bytes
}

/// Table F.11 头部 7 项定宽落位：前 20 字节逐项等于 32/32/32/16/16/16/16 的大端
/// 拼装，项序与位宽与读端解析完全一致（35820—35832）。
#[test]
fn header_items_land_in_the_table_f11_fixed_width_order() {
    let input = model();
    let bytes = encode_embedded_file_table(&input).unwrap();
    let mut expected = Vec::new();
    for value in [
        input.header.first_object_number,
        input.header.first_object_location,
        input.header.group_count,
    ] {
        expected.extend_from_slice(&(value as u32).to_be_bytes());
    }
    for bits in [
        input.header.object_number_bits,
        input.header.group_object_bits,
        input.header.group_length_bits,
        input.header.shared_ref_count_bits,
    ] {
        expected.extend_from_slice(&(bits as u16).to_be_bytes());
    }
    assert_eq!(expected.len(), HEADER_BITS as usize / 8, "头部恒 160 位整");
    assert_eq!(&bytes[..20], expected.as_slice(), "头部 7 项逐字节相等");
    assert_eq!(
        &bytes[12..20],
        [0, 9, 0, 3, 0, 11, 0, 2].as_slice(),
        "项 4/5/6/7 = 9/3/11/2，各按 16 位存储（高字节恒零）"
    );
}

/// **项主序（转置）**是 shall（35797—35799）：产物与「先把所有组的项 1 写完……」
/// 的手工装配逐字节相等，且与「逐组连续」的写法**不等**——后者会把五列读歪。
#[test]
fn group_columns_follow_the_f46_item_major_order() {
    let input = model();
    let bytes = encode_embedded_file_table(&input).unwrap();

    // 逐组连续（错误的读法）：组 0 的五项紧跟组 1 的五项。
    let mut group_major = BitWriter::default();
    let h = &input.header;
    write_f11(&mut group_major, h);
    for entry in &input.groups {
        group_major.unsigned(entry.object_number, h.object_number_bits);
        group_major.unsigned(entry.object_count, h.group_object_bits);
        group_major.unsigned(entry.group_length, h.group_length_bits);
        group_major.unsigned(entry.shared_reference_count, h.shared_ref_count_bits);
        for &id in &entry.shared_object_ids {
            group_major.unsigned(id, input.shared_object_id_bits);
        }
    }
    assert_ne!(
        bytes, group_major.bytes,
        "逐组连续不是 F.4.6 的条目序，写出的位流必须不同"
    );
    // 两组的对象号（9 位）在项主序下**相邻**：301 = 0b1_0010_1101、320 = 0b1_0100_0000。
    assert_eq!(bytes[20], 0b1001_0110, "组 0 对象号的前 8 位");
    assert_eq!(
        bytes[21] & 0b1111_1100,
        0b1101_0000,
        "组 0 余 1 位 + 组 1 前 7 位"
    );
    // 若按逐组连续交回读端，还原出的条目必与模型不符（反证列序）。
    let mut skewed = group_major.bytes;
    skewed.resize(29, 0);
    let misread = EmbeddedFileHintTable::parse(&skewed, 0, DOCUMENT_ID_BITS)
        .expect("同一批位无论怎么排都读得满 226 位");
    assert_ne!(misread.groups, input.groups, "逐组连续的字节读不回模型");
}

/// 文档级项 5 宽度越出 Table F.3 的**规范字面**范围（35524—35525），以及头部
/// 项 4—7 越出本库类比的同一范围：一律类型化拒绝、零字节。
#[test]
fn declared_width_above_32_is_rejected_before_any_byte() {
    let mut input = model();
    input.shared_object_id_bits = 33;
    assert_eq!(
        encode_embedded_file_table(&input),
        Err(EmbeddedFileEncodeError::SharedIdWidthOutOfRange { bits: 33 })
    );
    for (item, width) in [
        ("item 4", 33u32),
        ("item 5", 40),
        ("item 6", 64),
        ("item 7", u32::MAX),
    ] {
        let mut input = model();
        match item {
            "item 4" => input.header.object_number_bits = width,
            "item 5" => input.header.group_object_bits = width,
            "item 6" => input.header.group_length_bits = width,
            _ => input.header.shared_ref_count_bits = width,
        }
        assert_eq!(
            encode_embedded_file_table(&input),
            Err(EmbeddedFileEncodeError::DeclaredWidthOutOfRange {
                item,
                bits: u64::from(width)
            }),
            "{item} = {width} 须被拒绝"
        );
    }
    // 上界 32 仍合法（见 the_top_of_every_declared_width_roundtrips）。
    let mut input = model();
    input.shared_object_id_bits = 32;
    assert!(encode_embedded_file_table(&input).is_ok());
}

/// 键 `B` 的存在条件是「有内嵌文件流」（35381）：组向量为空（含项 3 也声明 0）
/// 的自相矛盾模型在写出第一位之前拒绝。
#[test]
fn empty_group_vector_is_rejected_before_any_byte() {
    let mut input = model();
    input.groups.clear();
    input.header.group_count = 0;
    assert_eq!(
        encode_embedded_file_table(&input),
        Err(EmbeddedFileEncodeError::GroupsMissing)
    );
}

/// Table F.11 项 3 与组向量不符：读端按项 3 决定每列读几个值，故一律类型化拒绝
/// 且零字节（含声明 0 组与远超 `usize` 的畸形计数）。
#[test]
fn group_count_mismatch_is_rejected_with_zero_bytes() {
    for declared in [1u64, 3, 0, u64::MAX] {
        let mut input = model();
        input.header.group_count = declared;
        assert_eq!(
            encode_embedded_file_table(&input),
            Err(EmbeddedFileEncodeError::GroupCountMismatch {
                declared,
                actual: 2
            }),
            "项 3 = {declared} 与两组模型不符"
        );
    }
}

/// Table F.12 项 4 与该项 5 的列表长度不符（35848—35850）：读端按项 4 把扁平列
/// 切回各组，故按组序报第一处不符并零字节拒绝。
#[test]
fn reference_count_mismatch_is_rejected_with_zero_bytes() {
    let mut input = model();
    input.groups[0].shared_reference_count = 2;
    assert_eq!(
        encode_embedded_file_table(&input),
        Err(EmbeddedFileEncodeError::ReferenceCountMismatch {
            group: 0,
            declared: 2,
            actual: 1
        })
    );
    // 第二组不符（第一组自洽）⇒ 组序 1。
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
}

/// 项 2/项 3/项 4 的互斥 shall（35837—35844，出处 35791—35794）：四种非法形态
/// 逐条给出被违反的规则文本（与读端 `zero_item_problems` 同源），且零字节产出。
#[test]
fn shared_only_group_shall_violations_are_rejected_with_zero_bytes() {
    let cases: [(u64, u64, &[u64]); 4] = [
        (0, 600, &[2]), // 项 2 为 0 却带着非零的项 3
        (0, 0, &[]),    // 项 2 为 0 却项 4 也为 0
        (2, 0, &[1]),   // 项 3 为 0 却带着非零的项 2
        (0, 5, &[]),    // 两项都不为 0 形态之外的组合
    ];
    for (index, (object_count, group_length, ids)) in cases.into_iter().enumerate() {
        let mut input = model();
        input.groups[0] = group(301, object_count, group_length, ids);
        let error = encode_embedded_file_table(&input).expect_err("非法零项形态须被拒绝");
        let EmbeddedFileEncodeError::SharedOnlyGroupViolation {
            group: violating_group,
            rule,
            object_count: reported_count,
            group_length: reported_length,
            shared_reference_count,
        } = error
        else {
            panic!("用例 {index} 期望互斥 shall 违规，实际 {error:?}");
        };
        assert_eq!(violating_group, 0);
        assert_eq!(reported_count, object_count);
        assert_eq!(reported_length, group_length);
        assert_eq!(shared_reference_count, ids.len() as u64);
        // 判定与读端同源：同一违规条目在两侧给出逐字相同的规则文本。
        let entry = group(301, object_count, group_length, ids);
        assert_eq!(
            entry.zero_item_problems().first(),
            Some(&rule),
            "用例 {index} 先命中的规则"
        );
        assert!(rule.contains("shall"), "规则文本须引原文口径，实际 {rule}");
    }
    // 合法形态（项 2 与项 3 同时为 0、项 4 > 0）不被拒绝。
    let input = model();
    assert!(encode_embedded_file_table(&input).is_ok());
}

/// 给定声明宽度下的数值溢出（读端会截断高位、往返不再相等）一律位级拒绝；
/// `Ok` 分支即「半成品位流」，故 helper 里直接 panic——`Result<Vec<u8>, _>` 的
/// `Err` 就是零字节产出。
fn overflow_width(mutate: impl FnOnce(&mut EmbeddedFileTableInput)) -> (u64, u32) {
    let mut input = model();
    mutate(&mut input);
    match encode_embedded_file_table(&input) {
        Err(EmbeddedFileEncodeError::Bit(HintEncodeError::ValueTooWide { value, bits })) => {
            (value, bits)
        }
        Err(other) => panic!("期望位级越宽拒绝，实际 {other:?}"),
        Ok(bytes) => panic!("越宽数值竟产出 {} 字节", bytes.len()),
    }
}

#[test]
fn value_over_a_declared_width_is_rejected_and_yields_no_bytes() {
    assert_eq!(
        overflow_width(|i| i.groups[0].object_number = 512),
        (512, 9),
        "项 1 装不进 9 位"
    );
    assert_eq!(
        overflow_width(|i| i.groups[0].object_count = 8),
        (8, 3),
        "项 2 装不进 3 位"
    );
    assert_eq!(
        overflow_width(|i| i.groups[0].group_length = 2_048),
        (2_048, 11),
        "项 3 装不进 11 位"
    );
    assert_eq!(
        overflow_width(|i| {
            i.groups[1].shared_reference_count = 4;
            i.groups[1].shared_object_ids = vec![0, 1, 2, 3];
        }),
        (4, 2),
        "项 4 装不进 2 位"
    );
    assert_eq!(
        overflow_width(|i| i.groups[1].shared_object_ids[0] = 16),
        (16, 4),
        "项 5 装不进文档级 4 位"
    );
    assert_eq!(
        overflow_width(|i| i.header.first_object_number = 1u64 << 32),
        (1u64 << 32, 32),
        "头部项 1—3 的 32 位定宽同样挡住越界对象号"
    );
}

/// 位宽全 0 时位预算给不出计数证据，读端设 65 536 的绝对上限并拒绝（库策略）：
/// 组数与标识符总数两处同口径。
///
/// 组数一处是**防御性**门：`bits_per_group() == 0` 要求头部项 4—7 全为 0，而那时
/// 每组的项 4 也只能是 0，于是 35837—35844 的互斥 shall 必然先被触发（上限之下
/// 同样拒绝，只是错误形态不同）——故本门只保证「越界的畸形计数绝不会被写成
/// 半张表」，不对偶读端任何可读形态。
#[test]
fn unevidenced_counts_beyond_the_readers_cap_are_rejected() {
    // 组数：项 4—7 全 0 ⇒ 每组一个位也不占。
    for count in [MAX_UNEVIDENCED_ITEMS + 1, MAX_UNEVIDENCED_ITEMS + 2] {
        let mut input = model();
        input.header.object_number_bits = 0;
        input.header.group_object_bits = 0;
        input.header.group_length_bits = 0;
        input.header.shared_ref_count_bits = 0;
        input.header.group_count = count;
        input.shared_object_id_bits = 0;
        input.groups = vec![group(0, 0, 0, &[]); count as usize];
        assert_eq!(
            encode_embedded_file_table(&input),
            Err(EmbeddedFileEncodeError::UnevidencedGroupsTooLarge {
                count,
                limit: MAX_UNEVIDENCED_ITEMS
            }),
            "宽度全 0 的 {count} 组须被拒绝"
        );
    }
    // 上限之内同样不产出，但命中的是互斥 shall（零宽组无从自洽，见上）。
    let mut input = model();
    for bits in [
        &mut input.header.object_number_bits,
        &mut input.header.group_object_bits,
        &mut input.header.group_length_bits,
        &mut input.header.shared_ref_count_bits,
    ] {
        *bits = 0;
    }
    input.shared_object_id_bits = 0;
    input.header.group_count = MAX_UNEVIDENCED_ITEMS;
    input.groups = vec![group(0, 0, 0, &[]); MAX_UNEVIDENCED_ITEMS as usize];
    let error = encode_embedded_file_table(&input).expect_err("零宽组在上限之内同样不产出");
    let EmbeddedFileEncodeError::SharedOnlyGroupViolation {
        group: violating,
        rule,
        ..
    } = error
    else {
        panic!("零宽组须命中互斥 shall，实际 {error:?}");
    };
    assert_eq!(violating, 0);
    // 规则文本与读端判定同源（照抄原文容易把续行写歪，故直接取读端的常量）。
    assert_eq!(
        rule,
        group(0, 0, 0, &[])
            .zero_item_problems()
            .first()
            .copied()
            .expect("零宽组必违反项 2 的 shall"),
        "上限之下的零宽组由互斥 shall 挡住"
    );

    // 标识符总数：文档级宽度 0 ⇒ 每个标识符一个位也不占。
    let mut input = model();
    input.shared_object_id_bits = 0;
    let overflow = MAX_UNEVIDENCED_ITEMS as usize + 1;
    input.groups = vec![group(301, 2, 600, &vec![0u64; overflow])];
    input.header.group_count = 1;
    assert_eq!(
        encode_embedded_file_table(&input),
        Err(EmbeddedFileEncodeError::UnevidencedIdentifiersTooLarge {
            count: MAX_UNEVIDENCED_ITEMS + 1,
            limit: MAX_UNEVIDENCED_ITEMS
        })
    );
    // 上限内通过：65 536 个 0 位标识符读端照样切回该组（项 4 需 32 位宽度才装得下）。
    let mut input = model();
    input.shared_object_id_bits = 0;
    input.header.shared_ref_count_bits = 32;
    input.groups = vec![group(
        301,
        2,
        600,
        &vec![0u64; MAX_UNEVIDENCED_ITEMS as usize],
    )];
    input.header.group_count = 1;
    let bytes = encode_embedded_file_table(&input).unwrap();
    assert_eq!(bytes.len(), 27, "160 + (9+3+11+32) = 215 位 → 补齐 27 字节");
    let parsed = EmbeddedFileHintTable::parse(&bytes, 0, 0).unwrap();
    assert_eq!(
        parsed.groups[0].shared_object_ids.len(),
        MAX_UNEVIDENCED_ITEMS as usize
    );
    assert_eq!(
        parsed.groups[0].shared_reference_count,
        MAX_UNEVIDENCED_ITEMS
    );
    assert_eq!(parsed.bits_consumed, 215);
}
