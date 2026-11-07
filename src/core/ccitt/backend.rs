//! `hayro-ccitt` 的唯一适配点：把位流与行解码挡在 PDF 语义层之外。
//!
//! 上层只经 [`CcittBackend`](super::decoder::CcittBackend) 交互；`/BlackIs1`
//! 的位反转、行末不足一字节的白色补齐、按 `/Rows` 补白行都在本文件完成。
//!
//! `/DamagedRowsBeforeError` 的恢复不依赖 hayro 的内部状态：出错后本文件
//! 自行按位扫描 EOL、定位损坏行所在的位区间，按上一行的换色位置重新编码
//! （或编码为全白行），替换该区间与行尾 EOL 后重新解码。行解码仍由 hayro
//! 承担，EOL 扫描、行号推进与错误恢复在适配层完成。

use hayro_ccitt::{
    DecodeSettings, Decoder as HayroDecoder, DecoderContext, EncodingMode, decode as hayro_decode,
};

use super::CcittParams;
use super::decoder::{CcittBackend, MAX_OUTPUT_BYTES};

/// 基于 `hayro-ccitt` 的 CCITT 后端。
pub(super) struct HayroCcittBackend;

impl CcittBackend for HayroCcittBackend {
    fn decode(&self, data: &[u8], params: &CcittParams) -> Result<Vec<u8>, String> {
        let settings = decode_settings(params)?;
        let mut stream = BitBuf::from_bytes(data);
        // `/EndOfBlock false` 时 hayro 会越过 EOB/RTC 继续解码；为使 EOB/RTC
        // 仍参与「三者最早」终止，先按位定位 EOB/RTC 并把码流截断到该处。
        if !params.end_of_block
            && let Some(eob) = eob_start(&stream, params)
        {
            stream.truncate(eob);
        }
        let mut damaged_left = params.damaged_rows_before_error;
        // 最近一次被替换的行号：识别连续损坏并防止对同一行反复替换。
        let mut last_substitute: Option<usize> = None;

        loop {
            let mut sink = RowSink::new(
                params.columns.div_ceil(8),
                params.rows,
                params.black_is_1,
            );
            let mut ctx = DecoderContext::new(settings);
            let error = match hayro_decode(stream.bytes(), &mut sink, &mut ctx) {
                Ok(_) => return sink.finish(),
                Err(error) => error,
            };
            // 一行也没解出来：错误原样上报（原实现同样直接报错）。
            if sink.rows == 0 {
                return Err(format!("CCITTFaxDecode: {error}"));
            }

            // 与原实现一致：只有 G3 + `/EndOfLine` 且还有可替换行时才恢复。
            if params.k < 0 || !params.end_of_line || damaged_left == 0 {
                return sink.finish();
            }

            let index = sink.rows;
            // 同一行替换后仍失败：停止，避免反复改写同一区间。
            if last_substitute == Some(index) {
                return sink.finish();
            }
            let runs = eol_runs(&stream);
            // 损坏行之后必须还有一个 EOL（原实现的 `seek_to_eol` 条件）。
            let Some(window) = damaged_window(&runs, index) else {
                return sink.finish();
            };

            // 连续损坏时按原实现口径输出空白行，否则复刻上一行。
            let previous_damaged = last_substitute == Some(index - 1);
            let reference = row_transitions(
                sink.row_bytes(index - 1),
                params.columns,
                params.black_is_1,
            );
            let replacement = substitute_row(params, &reference, !previous_damaged);
            stream.splice(window.start, window.end - window.start, &replacement);

            damaged_left -= 1;
            last_substitute = Some(index);
        }
    }
}

