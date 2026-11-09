//! MSB 优先的位缓冲。
//!
//! 契约：输入为任意字节串或空缓冲；输出为按位（MSB 优先）可读写、可截断的位流，
//! 并支持把 `[start, start + old_len)` 替换为一段行编码，且在其后补一个规范 EOL、
//! 保持后续位相对字节边界的相位不变。

/// MSB 优先的位缓冲：`bit_len` 为有效位数，末尾不足一字节的位为 0。
#[derive(Debug, Clone)]
pub(super) struct BitBuf {
    bytes: Vec<u8>,
    bit_len: usize,
}

impl BitBuf {
    pub(super) fn new() -> Self {
        Self {
            bytes: Vec::new(),
            bit_len: 0,
        }
    }

    pub(super) fn from_bytes(data: &[u8]) -> Self {
        Self {
            bytes: data.to_vec(),
            bit_len: data.len() * 8,
        }
    }

    pub(super) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub(super) fn bit_len(&self) -> usize {
        self.bit_len
    }

    pub(super) fn bit(&self, index: usize) -> bool {
        self.bytes[index / 8] & (0x80 >> (index % 8)) != 0
    }

    pub(super) fn push_bit(&mut self, value: bool) {
        if self.bit_len.is_multiple_of(8) {
            self.bytes.push(0);
        }
        if value {
            self.bytes[self.bit_len / 8] |= 0x80 >> (self.bit_len % 8);
        }
        self.bit_len += 1;
    }

    pub(super) fn push_code(&mut self, code: u16, len: u8) {
        for shift in (0..len).rev() {
            self.push_bit(code & (1u16 << shift) != 0);
        }
    }

    fn push_zeroes(&mut self, count: usize) {
        for _ in 0..count {
            self.push_bit(false);
        }
    }

    fn extend_from(&mut self, other: &BitBuf) {
        for index in 0..other.bit_len {
            self.push_bit(other.bit(index));
        }
    }

    /// 把有效位截断到 `bit_len`，末尾不足一字节的位清零。
    pub(super) fn truncate(&mut self, bit_len: usize) {
        self.bit_len = bit_len.min(self.bit_len);
        let bytes = self.bit_len.div_ceil(8);
        self.bytes.truncate(bytes);
        if let Some(last) = self.bytes.last_mut() {
            let rem = self.bit_len % 8;
            if rem != 0 {
                *last &= 0xff << (8 - rem);
            }
        }
    }

    fn extend_range(&mut self, source: &BitBuf, start: usize, end: usize) {
        for index in start..end.min(source.bit_len) {
            self.push_bit(source.bit(index));
        }
    }

    /// 用 `row` 替换 `[start, start + old_len)`，并补一个规范 EOL。
    ///
    /// 行编码之后先按 8 位同余补 0（EOL 前的填充位），再写入 `11` 个 0 和
    /// 一个 1：既得到合法 EOL，也不改变后续行相对字节边界的相位。
    pub(super) fn splice(&mut self, start: usize, old_len: usize, row: &BitBuf) {
        let mut next = BitBuf::new();
        next.extend_range(self, 0, start);
        next.extend_from(row);
        let base = row.bit_len + 12;
        let pad = (old_len % 8 + 8 - base % 8) % 8;
        next.push_zeroes(pad);
        next.push_zeroes(11);
        next.push_bit(true);
        next.extend_range(self, start + old_len, self.bit_len);
        *self = next;
    }
}

#[cfg(test)]
mod tests {
    use super::BitBuf;

    #[test]
    fn reads_and_writes_bits_msb_first() {
        let mut buf = BitBuf::new();
        for bit in [true, false, true, true, false, false, true, false] {
            buf.push_bit(bit);
        }
        assert_eq!(buf.bit_len(), 8);
        assert_eq!(buf.bytes(), &[0xB2][..]);

        let loaded = BitBuf::from_bytes(&[0x80]);
        assert!(loaded.bit(0));
        assert!(!loaded.bit(7));
    }

    #[test]
    fn push_code_writes_declared_bit_length() {
        let mut buf = BitBuf::new();
        buf.push_code(0b1011, 4);
        assert_eq!(buf.bit_len(), 4);
        assert_eq!(buf.bytes(), &[0xB0][..]);
    }

    #[test]
    fn truncate_clears_tail_bits() {
        let mut buf = BitBuf::from_bytes(&[0xFF]);
        buf.truncate(5);
        assert_eq!(buf.bit_len(), 5);
        assert_eq!(buf.bytes(), &[0xF8][..]);
    }

    /// 替换后：前缀与后缀原样保留，插入的行之后是一个规范 EOL，且后缀的字节
    /// 相位与替换前一致（`splice` 的全部语义都在这条断言里）。
    #[test]
    fn splice_keeps_suffix_phase_and_inserts_canonical_eol() {
        // "0011" + 被替换的 8 位 "11110000" + 后缀 "1010"
        let mut stream = BitBuf::new();
        for bit in [
            false, false, true, true, true, true, true, true, false, false, false, false, true,
            false, true, false,
        ] {
            stream.push_bit(bit);
        }
        let mut row = BitBuf::new();
        for bit in [false, true, true] {
            row.push_bit(bit);
        }

        stream.splice(4, 8, &row);

        // "0011" + "011" + 1 位相位填充 + 11 个 0 + 1（EOL）+ "1010"
        assert_eq!(stream.bit_len(), 24);
        assert_eq!(stream.bytes(), &[0x36, 0x00, 0x1A][..]);
    }
}
