//! 附录 F.4 hint 表位流的**写端**原语（与 [`crate::core::reader::linearization::
//! hints::bits`] 的位读取器对偶）。
//!
//! ISO 32000-1 F.4 前言规定 hint 表字节流「shall be treated as a bit stream,
//! high-order bit first, which shall then subdivided into fields of arbitrary
//! width without regard to byte boundaries」，且「each hint table shall begin at
//! a byte boundary」。本模块按同一口径**高位在前**逐位追加定宽字段，并在
//! [`HintBitWriter::finish`] 把末尾不足一个字节的残位**补零**到字节边界——
//! 读端只看前面的位，补零不影响解析，从而保证写出的表恒自字节边界起、可与
//! 读端逐位往返。
//!
//! **宽度口径与读端一致**：无符号字段的位宽最多 64 位，超过即类型化拒绝
//! [`HintEncodeError::WidthTooWide`]（镜像读端
//! [`crate::core::reader::linearization::hints::bits::BitReader::unsigned`] 的
//! `MAX_UNSIGNED_BITS` 门，见该文件 `src/core/reader/linearization/hints/bits.rs:71`）；
//! 宽度 0 合法且只接受值 0（F.4.1「have values in the range 0 through 32」，
//! 宽度 0 表示该项占 0 位、值为 0）。给定宽度下值超出 `2^bits` 即
//! [`HintEncodeError::ValueTooWide`]——读端会把高位截断，若写端不拒绝则往返
//! 不再逐项相等，故写端在这一层就挡住不可逆的编码，保证 round-trip 恒等。
//!
//! **带符号形式**（[`HintBitWriter::signed`]）与读端 `BitReader::signed` 对偶：
//! 1 位符号在前 + `bits-1` 位绝对值，PDF Reference 1.4 F.2.3 约定；ISO 32000-1
//! 的 F.4 各表实际未引用带符号字段，保留以支撑后续表族。

use std::fmt;

/// 无符号字段允许的最大位宽（`u64`）。与读端 `bits.rs` 的同名常量保持同口径。
pub const MAX_UNSIGNED_BITS: u32 = 64;

/// 写端编码失败的类型化错误：所有拒绝都在写出任何不可逆位之前给出，绝不 panic。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HintEncodeError {
    /// 声明位宽超过 64 位原语上限——读端 [`BitReader`](crate::core::reader::linearization::hints::bits::BitReader)
    /// 会在同一宽度上报 `InvalidWidth`，写端提前拒绝以保证往返可解析。
    WidthTooWide { bits: u64 },
    /// 给定宽度下数值超出 `2^bits`（宽度 0 时只接受 0）——若写出则读端会截断
    /// 高位、往返不再逐项相等，故写端拒绝。
    ValueTooWide { value: u64, bits: u32 },
    /// 签名标志与签名载荷不自洽：标志置位却无 16 字节 MD5，或标志清零却携带
    /// 签名——读端只在标志置位时读取 128 位并据此回填 `Option`，这样的模型无法
    /// 往返相等（Table F.6 项 2/项 3）。
    SignatureFlagMismatch { group: usize },
    /// 表头声明的条目数与实际组向量不自洽：首页条目数与首页组向量长度不符，
    /// 或总条目数与两序列向量长度之和不符（Table F.5 项 3/项 4）。
    EntryCountMismatch {
        declared_first_page: u64,
        actual_first_page: usize,
        declared_total: u64,
        actual_total: usize,
    },
}

impl fmt::Display for HintEncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WidthTooWide { bits } => write!(
                f,
                "F.4 bit stream: field width {bits} exceeds the 64-bit primitive limit"
            ),
            Self::ValueTooWide { value, bits } => write!(
                f,
                "F.4 bit stream: value {value} does not fit the declared {bits}-bit field width \
                 (the reader would truncate the high bits, breaking round-trip equality)"
            ),
            Self::SignatureFlagMismatch { group } => write!(
                f,
                "F.4.2 Table F.6: group {group} has an inconsistent signature flag and payload; \
                 item 2 shall be 1 if and only if item 3's 128-bit MD5 signature is present"
            ),
            Self::EntryCountMismatch {
                declared_first_page,
                actual_first_page,
                declared_total,
                actual_total,
            } => write!(
                f,
                "F.4.2 Table F.5: the header declares item 3 = {declared_first_page} and item 4 = \
                 {declared_total} entry(ies), but the group vectors hold {actual_first_page} first-page \
                 and {actual_total} total entries; the reader reconstructs the sequences from the \
                 header counts, so an inconsistent model cannot round-trip"
            ),
        }
    }
}

impl std::error::Error for HintEncodeError {}

/// 高位在前、按位追加的定宽字段写入器（F.4 前言位流形态的写端）。
#[derive(Debug, Default, Clone)]
pub struct HintBitWriter {
    bytes: Vec<u8>,
    /// 当前尾部残字节已占用的位数（0—7）。
    used: u32,
    /// 当前残字节已写入的位（左对齐）。
    pending: u8,
}