fn decode_settings(params: &CcittParams) -> Result<DecodeSettings, String> {
    Ok(DecodeSettings {
        columns: u32::try_from(params.columns)
            .map_err(|_| "CCITTFaxDecode: /Columns out of range".to_string())?,
        rows: match params.rows {
            Some(rows) => u32::try_from(rows)
                .map_err(|_| "CCITTFaxDecode: /Rows out of range".to_string())?,
            // 高度未预定义：交给 EOFB/RTC 或数据结束终止。
            None => u32::MAX,
        },
        // PDF 语义与原实现一致：EOB/RTC、`/Rows`、数据结束三者最早者终止。
        // `/EndOfBlock false` 只表示不*期望*显式 EOB，EOB/RTC 出现时仍须在此
        // 终止——该情形由 `decode` 先把码流截断到 EOB/RTC 再交给 hayro。
        end_of_block: params.end_of_block,
        end_of_line: params.end_of_line,
        rows_are_byte_aligned: params.encoded_byte_align,
        encoding: if params.k < 0 {
            EncodingMode::Group4
        } else if params.k == 0 {
            EncodingMode::Group3_1D
        } else {
            EncodingMode::Group3_2D { k: params.k as u32 }
        },
        // `/BlackIs1` 的位反转在打包阶段完成，库保持 0=黑、1=白。
        invert_black: false,
    })
}

/// 连续 EOL 的位区间与计数（6 个及以上为 RTC）。
#[derive(Debug, Clone, Copy)]
struct EolRun {
    start: usize,
    end: usize,
    count: usize,
}

/// 损坏行在码流中的位区间：行数据起点到其后第一个 EOL 的末尾。
#[derive(Debug, Clone, Copy)]
struct RowWindow {
    start: usize,
    end: usize,
}

/// Group 4 的 EOFB：连续两个 EOL（24 位，T.6 §2.4.1.1）。
const EOFB: u32 = 0x00_10_01;

/// 最早到达的 EOB/RTC 的起始位，供 `/EndOfBlock false` 截断码流。
fn eob_start(stream: &BitBuf, params: &CcittParams) -> Option<usize> {
    if params.k < 0 {
        let mut window = 0u32;
        for index in 0..stream.bit_len() {
            window = ((window << 1) | u32::from(stream.bit(index))) & 0x00ff_ffff;
            if index >= 23 && window == EOFB {
                return Some(index - 23);
            }
        }
        None
    } else {
        // Group 3 的 RTC：连续 6 个 EOL。
        eol_runs(stream)
            .into_iter()
            .find(|run| run.count >= 6)
            .map(|run| run.start)
    }
}

/// 按位扫描 EOL（`>=11` 个 0 后跟 1），紧邻的合并为一段；遇到 RTC 即停止。
fn eol_runs(stream: &BitBuf) -> Vec<EolRun> {
    let mut runs: Vec<EolRun> = Vec::new();
    let mut zeros = 0usize;
    let mut zero_start = 0usize;

    for index in 0..stream.bit_len() {
        if stream.bit(index) {
            if zeros >= 11 {
                match runs.last_mut() {
                    // 紧邻上一段 EOL：同一段连续 EOL。
                    Some(last) if last.end == zero_start => {
                        last.end = index + 1;
                        last.count += 1;
                    }
                    _ => runs.push(EolRun {
                        start: zero_start,
                        end: index + 1,
                        count: 1,
                    }),
                }
            }
            zeros = 0;
        } else {
            if zeros == 0 {
                zero_start = index;
            }
            zeros += 1;
        }
        if runs.last().is_some_and(|run| run.count >= 6) {
            // RTC：原实现在此终止，之后的行不再参与替换。
            break;
        }
    }
    runs
}

/// 第 `index` 行的替换区间。
///
/// 行数据从「上一条 EOL 的末尾」开始、到其后第一个 EOL 段的末尾结束。损坏
/// 行末尾的 0 位即使与其后的 EOL 合流，也会随该 EOL 一起被替换掉，与原实现
/// 「跳到下一 EOL 后用替换行继续」的恢复口径一致。
fn damaged_window(runs: &[EolRun], index: usize) -> Option<RowWindow> {
    // G3 可以带一个前导 EOL；此时第 0 行从首个 EOL 之后开始。
    let leading = runs.first().is_some_and(|run| run.start == 0);
    let (start, eol_index) = if leading {
        (runs.get(index)?.end, index + 1)
    } else if index == 0 {
        (0, 0)
    } else {
        (runs.get(index - 1)?.end, index)
    };
    Some(RowWindow {
        start,
        end: runs.get(eol_index)?.end,
    })
}

