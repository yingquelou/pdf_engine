//! CCITT Group 3/4 行解码与位流处理。

use super::tables::{black_codes, white_codes, Mode, MODE_CODES};
use super::CcittParams;

/// 输出规模上限（防止畸形 `/Columns`/`Rows` 触发超大分配）。
const MAX_OUTPUT_BYTES: usize = 256 * 1024 * 1024;

/// 解码 CCITT 码流，返回按行打包的 1 位像素数据。
pub(crate) fn decode(data: &[u8], params: &CcittParams) -> Result<Vec<u8>, String> {
    if params.columns == 0 {
        return Err("CCITTFaxDecode: /Columns must be positive".to_string());
    }
    let width = params.columns;
    let bytes_per_row = width.div_ceil(8);
    if bytes_per_row > MAX_OUTPUT_BYTES {
        return Err("CCITTFaxDecode: /Columns too large".to_string());
    }
    if let Some(rows) = params.rows {
        let total = bytes_per_row
            .checked_mul(rows)
            .ok_or_else(|| "CCITTFaxDecode: output size overflow".to_string())?;
        if total > MAX_OUTPUT_BYTES {
            return Err("CCITTFaxDecode: /Columns * /Rows exceeds output limit".to_string());
        }
    }
    let max_rows = params.rows.unwrap_or(usize::MAX);
    let mut out = Vec::new();
    let mut reader = BitReader::new(data);
    let mut reference: Vec<u32> = Vec::new();
    let mut rows_decoded = 0usize;
    let mut damaged_left = params.damaged_rows_before_error;
    let mut previous_damaged = false;
    let mut byte_align = params.encoded_byte_align;

    while rows_decoded < max_rows {
        if reader.at_end() {
            break;
        }
        if out.len().saturating_add(bytes_per_row) > MAX_OUTPUT_BYTES {
            return Err("CCITTFaxDecode: output exceeds size limit".to_string());
        }

        let scheme = if params.k < 0 {
            Scheme::G4
        } else if params.k == 0 {
            Scheme::G3OneD
        } else {
            Scheme::G3TwoD
        };

        match scheme {
            Scheme::G4 => {
                if byte_align && rows_decoded > 0 {
                    align_to_byte(&mut reader, &mut byte_align);
                }
            }
            Scheme::G3OneD | Scheme::G3TwoD => {
                // `EncodedByteAlign` 的填充位位于下一行（含 EOL）之前，
                // 因此先对齐再消费 EOL。
                if byte_align {
                    align_to_byte(&mut reader, &mut byte_align);
                }
                // Group 3 的 EOL 可能带 0–7 个填充位；6 个连续 EOL 为 RTC。
                let eols = consume_eols(&mut reader);
                if eols >= 6 || reader.at_end() {
                    break;
                }
            }
        }

        let one_dimensional = match scheme {
            Scheme::G4 => false,
            Scheme::G3OneD => true,
            Scheme::G3TwoD => match reader.read(1) {
                Some(1) => true,
                Some(0) => false,
                _ => break,
            },
        };

        let row = if one_dimensional {
            decode_row_1d(&mut reader, width)
        } else {
            decode_row_2d(&mut reader, width, &reference)
        };

        match row {
            Ok(RowEnd::EndOfBlock) => break,
            Ok(RowEnd::Row(transitions)) => {
                out.extend_from_slice(&pack_row(&transitions, width, params.black_is_1));
                reference = transitions;
                rows_decoded += 1;
                previous_damaged = false;
            }
            Err(err) => {
                // 损坏行容忍：仅在 G3 + EndOfLine 且已解出至少一行时生效，
                // 跳到下一 EOL 并用上一行（或全白行）替换。
                if scheme != Scheme::G4
                    && params.end_of_line
                    && damaged_left > 0
                    && rows_decoded > 0
                    && seek_to_eol(&mut reader)
                {
                    let substitute = if previous_damaged {
                        Vec::new()
                    } else {
                        reference.clone()
                    };
                    out.extend_from_slice(&pack_row(&substitute, width, params.black_is_1));
                    reference = substitute;
                    rows_decoded += 1;
                    damaged_left -= 1;
                    previous_damaged = true;
                    continue;
                }
                if rows_decoded == 0 {
                    return Err(err);
                }
                // 已有可用行：保留前缀，按截断处理。
                break;
            }
        }
    }

    // 已知高度但码流提前结束：以白行补齐（与常见阅读器的宽容行为一致）。
    if let Some(expected) = params.rows
        && out.len() / bytes_per_row < expected
    {
        let white = pack_row(&[], width, params.black_is_1);
        while out.len() / bytes_per_row < expected {
            out.extend_from_slice(&white);
        }
    }

    if out.is_empty() {
        return Err("CCITTFaxDecode: no rows decoded".to_string());
    }

    Ok(out)
}

/// 编码方案（由 `/K` 的正负/零决定）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scheme {
    G3OneD,
    G3TwoD,
    G4,
}

/// 单行解码结果。
enum RowEnd {
    Row(Vec<u32>),
    EndOfBlock,
}

