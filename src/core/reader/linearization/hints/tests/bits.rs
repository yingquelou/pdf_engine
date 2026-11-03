//! [`BitReader`] 的性质化用例：已知位图案 → 已知值，宽度 1..=64 全覆盖，
//! 截断一律类型化错误、不 panic。

use super::super::bits::{BitReader, HintBitError};
use crate::core::reader::linearization::hints::bits::BitWriter;

/// 逐宽度往返：随机感图案经 BitWriter 装配后按同宽度读回。
#[test]
fn unsigned_roundtrips_every_width_from_1_to_64() {
    for bits in 1..=64u32 {
        let value = 0xDEADBEEFCAFEBABEu64 >> (64 - bits);
        let mut writer = BitWriter::default();
        writer.unsigned(value, bits);
        let mut reader = BitReader::new(&writer.bytes);
        assert_eq!(reader.unsigned(bits), Ok(value), "width {bits}");
        // 写手把末字节补零，读者只消费恰好 `bits` 位。
        assert_eq!(reader.position_bits(), bits as u64, "width {bits}");
    }
}

/// 高位在前：0b1011_0000 的前 4 位是 0b1011。
#[test]
fn unsigned_reads_high_order_bits_first() {
    let mut reader = BitReader::new(&[0b1011_0000]);
    assert_eq!(reader.unsigned(1), Ok(1));
    assert_eq!(reader.unsigned(3), Ok(0b011));
    assert_eq!(reader.unsigned(4), Ok(0));
}

/// 跨字节边界：宽度 12 从第 4 位起吞掉下一字节的高 8 位。
#[test]
fn unsigned_spans_byte_boundaries() {
    let bytes = [0x00u8, 0xF0, 0x0F];
    let mut reader = BitReader::new(&bytes);
    assert_eq!(reader.unsigned(4), Ok(0));
    // 0000 | 1111 0000 | 0000 开始：接下来 12 位是 0000_1111_0000。
    assert_eq!(reader.unsigned(12), Ok(0x0F0));
}

/// 宽度 0 合法（F.4.1「values in the range 0 through 32」）且不消费位。
#[test]
fn zero_width_returns_zero_without_consuming() {
    let mut reader = BitReader::new(&[0xFF]);
    assert_eq!(reader.unsigned(0), Ok(0));
    assert_eq!(reader.signed(0), Ok(0));
    assert_eq!(reader.position_bits(), 0);
    assert_eq!(reader.unsigned(8), Ok(0xFF));
}

/// 宽度超过 64 是类型化错误，不消费位。
#[test]
fn too_wide_field_is_typed_error() {
    let mut reader = BitReader::new(&[0xFF; 32]);
    assert_eq!(
        reader.unsigned(65),
        Err(HintBitError::InvalidWidth { bits: 65 })
    );
    assert_eq!(
        reader.signed(65),
        Err(HintBitError::InvalidWidth { bits: 65 })
    );
    assert_eq!(reader.position_bits(), 0);
}

/// 带符号形式：符号位在前、绝对值在后（PDF Reference 1.4 F.2.3 约定）。
#[test]
fn signed_is_sign_bit_followed_by_magnitude() {
    // 位图案 1 0101 → 宽度 5 → -5；0 0011 → +3。
    let mut writer = BitWriter::default();
    writer.signed(-5, 5);
    writer.signed(3, 5);
    writer.signed(0, 5);
    writer.signed(-1, 2);
    let mut reader = BitReader::new(&writer.bytes);
    assert_eq!(reader.signed(5), Ok(-5));
    assert_eq!(reader.signed(5), Ok(3));
    assert_eq!(reader.signed(5), Ok(0));
    assert_eq!(reader.signed(2), Ok(-1));
    // 宽度 1 只有符号位：±0 都是 0。
    let mut single = BitReader::new(&[0b1000_0000]);
    assert_eq!(single.signed(1), Ok(0));
}

/// 64 位带符号极值：符号 + 63 位绝对值。
#[test]
fn signed_supports_63_bit_magnitudes() {
    let mut writer = BitWriter::default();
    writer.signed(i64::MIN + 1, 64);
    writer.signed(i64::MAX, 64);
    let mut reader = BitReader::new(&writer.bytes);
    assert_eq!(reader.signed(64), Ok(i64::MIN + 1));
    assert_eq!(reader.signed(64), Ok(i64::MAX));
}