/// 提取一行像素的颜色跳变位置（行首按白色开始）。
fn row_transitions(row: &[u8], columns: usize, black_is_1: bool) -> Vec<usize> {
    let mut transitions = Vec::new();
    let mut white = true;
    for column in 0..columns {
        let bit = row[column / 8] & (0x80 >> (column % 8)) != 0;
        let pixel_white = if black_is_1 { !bit } else { bit };
        if pixel_white != white {
            transitions.push(column);
            white = pixel_white;
        }
    }
    transitions
}

/// 构造替换行的合法编码（不含行尾 EOL）。
///
/// `repeat` 为 true 时复刻上一行，否则输出全白行（连续损坏行按原实现口径）。
fn substitute_row(params: &CcittParams, reference: &[usize], repeat: bool) -> BitBuf {
    let mut out = BitBuf::new();
    if params.k > 0 {
        out.push_bit(false); // 二维行标签
        if repeat {
            // V0 链逐个复刻参考行的换色位置，末尾再补一个 V0 覆盖到行尾。
            for _ in 0..=reference.len() {
                out.push_bit(true);
            }
        } else {
            // Pass 链只输出白色像素且不改变颜色：得到全白行。
            for _ in 0..(reference.len() + 2) / 2 {
                out.push_code(0b0001, 4);
            }
        }
    } else {
        let transitions: &[usize] = if repeat { reference } else { &[] };
        encode_1d_line(params.columns, transitions, &mut out);
    }
    out
}

/// 用一维游程码编码一行；`transitions` 为换色位置，行首为白色。
fn encode_1d_line(columns: usize, transitions: &[usize], out: &mut BitBuf) {
    let mut previous = 0usize;
    let mut white = true;
    for &position in transitions {
        let position = position.min(columns);
        push_run(out, position.saturating_sub(previous), white);
        previous = position;
        white = !white;
    }
    push_run(out, columns.saturating_sub(previous), white);
}

/// 追加一个白/黑游程：超过 2560 时重复 makeup 码，最后跟终止码。
fn push_run(out: &mut BitBuf, run: usize, white: bool) {
    let table: &[(&str, u16)] = if white { WHITE_RUN_CODES } else { BLACK_RUN_CODES };
    let mut remaining = run;
    while remaining > 2560 {
        push_run_code(out, table, 2560);
        remaining -= 2560;
    }
    if remaining >= 64 {
        let makeup = remaining / 64 * 64;
        push_run_code(out, table, makeup);
        remaining -= makeup;
    }
    push_run_code(out, table, remaining);
}

fn push_run_code(out: &mut BitBuf, table: &[(&str, u16)], run: usize) {
    if let Some((code, _)) = table.iter().find(|(_, value)| usize::from(*value) == run) {
        out.push_code(
            u16::from_str_radix(code, 2).unwrap_or(0),
            code.len() as u8,
        );
    }
}

