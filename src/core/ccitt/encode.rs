//! 损坏行替换的重新编码：换色位置提取与 T.4/T.6 行编码。
//!
//! 契约：输入为一行像素的换色位置、`/Columns` 与 `/K`；输出为该行的合法行编码
//! 位串（不含行尾 EOL），供 `backend` 替换损坏区间后重新解码。游程码表与唯一
//! 使用它的编码逻辑同处本文件。

use super::CcittParams;
use super::bitbuf::BitBuf;

/// 提取一行像素的颜色跳变位置（行首按白色开始）。
pub(super) fn row_transitions(row: &[u8], columns: usize, black_is_1: bool) -> Vec<usize> {
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
pub(super) fn substitute_row(params: &CcittParams, reference: &[usize], repeat: bool) -> BitBuf {
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

#[cfg(test)]
mod tests {
    use super::{row_transitions, substitute_row};
    use crate::core::ccitt::CcittParams;
    use crate::core::ccitt::decode;

    fn params(k: i32, columns: usize, rows: Option<usize>) -> CcittParams {
        CcittParams {
            k,
            columns,
            rows,
            end_of_line: false,
            encoded_byte_align: false,
            end_of_block: true,
            black_is_1: false,
            damaged_rows_before_error: 0,
        }
    }

    fn to_bytes(bits: &[bool]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut byte = 0u8;
        let mut count = 0u8;
        for &bit in bits {
            byte = (byte << 1) | u8::from(bit);
            count += 1;
            if count == 8 {
                out.push(byte);
                byte = 0;
                count = 0;
            }
        }
        if count > 0 {
            out.push(byte << (8 - count));
        }
        out
    }

    fn push_bits(bits: &mut Vec<bool>, pattern: &str) {
        bits.extend(pattern.bytes().map(|byte| byte == b'1'));
    }

    #[test]
    fn transitions_follow_black_is_1() {
        // 0x3C 的位序列为 0,0,1,1,1,1,0,0。
        // 默认 1=白：行首即为黑，换色位置为 0、2、6。
        let row = [0x3C];
        assert_eq!(row_transitions(&row, 8, false), vec![0, 2, 6]);
        // `/BlackIs1`：1=黑，行首为白，换色位置为 2、6。
        assert_eq!(row_transitions(&row, 8, true), vec![2, 6]);
    }

    /// 替换行编码用既有解码器反向验证：复刻的行编码解出后应当就是参考行。
    #[test]
    fn repeat_row_round_trips_through_decoder() {
        let replacement = substitute_row(&params(0, 8, Some(1)), &[4], true);
        assert_eq!(
            decode(replacement.bytes(), &params(0, 8, Some(1))).unwrap(),
            vec![0xF0]
        );
    }

    /// 全白替换行：空换色位置经一维游程编码后解码为全白。
    #[test]
    fn blank_row_round_trips_through_decoder() {
        let replacement = substitute_row(&params(0, 8, Some(1)), &[4], false);
        assert_eq!(
            decode(replacement.bytes(), &params(0, 8, Some(1))).unwrap(),
            vec![0xFF]
        );
    }

    /// G3 2-D 的 V0 链复刻：参考行由第 1 行给出，替换编码作为第 2 行解码。
    #[test]
    fn two_dimensional_repeat_round_trips_through_decoder() {
        let eol = "000000000001";
        let mut bits = Vec::new();
        // 第 1 行 1-D：EOL + 标签 1 + 白 4 + 黑 4 → 0xF0
        push_bits(&mut bits, &(eol.to_owned() + "1"));
        push_bits(&mut bits, "1011");
        push_bits(&mut bits, "011");
        push_bits(&mut bits, eol);

        let replacement = substitute_row(&params(2, 8, Some(2)), &[4], true);
        for index in 0..replacement.bit_len() {
            bits.push(replacement.bit(index));
        }
        push_bits(&mut bits, eol);

        let mut p = params(2, 8, Some(2));
        p.end_of_line = true;
        assert_eq!(decode(&to_bytes(&bits), &p).unwrap(), vec![0xF0, 0xF0]);
    }
}