impl HintBitWriter {
    /// 新建空位流。
    pub fn new() -> Self {
        Self::default()
    }

    /// 当前已追加的位数（含未落字的残位）。
    pub fn position_bits(&self) -> u64 {
        (self.bytes.len() as u64) * 8 + self.used as u64
    }

    /// 是否落在字节边界（无残位）。
    pub fn is_byte_aligned(&self) -> bool {
        self.used == 0
    }

    /// 追加一个 `bits`（0..=64）位无符号值，高位在前。
    ///
    /// 宽度超过 [`MAX_UNSIGNED_BITS`] 报 [`HintEncodeError::WidthTooWide`]；给定
    /// 宽度下值溢出报 [`HintEncodeError::ValueTooWide`]（宽度 0 只接受 0）。两者
    /// 都在写入任何位之前给出，残位状态不被污染。
    pub fn unsigned(&mut self, value: u64, bits: u32) -> Result<(), HintEncodeError> {
        if bits > MAX_UNSIGNED_BITS {
            return Err(HintEncodeError::WidthTooWide {
                bits: u64::from(bits),
            });
        }
        // 值必须落进声明宽度：`bits < 64` 时用移位判溢出，`bits == 64` 时任何
        // `u64` 都在宽度内；`bits == 0` 时 `value >= 1` 即溢出（1 << 0 == 1）。
        if bits < MAX_UNSIGNED_BITS && value >= (1u64 << bits) {
            return Err(HintEncodeError::ValueTooWide { value, bits });
        }
        for index in (0..bits).rev() {
            self.push_bit(((value >> index) & 1) as u8);
        }
        Ok(())
    }

    /// 追加一个 `bits`（1..=64）位带符号值：1 位符号 + `bits-1` 位绝对值。
    ///
    /// 与读端 `BitReader::signed` 同口径；`bits == 0` 表示该项不占位、值恒 0。
    pub fn signed(&mut self, value: i64, bits: u32) -> Result<(), HintEncodeError> {
        if bits == 0 {
            if value != 0 {
                return Err(HintEncodeError::ValueTooWide {
                    value: value.unsigned_abs(),
                    bits,
                });
            }
            return Ok(());
        }
        self.unsigned(u64::from(value < 0), 1)?;
        self.unsigned(value.unsigned_abs(), bits - 1)
    }

    /// 追加一个 1 位布尔标志（Table F.6 项 2 形态）。
    pub fn flag(&mut self, value: bool) -> Result<(), HintEncodeError> {
        self.unsigned(u64::from(value), 1)
    }

    /// 追加 `data` 的每个字节（各按 8 位高位在前拼装；Table F.6 项 3 的 128 位
    /// MD5 用 `data.len() == 16`）。
    pub fn bytes(&mut self, data: &[u8]) -> Result<(), HintEncodeError> {
        for &byte in data {
            self.unsigned(u64::from(byte), 8)?;
        }
        Ok(())
    }

    /// 收尾：把不足一字节的尾部残位按零填充到字节边界，返回完整字节串。
    ///
    /// F.4 前言的位流形态下读端只消费前面的位，补零不影响往返解析。调用本方法
    /// 后写入器复位，可继续构造下一张以字节边界开始的表。
    pub fn finish(&mut self) -> Vec<u8> {
        if self.used > 0 {
            self.bytes.push(self.pending << (8 - self.used));
            self.pending = 0;
            self.used = 0;
        }
        std::mem::take(&mut self.bytes)
    }

