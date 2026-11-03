//! 附录 F.4 hint 表位流原语（F.4 前言的「高位在前」编码）。
//!
//! ISO 32000-1 F.4 前言规定：hint 表字节流「shall be treated as a bit
//! stream, high-order bit first, which shall then subdivided into fields
//! of arbitrary width without regard to byte boundaries」，且「each hint
//! table shall begin at a byte boundary」。本模块提供在该位流上按任意
//! 宽度读取值的纯函数式原语，全部不可信输入路径返回类型化错误
//! [`HintBitError`]，不 panic。
//!
//! **宽度 0**：F.4.1 注明「All the items in Table F.3 that specify a
//! number of bits needed … have values in the range 0 through 32」，
//! 宽度字段为 0 表示该项占 0 位（值为 0），故 `unsigned(0)`/`signed(0)`
//! 合法且消费 0 位。
//!
//! **带符号形式**：符号位在前、绝对值在后（PDF Reference 1.4 附录 F.2.3
//! “Numeric representations” 的约定；ISO 32000-1 删除了该小节，F.4 各表
//! 实际未引用带符号字段，保留原语并为后续家族预留）。绝对值宽度上限 63
//! 位，使结果落进 `i64`。
//!
//! **链式增量表**（[`BitReader::link_delta_list`]）：元素后跟「本链剩余
//! 元素数」的约定同样出自 PDF Reference 1.4 附录 F.2.2（对象号序列的
//! linked-list 编码，用于 1.4 的 object/first-page hint 表；ISO 32000-1
//! 已用 generic/extended generic 表替换这两个家族，故本原语不被任何
//! ISO 表引用，属**待核**项，仅按字面实现并测试）。

use std::fmt;

/// 位流读取失败的类型化错误（不可信输入永不 panic）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HintBitError {
    /// 剩余位不足：请求 `needed_bits` 位，位于 `position_bits` 处只剩
    /// `remaining_bits` 位。
    ShortBuffer {
        needed_bits: u64,
        remaining_bits: u64,
        position_bits: u64,
    },
    /// 字段宽度超过原语上限（无符号/绝对值最多 64 位）。
    InvalidWidth { bits: u64 },
}

impl fmt::Display for HintBitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ShortBuffer {
                needed_bits,
                remaining_bits,
                position_bits,
            } => write!(
                f,
                "F.4 bit stream truncated: {needed_bits} bit(s) needed at bit {position_bits}, only {remaining_bits} remain"
            ),
            Self::InvalidWidth { bits } => write!(
                f,
                "F.4 bit stream: field width {bits} exceeds the 64-bit primitive limit"
            ),
        }
    }
}

impl std::error::Error for HintBitError {}

/// 在字节切片上按「高位在前」读取任意宽度字段的位游标。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BitReader<'a> {
    data: &'a [u8],
    cursor: usize,
}

/// 无符号字段允许的最大位宽（`u64`）。
const MAX_UNSIGNED_BITS: u32 = 64;