/// Group 3 1-D（Modified Huffman）：从白色游程开始交替解码，直到覆盖整行。
fn decode_row_1d(reader: &mut BitReader<'_>, width: usize) -> Result<RowEnd, String> {
    let mut transitions = Vec::new();
    let mut a0 = 0usize;
    let mut white = true;

    while a0 < width {
        let run = read_run(reader, white)?;
        let a1 = a0
            .checked_add(run)
            .ok_or_else(|| "CCITTFaxDecode: run length overflow".to_string())?;
        if a1 >= width {
            break;
        }
        transitions.push(a1 as u32);
        a0 = a1;
        white = !white;
    }

    Ok(RowEnd::Row(transitions))
}

/// Group 3 2-D / Group 4 的二维行解码，输出相对白色起点的换色位置。
fn decode_row_2d(
    reader: &mut BitReader<'_>,
    width: usize,
    reference: &[u32],
) -> Result<RowEnd, String> {
    let mut transitions: Vec<u32> = Vec::new();
    let mut pos = 0usize;
    let mut a0 = 0usize;
    let mut black = false;
    let mut start_of_row = true;

    loop {
        let mode = read_mode(reader)?;
        match mode {
            Mode::Pass => {
                if start_of_row && !black {
                    pos += 1;
                } else if next_color(reference, &mut pos, a0, !black, false).is_none() {
                    return Err("CCITTFaxDecode: pass mode without b1".to_string());
                }
                if let Some(b2) = ref_next(reference, &mut pos) {
                    a0 = b2;
                }
            }
            Mode::Vertical(delta) => {
                let b1 = next_color(reference, &mut pos, a0, !black, start_of_row)
                    .unwrap_or(width);
                let a1 = b1 as i64 + delta as i64;
                if a1 < 0 || a1 > width as i64 {
                    break;
                }
                let a1 = a1 as usize;
                if a1 < width {
                    transitions.push(a1 as u32);
                }
                black = !black;
                a0 = a1;
                if delta < 0 {
                    seek_back(reference, &mut pos, a0);
                }
            }
            Mode::Horizontal => {
                let r1 = read_run(reader, !black)?;
                let r2 = read_run(reader, black)?;
                let a1 = a0
                    .checked_add(r1)
                    .ok_or_else(|| "CCITTFaxDecode: horizontal run overflow".to_string())?;
                let a2 = a1
                    .checked_add(r2)
                    .ok_or_else(|| "CCITTFaxDecode: horizontal run overflow".to_string())?;
                if a1 < width {
                    transitions.push(a1 as u32);
                }
                if a2 >= width {
                    break;
                }
                transitions.push(a2 as u32);
                a0 = a2;
            }
            Mode::Extension => {
                return Err("CCITTFaxDecode: uncompressed extension mode is not supported".to_string());
            }
            Mode::Eof => return Ok(RowEnd::EndOfBlock),
        }
        start_of_row = false;
        if a0 >= width {
            break;
        }
    }

    Ok(RowEnd::Row(transitions))
}

/// 在参考行中找 b1：`start` 右侧、颜色为 `want_black` 的第一个换色位置。
///
/// `edges[k]` 是换色位置，首段为白色，故 `k` 偶数时该边开始黑色、奇数时开始白色。
fn next_color(
    edges: &[u32],
    pos: &mut usize,
    start: usize,
    want_black: bool,
    start_of_row: bool,
) -> Option<usize> {
    if start_of_row {
        if want_black {
            *pos = 1;
            return edges.first().map(|&v| v as usize);
        }
        *pos = 2;
        return edges.get(1).map(|&v| v as usize);
    }
    while *pos < edges.len() {
        if edges[*pos] as usize <= start {
            *pos += 1;
            continue;
        }
        if (*pos).is_multiple_of(2) != want_black {
            *pos += 1;
        }
        break;
    }
    if *pos < edges.len() {
        let v = edges[*pos] as usize;
        *pos += 1;
        Some(v)
    } else {
        None
    }
}

/// 取参考行的下一条边（b2）。
fn ref_next(edges: &[u32], pos: &mut usize) -> Option<usize> {
    if *pos < edges.len() {
        let v = edges[*pos] as usize;
        *pos += 1;
        Some(v)
    } else {
        None
    }
}

/// 负向垂直模式后回退参考行游标。
fn seek_back(edges: &[u32], pos: &mut usize, start: usize) {
    *pos = (*pos).min(edges.len().saturating_sub(1));
    while *pos > 0 && start < edges[*pos - 1] as usize {
        *pos -= 1;
    }
}

// ── 位读取 ────────────────────────────────────────────────────────────