/// ITU-T T.4 白游程码：0–63 终止码，64–1728 与 1792–2560 为 makeup。
static WHITE_RUN_CODES: &[(&str, u16)] = &[
    ("00110101", 0),
    ("000111", 1),
    ("0111", 2),
    ("1000", 3),
    ("1011", 4),
    ("1100", 5),
    ("1110", 6),
    ("1111", 7),
    ("10011", 8),
    ("10100", 9),
    ("00111", 10),
    ("01000", 11),
    ("001000", 12),
    ("000011", 13),
    ("110100", 14),
    ("110101", 15),
    ("101010", 16),
    ("101011", 17),
    ("0100111", 18),
    ("0001100", 19),
    ("0001000", 20),
    ("0010111", 21),
    ("0000011", 22),
    ("0000100", 23),
    ("0101000", 24),
    ("0101011", 25),
    ("0010011", 26),
    ("0100100", 27),
    ("0011000", 28),
    ("00000010", 29),
    ("00000011", 30),
    ("00011010", 31),
    ("00011011", 32),
    ("00010010", 33),
    ("00010011", 34),
    ("00010100", 35),
    ("00010101", 36),
    ("00010110", 37),
    ("00010111", 38),
    ("00101000", 39),
    ("00101001", 40),
    ("00101010", 41),
    ("00101011", 42),
    ("00101100", 43),
    ("00101101", 44),
    ("00000100", 45),
    ("00000101", 46),
    ("00001010", 47),
    ("00001011", 48),
    ("01010010", 49),
    ("01010011", 50),
    ("01010100", 51),
    ("01010101", 52),
    ("00100100", 53),
    ("00100101", 54),
    ("01011000", 55),
    ("01011001", 56),
    ("01011010", 57),
    ("01011011", 58),
    ("01001010", 59),
    ("01001011", 60),
    ("00110010", 61),
    ("00110011", 62),
    ("00110100", 63),
    ("11011", 64),
    ("10010", 128),
    ("010111", 192),
    ("0110111", 256),
    ("00110110", 320),
    ("00110111", 384),
    ("01100100", 448),
    ("01100101", 512),
    ("01101000", 576),
    ("01100111", 640),
    ("011001100", 704),
    ("011001101", 768),
    ("011010010", 832),
    ("011010011", 896),
    ("011010100", 960),
    ("011010101", 1024),
    ("011010110", 1088),
    ("011010111", 1152),
    ("011011000", 1216),
    ("011011001", 1280),
    ("011011010", 1344),
    ("011011011", 1408),
    ("010011000", 1472),
    ("010011001", 1536),
    ("010011010", 1600),
    ("011000", 1664),
    ("010011011", 1728),
    ("00000001000", 1792),
    ("00000001100", 1856),
    ("00000001101", 1920),
    ("000000010010", 1984),
    ("000000010011", 2048),
    ("000000010100", 2112),
    ("000000010101", 2176),
    ("000000010110", 2240),
    ("000000010111", 2304),
    ("000000011100", 2368),
    ("000000011101", 2432),
    ("000000011110", 2496),
    ("000000011111", 2560),
];

/// ITU-T T.4 黑游程码：0–63 终止码，64–1728 与 1792–2560 为 makeup。
static BLACK_RUN_CODES: &[(&str, u16)] = &[
    ("0000110111", 0),
    ("010", 1),
    ("11", 2),
    ("10", 3),
    ("011", 4),
    ("0011", 5),
    ("0010", 6),
    ("00011", 7),
    ("000101", 8),
    ("000100", 9),
    ("0000100", 10),
    ("0000101", 11),
    ("0000111", 12),
    ("00000100", 13),
    ("00000111", 14),
    ("000011000", 15),
    ("0000010111", 16),
    ("0000011000", 17),
    ("0000001000", 18),
    ("00001100111", 19),
    ("00001101000", 20),
    ("00001101100", 21),
    ("00000110111", 22),
    ("00000101000", 23),
    ("00000010111", 24),
    ("00000011000", 25),
    ("000011001010", 26),
    ("000011001011", 27),
    ("000011001100", 28),
    ("000011001101", 29),
    ("000001101000", 30),
    ("000001101001", 31),
    ("000001101010", 32),
    ("000001101011", 33),
    ("000011010010", 34),
    ("000011010011", 35),
    ("000011010100", 36),
    ("000011010101", 37),
    ("000011010110", 38),
    ("000011010111", 39),
    ("000001101100", 40),
    ("000001101101", 41),
    ("000011011010", 42),
    ("000011011011", 43),
    ("000001010100", 44),
    ("000001010101", 45),
    ("000001010110", 46),
    ("000001010111", 47),
    ("000001100100", 48),
    ("000001100101", 49),
    ("000001010010", 50),
    ("000001010011", 51),
    ("000000100100", 52),
    ("000000110111", 53),
    ("000000111000", 54),
    ("000000100111", 55),
    ("000000101000", 56),
    ("000001011000", 57),
    ("000001011001", 58),
    ("000000101011", 59),
    ("000000101100", 60),
    ("000001011010", 61),
    ("000001100110", 62),
    ("000001100111", 63),
    ("0000001111", 64),
    ("000011001000", 128),
    ("000011001001", 192),
    ("000001011011", 256),
    ("000000110011", 320),
    ("000000110100", 384),
    ("000000110101", 448),
    ("0000001101100", 512),
    ("0000001101101", 576),
    ("0000001001010", 640),
    ("0000001001011", 704),
    ("0000001001100", 768),
    ("0000001001101", 832),
    ("0000001110010", 896),
    ("0000001110011", 960),
    ("0000001110100", 1024),
    ("0000001110101", 1088),
    ("0000001110110", 1152),
    ("0000001110111", 1216),
    ("0000001010010", 1280),
    ("0000001010011", 1344),
    ("0000001010100", 1408),
    ("0000001010101", 1472),
    ("0000001011010", 1536),
    ("0000001011011", 1600),
    ("0000001100100", 1664),
    ("0000001100101", 1728),
    ("00000001000", 1792),
    ("00000001100", 1856),
    ("00000001101", 1920),
    ("000000010010", 1984),
    ("000000010011", 2048),
    ("000000010100", 2112),
    ("000000010101", 2176),
    ("000000010110", 2240),
    ("000000010111", 2304),
    ("000000011100", 2368),
    ("000000011101", 2432),
    ("000000011110", 2496),
    ("000000011111", 2560),
];