    /// 内部：把单个高位在前位并入尾部残字节。满 8 位即落字复位，保证写入顺序
    /// 与读端 `BitReader::unsigned` 的逐位还原严格对偶（最先写入的位落在字节
    /// 最高位）。
    fn push_bit(&mut self, bit: u8) {
        self.pending = (self.pending << 1) | (bit & 1);
        self.used += 1;
        if self.used == 8 {
            self.bytes.push(self.pending);
            self.pending = 0;
            self.used = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reader::linearization::hints::bits::BitReader;

    /// 写出的定宽无符号值可被读端逐位还原，且高位在前（首字段的最高位落进首字节）。
    #[test]
    fn unsigned_writes_known_bit_pattern_and_reader_recovers_it() {
        let mut writer = HintBitWriter::new();
        writer.unsigned(0b1011, 4).unwrap();
        writer.unsigned(0xAB, 8).unwrap();
        writer.unsigned(u64::MAX, 64).unwrap();
        let bytes = writer.finish();

        let mut reader = BitReader::new(&bytes);
        assert_eq!(reader.unsigned(4).unwrap(), 0b1011);
        assert_eq!(reader.unsigned(8).unwrap(), 0xAB);
        assert_eq!(reader.unsigned(64).unwrap(), u64::MAX);
        // 读端消费 76 位后只剩收尾补的零位（不足一个字节）。
        assert!(reader.remaining_bits() < 8);
    }

    /// 首个 4 位字段应占据首字节最高位，证明 MSB-first 落位方向与读端一致。
    #[test]
    fn first_written_bits_land_in_msb_of_first_byte() {
        let mut writer = HintBitWriter::new();
        writer.unsigned(0b1010, 4).unwrap();
        let bytes = writer.finish();
        assert_eq!(bytes, vec![0b1010_0000]);
    }

    /// 尾部残位按零补齐到字节边界；读端只消费前面的位，补齐不影响解析。
    #[test]
    fn final_byte_is_zero_padded_and_reader_ignores_the_tail() {
        let mut writer = HintBitWriter::new();
        writer.unsigned(0b101, 3).unwrap();
        assert!(!writer.is_byte_aligned());
        assert_eq!(writer.position_bits(), 3);
        let bytes = writer.finish();
        assert_eq!(bytes.len(), 1);
        assert_eq!(bytes[0], 0b1010_0000);

        let mut reader = BitReader::new(&bytes);
        assert_eq!(reader.unsigned(3).unwrap(), 0b101);
    }

    /// 位宽超过 64 位原语上限即类型化拒绝，且不触碰残位状态（读端同宽度报 InvalidWidth）。
    #[test]
    fn width_above_the_64_bit_limit_is_rejected_without_touching_the_stream() {
        let mut writer = HintBitWriter::new();
        writer.unsigned(7, 3).unwrap();
        let before = writer.position_bits();
        assert_eq!(
            writer.unsigned(1, MAX_UNSIGNED_BITS + 1),
            Err(HintEncodeError::WidthTooWide {
                bits: u64::from(MAX_UNSIGNED_BITS) + 1
            })
        );
        assert_eq!(writer.position_bits(), before);
    }

    /// 给定宽度下数值溢出即拒绝，且绝不写出被截断的高位——往返相等的前提。
    #[test]
    fn value_over_the_declared_width_is_rejected_and_writes_nothing() {
        let mut writer = HintBitWriter::new();
        let before = writer.position_bits();
        // 8 装不进 3 位（2^3 == 8），读端会截断成 0。
        assert_eq!(
            writer.unsigned(8, 3),
            Err(HintEncodeError::ValueTooWide { value: 8, bits: 3 })
        );
        assert_eq!(writer.position_bits(), before);
        // 边界：7 恰好装进 3 位。
        writer.unsigned(7, 3).unwrap();
        assert_eq!(writer.position_bits(), before + 3);
    }

    /// 宽度 0 合法（该项占 0 位、值为 0），值非 0 即拒绝——镜像读端 F.4.1 的 0—32 约定。
    #[test]
    fn width_zero_accepts_only_the_value_zero() {
        let mut writer = HintBitWriter::new();
        writer.unsigned(0, 0).unwrap();
        assert_eq!(writer.position_bits(), 0);
        assert_eq!(
            writer.unsigned(1, 0),
            Err(HintEncodeError::ValueTooWide { value: 1, bits: 0 })
        );
        assert_eq!(writer.position_bits(), 0);
    }

    /// 带符号字段（1 位符号 + 绝对值）与读端 `signed` 对偶往返；0 与非负/负值都成立。
    #[test]
    fn signed_values_roundtrip_against_the_reader_signed_primitive() {
        for value in [0i64, 1, -1, 5, -5, i64::from(i32::MAX), -2_000_000] {
            let mut writer = HintBitWriter::new();
            writer.signed(value, 32).unwrap();
            let bytes = writer.finish();
            let mut reader = BitReader::new(&bytes);
            assert_eq!(reader.signed(32).unwrap(), value);
        }
    }

    /// 1 位标志与 16 字节（128 位）MD5 各自按位追加，读端逐项还原。
    #[test]
    fn flag_and_sixteen_byte_md5_roundtrip_bit_for_bit() {
        let md5: [u8; 16] = std::array::from_fn(|i| i as u8);
        let mut writer = HintBitWriter::new();
        writer.flag(true).unwrap();
        writer.flag(false).unwrap();
        writer.bytes(&md5).unwrap();
        let bytes = writer.finish();
        assert_eq!(bytes.len(), 17); // 2 位标志 + 128 位 MD5 = 130 位 → 补齐成 17 字节

        let mut reader = BitReader::new(&bytes);
        assert!(reader.flag().unwrap());
        assert!(!reader.flag().unwrap());
        let restored = reader.bytes(16).unwrap();
        assert_eq!(restored.as_slice(), md5.as_slice());
    }

    /// 收尾后写入器复位，可继续构造下一张以字节边界开始的表（F.4 前言「每表以字节边界开始」）。
    #[test]
    fn finish_resets_the_writer_for_the_next_table() {
        let mut writer = HintBitWriter::new();
        writer.unsigned(0xFF, 12).unwrap();
        let first = writer.finish();
        assert_eq!(first.len(), 2);
        assert_eq!(writer.position_bits(), 0);
        assert!(writer.is_byte_aligned());
        // 复位后可立即再写一张。
        writer.unsigned(0b1, 1).unwrap();
        assert_eq!(writer.finish(), vec![0b1000_0000]);
    }
}