/// MSB-first 位读取器；越界返回 `None`，绝不 panic。
struct BitReader<'a> {
    data: &'a [u8],
    bit_pos: usize,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, bit_pos: 0 }
    }

    fn peek(&self, width: usize) -> Option<u16> {
        if width == 0 || width > 16 || self.bit_pos + width > self.data.len() * 8 {
            return None;
        }
        let mut value = 0u16;
        for i in 0..width {
            let bit_pos = self.bit_pos + i;
            let bit = (self.data[bit_pos / 8] >> (7 - bit_pos % 8)) & 1;
            value = (value << 1) | u16::from(bit);
        }
        Some(value)
    }

    fn read(&mut self, width: usize) -> Option<u16> {
        let value = self.peek(width)?;
        self.bit_pos += width;
        Some(value)
    }

    fn consume(&mut self, width: usize) {
        self.bit_pos += width;
    }

    fn at_end(&self) -> bool {
        self.bit_pos >= self.data.len() * 8
    }
}

/// 跳过 EOL（`000000000001`，允许前置 0–7 填充位），返回消费的 EOL 数量。
///
/// Group 3 的 EOL 前可有任意数量 0 位；若遇到不足 11 个 0 的非 EOL 序列，
/// 回退到扫描前，交给行解码。
fn consume_eols(reader: &mut BitReader<'_>) -> usize {
    let mut count = 0usize;
    loop {
        let save = reader.bit_pos;
        let mut zeros = 0usize;
        while reader.peek(1) == Some(0) {
            reader.consume(1);
            zeros += 1;
        }
        if zeros >= 11 {
            if reader.peek(1) == Some(1) {
                reader.consume(1);
                count += 1;
                continue;
            }
            // 数据在填充 0 后结束：视为终止。
            return count;
        }
        reader.bit_pos = save;
        return count;
    }
}

/// 跳到下一 EOL 的起点（不消费），用于损坏行重同步。
fn seek_to_eol(reader: &mut BitReader<'_>) -> bool {
    let mut pos = reader.bit_pos;
    let total = reader.data.len() * 8;
    while pos < total {
        let mut zeros = 0usize;
        let mut p = pos;
        while p < total && (reader.data[p / 8] >> (7 - p % 8)) & 1 == 0 {
            zeros += 1;
            p += 1;
        }
        if zeros >= 11 {
            if p < total && (reader.data[p / 8] >> (7 - p % 8)) & 1 == 1 {
                reader.bit_pos = pos;
                return true;
            }
            return false;
        }
        pos += 1;
    }
    false
}

/// 若启用 `EncodedByteAlign`，跳过到下一字节边界；填充位非 0 时禁用对齐。
fn align_to_byte(reader: &mut BitReader<'_>, active: &mut bool) {
    let rem = reader.bit_pos % 8;
    if rem == 0 {
        return;
    }
    let save = reader.bit_pos;
    for _ in rem..8 {
        match reader.peek(1) {
            Some(0) => reader.consume(1),
            Some(_) => {
                reader.bit_pos = save;
                *active = false;
                return;
            }
            None => return,
        }
    }
}

// ── 码表读取 ──────────────────────────────────────────────────────────

fn read_mode(reader: &mut BitReader<'_>) -> Result<Mode, String> {
    for &(len, code, mode) in MODE_CODES {
        if reader.peek(len as usize) == Some(code) {
            reader.consume(len as usize);
            return Ok(mode);
        }
    }
    Err("CCITTFaxDecode: invalid 2-D mode code".to_string())
}

/// 读取一个白/黑游程（可含多个 makeup 码，累计到 `<64` 的终止码）。
fn read_run(reader: &mut BitReader<'_>, white: bool) -> Result<usize, String> {
    let table = if white { white_codes() } else { black_codes() };
    let mut total = 0usize;
    loop {
        let n = lookup_run(reader, table)?;
        total = total
            .checked_add(n as usize)
            .ok_or_else(|| "CCITTFaxDecode: run length overflow".to_string())?;
        if n < 64 {
            return Ok(total);
        }
        if total > 100_000 {
            return Err("CCITTFaxDecode: run length unreasonable".to_string());
        }
    }
}

fn lookup_run(reader: &mut BitReader<'_>, table: &[(u8, u16, u16)]) -> Result<u16, String> {
    for &(len, code, run) in table {
        if reader.peek(len as usize) == Some(code) {
            reader.consume(len as usize);
            return Ok(run);
        }
    }
    Err("CCITTFaxDecode: invalid run-length code".to_string())
}

fn pack_row(transitions: &[u32], width: usize, black_is_1: bool) -> Vec<u8> {
    let mut out = vec![0u8; width.div_ceil(8)];
    let mut white = true;
    let mut prev = 0usize;

    for &t in transitions {
        let end = (t as usize).min(width);
        set_span(&mut out, prev, end, white, black_is_1);
        prev = end;
        white = !white;
    }
    set_span(&mut out, prev, width, white, black_is_1);

    // 行末不足一字节的位按白色补齐（未使用位不参与图像语义）。
    let rem = width % 8;
    if rem != 0 && !black_is_1 {
        let last = out.len() - 1;
        out[last] |= (1u8 << (8 - rem)) - 1;
    }
    out
}

fn set_span(out: &mut [u8], from: usize, to: usize, white: bool, black_is_1: bool) {
    let bit = if black_is_1 { !white } else { white };
    for x in from..to {
        if bit {
            out[x / 8] |= 1 << (7 - x % 8);
        }
    }
}
