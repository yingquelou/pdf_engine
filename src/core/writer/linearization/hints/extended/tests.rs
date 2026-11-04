//! F.4.5 extended generic hint 表写端的单元测试：Table F.10 七项的定宽落位、
//! 项 7 按**组内项 6** 的位宽连续打包、与 Table F.9 前 16 字节的同形 shall、
//! 变长表长的补零，以及四类拒绝（名册键、项 6 越出 0—32、项 5 与向量不自洽、
//! 宽度 0 的列长上限）全部在写出第一位之前给出。对拍一律交回真实读端
//! `ExtendedHintTable::parse`。

use super::*;
use crate::core::reader::linearization::hints::{ExtendedHintTable, GenericHintTable};
use crate::core::writer::linearization::hints::generic::encode_generic_table;

/// well-formed 单组模型：项 1—4 用与集成夹具同值的绝对量，项 5 = 3、项 6 = 4 位、
/// 项 7 的标识符落在 4 位内（`15` 是 4 位上界，用于钉住打包方向）。
fn model() -> ExtendedHintTableInput {
    ExtendedHintTableInput {
        key: "V",
        first_object_number: 200,
        first_object_location: 21_000,
        object_count: 3,
        group_length: 700,
        shared_reference_count: 3,
        shared_object_id_bits: 4,
        shared_object_ids: vec![1, 5, 15],
    }
}

/// 模型对应的读端结果（`start` 起点、项 1—7 逐项、`bits_consumed` = 176 + 列位）。
fn parsed_at(
    start: usize,
    key: &'static str,
    ids: &[u64],
    id_bits: u32,
    count: u64,
) -> ExtendedHintTable {
    let items = model();
    ExtendedHintTable {
        key,
        first_object_number: items.first_object_number,
        first_object_location: items.first_object_location,
        object_count: items.object_count,
        group_length: items.group_length,
        shared_reference_count: count,
        shared_object_id_bits: id_bits,
        shared_object_ids: ids.to_vec(),
        start_byte: start as u64,
        bits_consumed: HEADER_BITS + u64::from(id_bits).saturating_mul(ids.len() as u64),
    }
}

/// 项 1—5 的 32 位与项 6 的 16 位按表号顺序大端拼装（22 字节 = 176 位整）。
fn header_bytes(items: &ExtendedHintTableInput) -> Vec<u8> {
    let mut out = Vec::new();
    for value in [
        items.first_object_number,
        items.first_object_location,
        items.object_count,
        items.group_length,
        items.shared_reference_count,
    ] {
        out.extend_from_slice(&(value as u32).to_be_bytes());
    }
    out.extend_from_slice(
        &u16::try_from(items.shared_object_id_bits)
            .unwrap()
            .to_be_bytes(),
    );
    out
}

/// 编码产物交回真实读端：七个字段加 `key`/`start_byte`/`bits_consumed` **整体**相等。
#[test]
fn encode_extended_table_roundtrips_the_seven_items_of_table_f10() {
    let input = model();
    let bytes = encode_extended_table(&input).unwrap();
    assert_eq!(input.identifier_bits(), 12);
    assert_eq!(input.total_bits(), 188); // 176 位头部 + 3×4 位标识符
    assert_eq!(input.byte_len(), 24);
    assert_eq!(bytes.len(), 24);
    assert_eq!(
        ExtendedHintTable::parse(&bytes, 0, "V").unwrap(),
        parsed_at(0, "V", &[1, 5, 15], 4, 3)
    );
}

/// F.4.5 的 shall（35779—35780，配 Table F.9 的 35802—35805）：extended 表「shall begin with the same entries
/// as in a generic hint table」——前 16 字节必须与同四项的 Table F.9 编码**逐字节**
/// 相等，故读端在 < 1.5 的形态下取到的正是同一组项 1—4。
#[test]
fn first_sixteen_bytes_equal_the_generic_table_encoding() {
    let input = model();
    let bytes = encode_extended_table(&input).unwrap();
    let generic = encode_generic_table(&GenericHintTable {
        key: input.key,
        first_object_number: input.first_object_number,
        first_object_location: input.first_object_location,
        object_count: input.object_count,
        group_length: input.group_length,
        start_byte: 0,
        bits_consumed: 128,
    })
    .unwrap();
    assert_eq!(generic.len(), 16);
    assert_eq!(&bytes[..16], generic.as_slice());
    // 项 5—6 是 extended 独有的三项里的两个定宽项（22 字节处才换列）。
    assert_eq!(&bytes[16..22], &header_bytes(&input)[16..]);
}