impl<'a> BitReader<'a> {
    /// 从字节切片的第 0 位开始。
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, cursor: 0 }
    }

    /// 当前位游标（自流起点的位数）。
    pub fn position_bits(&self) -> u64 {
        self.cursor as u64
    }

    /// 剩余可读位数。
    pub fn remaining_bits(&self) -> u64 {
        (self.data.len() * 8).saturating_sub(self.cursor) as u64
    }

    /// 是否落在字节边界（hint 表须以字节边界开始，F.4 前言）。
    pub fn is_byte_aligned(&self) -> bool {
        self.cursor.is_multiple_of(8)
    }

    /// 读取 `bits`（0..=64）位无符号整数，高位在前；宽度 0 返回 0 且不消费位。
    pub fn unsigned(&mut self, bits: u32) -> Result<u64, HintBitError> {
        if bits == 0 {
            return Ok(0);
        }
        if bits > MAX_UNSIGNED_BITS {
            return Err(HintBitError::InvalidWidth { bits: bits as u64 });
        }
        self.check_room(bits as u64)?;
        let mut value: u64 = 0;
        for _ in 0..bits {
            let byte = self.data[self.cursor / 8];
            let bit = (byte >> (7 - self.cursor % 8)) & 1;
            value = (value << 1) | bit as u64;
            self.cursor += 1;
        }
        Ok(value)
    }

    /// 读取 `bits`（0..=64）位带符号整数：1 位符号 + `bits-1` 位绝对值。
    ///
    /// 语义按 PDF Reference 1.4 F.2.3：符号位为 1 表示负值，数值部分按
    /// 绝对值编码；宽度 1（只有符号位）返回 0。
    pub fn signed(&mut self, bits: u32) -> Result<i64, HintBitError> {
        if bits == 0 {
            return Ok(0);
        }
        if bits > MAX_UNSIGNED_BITS {
            return Err(HintBitError::InvalidWidth { bits: bits as u64 });
        }
        let sign = self.unsigned(1)?;
        let magnitude = self.unsigned(bits - 1)?;
        Ok(if sign == 1 {
            -(magnitude as i64)
        } else {
            magnitude as i64
        })
    }

    /// 读取 1 位布尔标志（Table F.6 项 2 形态）。
    pub fn flag(&mut self) -> Result<bool, HintBitError> {
        Ok(self.unsigned(1)? != 0)
    }

    /// 读取 `count` 字节（每字节按 8 位高位在前拼装；Table F.6 项 3 的
    /// 128 位 MD5 签名用 `count = 16`）。
    pub fn bytes(&mut self, count: usize) -> Result<Vec<u8>, HintBitError> {
        self.check_room(count as u64 * 8)?;
        let mut out = Vec::with_capacity(count);
        for _ in 0..count {
            out.push(self.unsigned(8)? as u8);
        }
        Ok(out)
    }

    /// 链式增量表：循环读取 32 位「本链剩余元素数」，为 0 终止，否则读取
    /// 该多个 `value_bits` 位带符号增量。见模块头的**待核**说明。
    ///
    /// 分配前按剩余位预算校验元素数，防止不可信计数撑爆内存。
    pub fn link_delta_list(&mut self, value_bits: u32) -> Result<Vec<i64>, HintBitError> {
        let mut values = Vec::new();
        loop {
            let remaining = self.unsigned(32)?;
            if remaining == 0 {
                return Ok(values);
            }
            let cost = remaining.saturating_mul(value_bits as u64);
            if cost > self.remaining_bits() {
                return Err(self.short(cost));
            }
            for _ in 0..remaining {
                values.push(self.signed(value_bits)?);
            }
        }
    }

    fn check_room(&self, bits: u64) -> Result<(), HintBitError> {
        if bits > self.remaining_bits() {
            return Err(self.short(bits));
        }
        Ok(())
    }

    fn short(&self, needed_bits: u64) -> HintBitError {
        HintBitError::ShortBuffer {
            needed_bits,
            remaining_bits: self.remaining_bits(),
            position_bits: self.position_bits(),
        }
    }
}

/// 测试装配用的位写手：把「已知位图案」按高位在前打包成字节串。
///
/// 仅 `cfg(test)`：写端线性化不在本切片范围，公开 API 不暴露写手。
#[cfg(test)]
#[derive(Debug, Default)]
pub(crate) struct BitWriter {
    pub(crate) bytes: Vec<u8>,
    bits_in_current: u32,
}

#[cfg(test)]
impl BitWriter {
    /// 追加一个 `bits`（0..=64）位无符号值；超出宽度的高位被截断。
    pub(crate) fn unsigned(&mut self, value: u64, bits: u32) {
        for shift in (0..bits).rev() {
            let bit = (value >> shift) & 1;
            if self.bits_in_current == 0 {
                self.bytes.push(0);
            }
            let last = self.bytes.len() - 1;
            self.bytes[last] |= (bit as u8) << (7 - self.bits_in_current);
            self.bits_in_current += 1;
            if self.bits_in_current == 8 {
                self.bits_in_current = 0;
            }
        }
    }

    /// 追加带符号值：符号位 + `bits-1` 位绝对值。
    pub(crate) fn signed(&mut self, value: i64, bits: u32) {
        self.unsigned(if value < 0 { 1 } else { 0 }, 1);
        self.unsigned(value.unsigned_abs(), bits - 1);
    }

    /// 把每个字节的最高位写成 `true`，其余字节写 `false` 之外的位不变
    /// （测试专用：仅用于写入 1 位标志序列）。
    pub(crate) fn flag(&mut self, value: bool) {
        self.unsigned(value as u64, 1);
    }
}