/// MSB 优先的位缓冲：`bit_len` 为有效位数，末尾不足一字节的位为 0。
#[derive(Debug, Clone)]
struct BitBuf {
    bytes: Vec<u8>,
    bit_len: usize,
}

impl BitBuf {
    fn new() -> Self {
        Self {
            bytes: Vec::new(),
            bit_len: 0,
        }
    }

    fn from_bytes(data: &[u8]) -> Self {
        Self {
            bytes: data.to_vec(),
            bit_len: data.len() * 8,
        }
    }

    fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    fn bit_len(&self) -> usize {
        self.bit_len
    }

    fn bit(&self, index: usize) -> bool {
        self.bytes[index / 8] & (0x80 >> (index % 8)) != 0
    }

    fn push_bit(&mut self, value: bool) {
        if self.bit_len.is_multiple_of(8) {
            self.bytes.push(0);
        }
        if value {
            self.bytes[self.bit_len / 8] |= 0x80 >> (self.bit_len % 8);
        }
        self.bit_len += 1;
    }

    fn push_code(&mut self, code: u16, len: u8) {
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
    fn truncate(&mut self, bit_len: usize) {
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
    fn splice(&mut self, start: usize, old_len: usize, row: &BitBuf) {
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

/// 逐行打包的 [`HayroDecoder`]：每行 `ceil(Columns/8)` 字节。
struct RowSink {
    out: Vec<u8>,
    row: Vec<u8>,
    bytes_per_row: usize,
    bit_in_row: usize,
    rows: usize,
    expected_rows: Option<usize>,
    black_is_1: bool,
    overflowed: bool,
}

impl RowSink {
    fn new(bytes_per_row: usize, expected_rows: Option<usize>, black_is_1: bool) -> Self {
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
    fn row_bytes(&self, index: usize) -> &[u8] {
        let start = index * self.bytes_per_row;
        &self.out[start..start + self.bytes_per_row]
    }

    fn finish(mut self) -> Result<Vec<u8>, String> {
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
}

impl HayroDecoder for RowSink {
    fn push_pixel(&mut self, white: bool) {
        if self.bit_in_row >= self.bytes_per_row * 8 {
            return;
        }
        if self.bit(white) == 1 {
            self.row[self.bit_in_row / 8] |= 1 << (7 - self.bit_in_row % 8);
        }
        self.bit_in_row += 1;
    }

    fn push_pixel_chunk(&mut self, white: bool, chunk_count: u32) {
        let start = self.bit_in_row / 8;
        let end = start
            .saturating_add(chunk_count as usize)
            .min(self.bytes_per_row);
        let value = if self.bit(white) == 1 { 0xFF } else { 0x00 };
        self.row[start..end].fill(value);
        self.bit_in_row = end * 8;
    }

    fn next_line(&mut self) {
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