/// 头部 6 项定宽落位：前 22 字节逐项等于 32/32/32/32/32/16 的按值大端拼装，
/// 证明项序与位宽与读端解析完全一致（35808—35814）。
#[test]
fn header_items_land_in_the_table_f10_fixed_width_order() {
    let input = model();
    let bytes = encode_extended_table(&input).unwrap();
    let mut expected = header_bytes(&input);
    expected.extend_from_slice(&[0x15, 0xF0]); // 项 7 的 1,5,15 → 4 位一组 + 补零
    assert_eq!(bytes, expected);
    assert_eq!(&bytes[0..4], &200u32.to_be_bytes()); // 项 1
    assert_eq!(&bytes[4..8], &21_000u32.to_be_bytes()); // 项 2
    assert_eq!(&bytes[8..12], &3u32.to_be_bytes()); // 项 3
    assert_eq!(&bytes[12..16], &700u32.to_be_bytes()); // 项 4
    assert_eq!(&bytes[16..20], &3u32.to_be_bytes()); // 项 5
    assert_eq!(&bytes[20..22], &4u16.to_be_bytes()); // 项 6
}

/// 项 7 的列**不因字节边界而对齐**：三个 4 位标识符依次高位在前，末尾补零，
/// 故第 23 字节是 `0001_0101`、第 24 字节是 `1111_0000`。
#[test]
fn identifier_column_is_packed_bit_tight_at_the_group_declared_width() {
    let bytes = encode_extended_table(&model()).unwrap();
    assert_eq!(bytes[22], 0b0001_0101); // 标识符 1 与 5
    assert_eq!(bytes[23], 0b1111_0000); // 标识符 15 + 收尾补零的 4 位
    // 换 5 位宽（上界 31）同样逐位紧密：176 + 15 = 191 位 → 补零到 24 字节。
    let mut wide = model();
    wide.shared_object_id_bits = 5;
    wide.shared_object_ids = vec![1, 31, 0];
    let bytes = encode_extended_table(&wide).unwrap();
    assert_eq!(bytes.len(), 24);
    assert_eq!(bytes[22], 0b0000_1111); // 00001 与 11111 的前 3 位
    assert_eq!(bytes[23], 0b1100_0000); // 11111 的后 2 位 + 00000 + 1 个补零位
    assert_eq!(
        ExtendedHintTable::parse(&bytes, 0, "C").unwrap(),
        parsed_at(0, "C", &[1, 31, 0], 5, 3)
    );
}

/// 项 6 = 0：0 位只能表示值 0，故列不占位、表长恒 176 位（22 字节），读端按项 5
/// 补出全 0 标识符（读端模块头的**待核**，写端同形）。
#[test]
fn zero_width_column_roundtrips_as_zero_identifiers() {
    let input = ExtendedHintTableInput {
        key: "C",
        shared_object_id_bits: 0,
        shared_reference_count: 3,
        shared_object_ids: vec![0, 0, 0],
        ..model()
    };
    let bytes = encode_extended_table(&input).unwrap();
    assert_eq!(input.total_bits(), HEADER_BITS);
    assert_eq!(bytes.len(), 22);
    assert_eq!(bytes, header_bytes(&input));
    assert_eq!(
        ExtendedHintTable::parse(&bytes, 0, "C").unwrap(),
        parsed_at(0, "C", &[0, 0, 0], 0, 3)
    );
}

/// 项 5 = 0：没有共享对象引用即没有项 7 的列，表长恒 176 位。
#[test]
fn zero_reference_count_yields_a_176_bit_table() {
    let input = ExtendedHintTableInput {
        key: "R",
        shared_reference_count: 0,
        shared_object_ids: vec![],
        ..model()
    };
    let bytes = encode_extended_table(&input).unwrap();
    assert_eq!(input.byte_len(), 22);
    assert_eq!(bytes.len(), 22);
    let parsed = ExtendedHintTable::parse(&bytes, 0, "R").unwrap();
    assert_eq!(parsed.shared_reference_count, 0);
    assert_eq!(parsed.shared_object_id_bits, 4);
    assert!(parsed.shared_object_ids.is_empty());
    assert_eq!(parsed.bits_consumed, HEADER_BITS);
    assert_eq!(parsed.end_byte(), 22);
}

