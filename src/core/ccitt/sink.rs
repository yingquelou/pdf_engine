//! 逐行打包与 PDF 行契约（不引用第三方 crate：`hayro-ccitt` 的 trait 适配在
//! `backend`，本文件只接收中性的逐像素/逐块调用）。
//!
//! 契约：输入为逐像素（是否白）或逐块（是否白 + 字节数）的推进调用；输出为
//! `ceil(Columns/8)` 字节每行的位图，含 `/BlackIs1` 反转、行末不足一字节的白色
//! 补齐、`/Rows` 不足时的白行补齐，以及输出规模护栏。

use super::decoder::MAX_OUTPUT_BYTES;

/// 逐行打包的行汇（每行 `ceil(Columns/8)` 字节）。
pub(super) struct RowSink {
    out: Vec<u8>,
    row: Vec<u8>,
    bytes_per_row: usize,
    bit_in_row: usize,
    pub(super) rows: usize,
    expected_rows: Option<usize>,
    black_is_1: bool,
    overflowed: bool,
}

impl RowSink {
    pub(super) fn new(bytes_per_row: usize, expected_rows: Option<usize>, black_is_1: bool) -> Self {
        Self {
            out: Vec::new(),
            row: vec![0u8; bytes_per_row],
            bytes_per_row,
            bit_in_row: 0,
            rows: 0,
            expected_rows,
            black_is_1,
            overflowed: false,
        }
    }

    /// 单像素对应的位值：默认 1=白、0=黑；`/BlackIs1` 反转。
    fn bit(&self, white: bool) -> u8 {
        u8::from(if self.black_is_1 { !white } else { white })
    }

    /// 一整行的白色位图（行末填充位同为白色，故直接按字节填满）。
    fn white_row(&self) -> Vec<u8> {
        let value = if self.bit(true) == 1 { 0xFF } else { 0x00 };
        vec![value; self.bytes_per_row]
    }

    /// 已完成某行的打包字节（用于提取损坏恢复的参考行）。
    pub(super) fn row_bytes(&self, index: usize) -> &[u8] {
        let start = index * self.bytes_per_row;
        &self.out[start..start + self.bytes_per_row]
    }

    pub(super) fn finish(mut self) -> Result<Vec<u8>, String> {
        if self.overflowed {
            return Err("CCITTFaxDecode: output exceeds size limit".to_string());
        }
        // 已知高度但码流提前结束：以白行补齐（与常见阅读器的宽容行为一致）。
        if let Some(expected) = self.expected_rows {
            let white = self.white_row();
            while self.rows < expected {
                if self.out.len().saturating_add(self.bytes_per_row) > MAX_OUTPUT_BYTES {
                    return Err("CCITTFaxDecode: output exceeds size limit".to_string());
                }
                self.out.extend_from_slice(&white);
                self.rows += 1;
            }
        }
        if self.rows == 0 {
            return Err("CCITTFaxDecode: no rows decoded".to_string());
        }
        Ok(self.out)
    }

    pub(super) fn push_pixel(&mut self, white: bool) {
        if self.bit_in_row >= self.bytes_per_row * 8 {
            return;
        }
        if self.bit(white) == 1 {
            self.row[self.bit_in_row / 8] |= 1 << (7 - self.bit_in_row % 8);
        }
        self.bit_in_row += 1;
    }

    pub(super) fn push_pixel_chunk(&mut self, white: bool, chunk_count: u32) {
        let start = self.bit_in_row / 8;
        let end = start
            .saturating_add(chunk_count as usize)
            .min(self.bytes_per_row);
        let value = if self.bit(white) == 1 { 0xFF } else { 0x00 };
        self.row[start..end].fill(value);
        self.bit_in_row = end * 8;
    }

    pub(super) fn next_line(&mut self) {
        if self.out.len().saturating_add(self.bytes_per_row) > MAX_OUTPUT_BYTES {
            self.overflowed = true;
            return;
        }
        // 行末不足一字节的位按白色补齐（未使用位不参与图像语义）。
        let rem = self.bit_in_row % 8;
        if rem != 0 && self.bit(true) == 1 {
            let last = self.bytes_per_row - 1;
            self.row[last] |= (1u8 << (8 - rem)) - 1;
        }
        self.out.extend_from_slice(&self.row);
        self.row.fill(0);
        self.bit_in_row = 0;
        self.rows += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::RowSink;

    #[test]
    fn packs_pixels_msb_first() {
        let mut sink = RowSink::new(1, Some(1), false);
        for white in [false, false, false, false, true, true, true, true] {
            sink.push_pixel(white);
        }
        sink.next_line();

        assert_eq!(sink.rows, 1);
        assert_eq!(sink.row_bytes(0), &[0x0F][..]);
    }

    #[test]
    fn black_is_1_inverts_pixels_and_pads_tail_with_white() {
        let mut sink = RowSink::new(2, Some(1), true);
        for white in [
            true, true, true, true, false, false, false, false, false, false, false, false,
        ] {
            sink.push_pixel(white);
        }
        sink.next_line();

        // 白=0、黑=1；末 4 个未使用位按白色（0）补齐。
        assert_eq!(sink.row_bytes(0), &[0x0F, 0xF0][..]);
    }

    #[test]
    fn fills_missing_rows_with_white() {
        let mut sink = RowSink::new(1, Some(3), false);
        for white in [false, false, false, false, true, true, true, true] {
            sink.push_pixel(white);
        }
        sink.next_line();

        assert_eq!(sink.finish().unwrap(), vec![0x0F, 0xFF, 0xFF]);
    }

    #[test]
    fn rejects_output_without_rows_when_height_unknown() {
        // 高度未知时没有任何一行可解出：报错而非返回空位图。
        let sink = RowSink::new(1, None, false);
        assert!(sink.finish().is_err());
    }
}