/// 标志位与位打包字节读取（跨字节、非对齐起点）。
#[test]
fn flag_and_bytes_read_from_bit_stream() {
    let mut reader = BitReader::new(&[0b1010_0001, 0b0000_1111]);
    assert_eq!(reader.flag(), Ok(true));
    assert_eq!(reader.flag(), Ok(false));
    assert_eq!(reader.flag(), Ok(true));
    // 从第 3 位起拼 8 位：0000 1 000 → 0b0000_1000。
    assert_eq!(reader.bytes(1), Ok(vec![0b0000_1000]));
    assert!(!reader.is_byte_aligned());
    assert_eq!(reader.position_bits(), 11);
}

/// 逐字节前缀截断：每个不足读取需求的缓冲都必须给出类型化错误而非 panic。
#[test]
fn every_truncation_yields_typed_short_buffer_error() {
    let mut writer = BitWriter::default();
    writer.unsigned(0xABCDEF12, 32);
    writer.unsigned(0x34, 16);
    writer.flag(true);
    let bytes = writer.bytes; // 32+16+1 = 49 位 = 7 字节。
    assert_eq!(bytes.len(), 7);
    for prefix in 0..bytes.len() {
        let mut reader = BitReader::new(&bytes[..prefix]);
        let result = reader
            .unsigned(32)
            .and_then(|_| reader.unsigned(16))
            .and_then(|_| reader.flag());
        // 前缀不足 32 位在首读即失败；其后按段推进失败位置。
        let bits = prefix * 8;
        let expected_position = if bits < 32 {
            0
        } else if bits < 48 {
            32
        } else {
            48
        };
        assert!(
            matches!(
                result,
                Err(HintBitError::ShortBuffer {
                    remaining_bits,
                    position_bits,
                    ..
                }) if remaining_bits == (bits - expected_position) as u64
                    && position_bits == expected_position as u64
            ),
            "prefix {prefix} must be a typed error, got {result:?}"
        );
    }
    // 完整 7 字节则恰好读满 49 位（末字节 7 位补零不算截断）。
    let mut reader = BitReader::new(&bytes);
    assert_eq!(reader.unsigned(32), Ok(0xABCDEF12));
    assert_eq!(reader.unsigned(16), Ok(0x34));
    assert_eq!(reader.flag(), Ok(true));
    assert_eq!(reader.position_bits(), 49);
    assert_eq!(reader.remaining_bits(), 56 - 49);
}

/// 链式增量表：32 位剩余数计到 0 终止，元素为带符号增量。
#[test]
fn link_delta_list_terminates_on_zero_remaining() {
    let mut writer = BitWriter::default();
    writer.unsigned(3, 32); // 本链还剩 3 个元素
    writer.signed(4, 8);
    writer.signed(-2, 8);
    writer.signed(1, 8);
    writer.unsigned(1, 32); // 第二条链还剩 1 个元素
    writer.signed(-7, 8);
    writer.unsigned(0, 32); // 终止符
    let mut reader = BitReader::new(&writer.bytes);
    let deltas = reader.link_delta_list(8).expect("well-formed list");
    assert_eq!(deltas, vec![4, -2, 1, -7]);
    // 累加还原（消费方语义）：10,14,12,13,6。
    let mut cursor = 10i64;
    let mut positions = Vec::new();
    for delta in deltas {
        cursor += delta;
        positions.push(cursor);
    }
    assert_eq!(positions, vec![14, 12, 13, 6]);
}

/// 链式增量表对不可信计数：剩余数超过位预算必须先类型化报错，不分配。
#[test]
fn link_delta_list_rejects_bogus_count_before_allocating() {
    let mut writer = BitWriter::default();
    writer.unsigned(u32::MAX as u64, 32);
    let mut reader = BitReader::new(&writer.bytes);
    assert!(matches!(
        reader.link_delta_list(8),
        Err(HintBitError::ShortBuffer {
            needed_bits: _,
            remaining_bits: 0,
            ..
        })
    ));
}

/// 任意字节上任意读法都不 panic（位游标越界防护的全量抽查）。
#[test]
fn reader_never_panics_on_untrusted_bytes() {
    let samples: [&[u8]; 4] = [&[], &[0x80], &[0xFF], &[0x5A, 0xA5, 0x00, 0x00]];
    for data in samples {
        for bits in 0..=64u32 {
            let mut reader = BitReader::new(data);
            let _ = reader.unsigned(bits);
            let _ = reader.signed(bits);
            let _ = reader.flag();
            let _ = reader.bytes(bits as usize);
            let _ = reader.link_delta_list(bits);
        }
    }
}