/// 32 位上界与 32 位列宽都在声明范围内：项 1—5 取 `u32::MAX`、项 6 取 32、
/// 标识符取 `u32::MAX` 与 0，共 240 位 = 30 字节整。
#[test]
fn the_top_of_every_declared_width_roundtrips() {
    let input = ExtendedHintTableInput {
        key: "V",
        first_object_number: u64::from(u32::MAX),
        first_object_location: u64::from(u32::MAX),
        object_count: u64::from(u32::MAX),
        group_length: u64::from(u32::MAX),
        shared_reference_count: 2,
        shared_object_id_bits: 32,
        shared_object_ids: vec![u64::from(u32::MAX), 0],
    };
    let bytes = encode_extended_table(&input).unwrap();
    assert_eq!(input.total_bits(), 176 + 64);
    assert_eq!(bytes.len(), 30);
    assert_eq!(&bytes[..22], header_bytes(&input).as_slice());
    assert_eq!(&bytes[22..26], &u32::MAX.to_be_bytes());
    assert_eq!(&bytes[26..30], &0u32.to_be_bytes());
    let parsed = ExtendedHintTable::parse(&bytes, 0, "V").unwrap();
    assert_eq!(parsed.shared_object_id_bits, 32);
    assert_eq!(parsed.shared_object_ids, vec![u64::from(u32::MAX), 0]);
    assert_eq!(parsed.bits_consumed, 240);
}

/// 三张表按名册键序首尾相邻：每张从自己的起点解出、`start_byte` 与 `end_byte`
/// 链到下一张的下界，补零字节不吃掉邻居（F.4 前言 35482—35483）。
#[test]
fn three_tables_tile_the_stream_and_parse_back_at_their_offsets() {
    let mut stream = Vec::new();
    let mut starts = Vec::new();
    for (index, key) in EXTENDED_HINT_KEYS.iter().copied().enumerate() {
        let input = ExtendedHintTableInput {
            key,
            first_object_number: 200 + index as u64,
            shared_object_ids: vec![index as u64, 5, 15],
            ..model()
        };
        starts.push(stream.len());
        stream.extend(encode_extended_table(&input).unwrap());
    }
    assert_eq!(starts, vec![0, 24, 48]);
    stream.push(0xAA); // 下一张表的起点仍在
    for (index, key) in EXTENDED_HINT_KEYS.iter().copied().enumerate() {
        let parsed = ExtendedHintTable::parse(&stream, starts[index], key).unwrap();
        assert_eq!(parsed.key, key);
        assert_eq!(parsed.start_byte, starts[index] as u64);
        assert_eq!(parsed.first_object_number, 200 + index as u64);
        assert_eq!(parsed.shared_object_ids, vec![index as u64, 5, 15]);
        assert_eq!(parsed.end_byte(), (starts[index] + 24) as u64);
    }
    assert_eq!(stream[72], 0xAA);
}

/// 尾部残位补零到字节边界（191 位 → 24 字节，末字节只剩 1 个零位）：读端只消费
/// 前面的位，补零不影响往返，且补齐后下一张表仍自字节边界开始。
#[test]
fn residual_bits_are_zero_padded_to_the_byte_boundary() {
    let input = ExtendedHintTableInput {
        key: "C",
        shared_object_id_bits: 5,
        shared_object_ids: vec![1, 31, 0],
        ..model()
    };
    let bytes = encode_extended_table(&input).unwrap();
    assert_eq!(input.total_bits() % 8, 7);
    assert_eq!(bytes.len(), input.byte_len() as usize);
    assert_eq!(bytes.last().unwrap() & 0b0000_0001, 0);
    let mut stream = bytes.clone();
    stream.extend_from_slice(&encode_extended_table(&model()).unwrap());
    assert_eq!(
        ExtendedHintTable::parse(&stream, 24, "V").unwrap(),
        parsed_at(24, "V", &[1, 5, 15], 4, 3)
    );
}

/// F.4.5 名册之外（35781—35785）的键不产出本表形态：`O` 属 F.4.4、`B` 由
/// 35786—35788 划归 F.4.6，一律类型化拒绝、零字节。
#[test]
fn key_outside_the_f45_roster_is_rejected_before_any_byte() {
    for key in ["O", "A", "E", "I", "L", "S", "T", "B"] {
        let input = ExtendedHintTableInput { key, ..model() };
        assert_eq!(
            encode_extended_table(&input),
            Err(ExtendedHintEncodeError::KeyNotInExtendedRoster { key }),
            "键 {key} 不在 V/C/R 名册里"
        );
    }
}

/// 项 6 越出 0—32（35524—35525 的同一措辞类比 35813—35814）即类型化拒绝、
/// 零字节——项 7 的列长直接由它决定，保留原值没有意义。
#[test]
fn declared_width_above_32_is_rejected_before_any_byte() {
    for bits in [33u32, 40, 64, u32::from(u16::MAX)] {
        let input = ExtendedHintTableInput {
            shared_object_id_bits: bits,
            ..model()
        };
        assert_eq!(
            encode_extended_table(&input),
            Err(ExtendedHintEncodeError::IdWidthOutOfRange {
                bits: u64::from(bits)
            }),
            "项 6 声明 {bits} 位须被拒绝"
        );
    }
}

/// 项 5 与标识符向量必须严格等长：读端按项 5 决定项 7 读几个，多写少写都无从
/// 逐项往返，故零字节拒绝（含 32 位计数大得无法成为向量长度的形态）。
#[test]
fn reference_count_mismatch_is_rejected_with_zero_bytes() {
    let input = ExtendedHintTableInput {
        shared_reference_count: 3,
        shared_object_ids: vec![1, 5],
        ..model()
    };
    assert_eq!(
        encode_extended_table(&input),
        Err(ExtendedHintEncodeError::ReferenceCountMismatch {
            declared: 3,
            actual: 2
        })
    );
    let input = ExtendedHintTableInput {
        shared_reference_count: 0,
        ..model()
    };
    assert_eq!(
        encode_extended_table(&input),
        Err(ExtendedHintEncodeError::ReferenceCountMismatch {
            declared: 0,
            actual: 3
        })
    );
    let input = ExtendedHintTableInput {
        shared_reference_count: 1u64 << 32, // 超出 32 位定宽，也永不可能等于向量长度
        ..model()
    };
    assert_eq!(
        encode_extended_table(&input),
        Err(ExtendedHintEncodeError::ReferenceCountMismatch {
            declared: 1u64 << 32,
            actual: 3
        })
    );
}

/// 给定宽度下的数值溢出（读端会截断高位）即类型化拒绝，`Result::Err` 意味着
/// 零字节产出——半成品位流绝不外流。
#[test]
fn value_over_a_declared_width_is_rejected_and_yields_no_bytes() {
    for field in 0..4 {
        let mut input = model();
        match field {
            0 => input.first_object_number = 1u64 << 32,
            1 => input.first_object_location = 1u64 << 32,
            2 => input.object_count = 1u64 << 32,
            _ => input.group_length = 1u64 << 32,
        }
        assert_eq!(
            encode_extended_table(&input),
            Err(ExtendedHintEncodeError::Bit(
                HintEncodeError::ValueTooWide {
                    value: 1u64 << 32,
                    bits: 32
                }
            )),
            "项 {} 超出 32 位定宽须被拒绝",
            field + 1
        );
    }
    // 项 7 的标识符超出项 6 声明的组内宽度（16 装不进 4 位）。
    let input = ExtendedHintTableInput {
        shared_reference_count: 1,
        shared_object_ids: vec![16],
        ..model()
    };
    assert_eq!(
        encode_extended_table(&input),
        Err(ExtendedHintEncodeError::Bit(
            HintEncodeError::ValueTooWide { value: 16, bits: 4 }
        ))
    );
    // 宽度 0 时列只接受值 0。
    let input = ExtendedHintTableInput {
        key: "C",
        shared_object_id_bits: 0,
        shared_reference_count: 1,
        shared_object_ids: vec![1],
        ..model()
    };
    assert_eq!(
        encode_extended_table(&input),
        Err(ExtendedHintEncodeError::Bit(
            HintEncodeError::ValueTooWide { value: 1, bits: 0 }
        ))
    );
}

/// 位预算门的对偶：项 6 = 0 时列不占位、位流给不出引用数的证据，读端对这类计数
/// 设 65 536 绝对上限并拒绝——写端同口径提前拒绝，上限之内照常编码。
#[test]
fn unevidenced_column_beyond_the_readers_cap_is_rejected() {
    for count in [MAX_UNEVIDENCED_ITEMS + 1, MAX_UNEVIDENCED_ITEMS + 2] {
        let input = ExtendedHintTableInput {
            shared_object_id_bits: 0,
            shared_reference_count: count,
            shared_object_ids: vec![0; count as usize],
            ..model()
        };
        assert_eq!(
            encode_extended_table(&input),
            Err(ExtendedHintEncodeError::UnevidencedReferencesTooLarge {
                count,
                limit: MAX_UNEVIDENCED_ITEMS
            }),
            "宽度 0 的 {count} 条列须被拒绝"
        );
    }
    // 上限内通过：表长仍恒 176 位（列一个位也不占）。
    let input = ExtendedHintTableInput {
        shared_object_id_bits: 0,
        shared_reference_count: MAX_UNEVIDENCED_ITEMS,
        shared_object_ids: vec![0; MAX_UNEVIDENCED_ITEMS as usize],
        ..model()
    };
    let bytes = encode_extended_table(&input).unwrap();
    assert_eq!(bytes.len(), 22);
    let parsed = ExtendedHintTable::parse(&bytes, 0, "V").unwrap();
    assert_eq!(
        parsed.shared_object_ids.len(),
        MAX_UNEVIDENCED_ITEMS as usize
    );
    assert_eq!(parsed.bits_consumed, HEADER_BITS);
}
