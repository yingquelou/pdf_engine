//! `ccitt` 模块测试。

use super::*;

fn bits_to_bytes(bits: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut byte = 0u8;
    let mut n = 0u8;
    for &b in bits {
        byte = (byte << 1) | (b & 1);
        n += 1;
        if n == 8 {
            out.push(byte);
            byte = 0;
            n = 0;
        }
    }
    if n > 0 {
        out.push(byte << (8 - n));
    }
    out
}

fn code_bits(code: &str) -> Vec<u8> {
    code.bytes().map(|b| b - b'0').collect()
}

fn push_code(bits: &mut Vec<u8>, code: &str) {
    bits.extend(code_bits(code));
}

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

#[test]
fn g3_1d_decodes_runs() {
    // 16 像素：白 4（1011）+ 黑 4（011）+ 白 8（10011）
    let mut bits = Vec::new();
    push_code(&mut bits, "1011");
    push_code(&mut bits, "011");
    push_code(&mut bits, "10011");
    let data = bits_to_bytes(&bits);
    let p = params(0, 16, Some(1));
    assert_eq!(decode(&data, &p).unwrap(), vec![0xF0, 0xFF]);
}

#[test]
fn g3_1d_all_white_and_black_is_1() {
    // 8 像素全白：白 8 = 10011
    let mut bits = Vec::new();
    push_code(&mut bits, "10011");
    let data = bits_to_bytes(&bits);
    let mut p = params(0, 8, Some(1));
    assert_eq!(decode(&data, &p).unwrap(), vec![0xFF]);

    p.black_is_1 = true;
    assert_eq!(decode(&data, &p).unwrap(), vec![0x00]);
}

#[test]
fn g4_2d_modes_decode() {
    // 第 1 行全白：V0（1）
    // 第 2 行：水平 001 + 白 4(1011) + 黑 4(011)
    // 第 3 行：两个 V0（1,1），相对第 2 行
    // EOFB：000000000001 000000000001
    let mut bits = Vec::new();
    push_code(&mut bits, "1");
    push_code(&mut bits, "0011011011");
    push_code(&mut bits, "11");
    for _ in 0..2 {
        push_code(&mut bits, "000000000001");
    }
    let data = bits_to_bytes(&bits);
    let p = params(-1, 8, Some(3));
    assert_eq!(decode(&data, &p).unwrap(), vec![0xFF, 0xF0, 0xF0]);
}

#[test]
fn g3_2d_tag_and_eol() {
    // 第 1 行 1-D 全白：EOL + tag 1 + 白 8
    // 第 2 行 2-D：EOL + tag 0 + 水平(001) + 白 4 + 黑 4
    let mut bits = Vec::new();
    push_code(&mut bits, "000000000001");
    push_code(&mut bits, "1");
    push_code(&mut bits, "10011");
    push_code(&mut bits, "000000000001");
    push_code(&mut bits, "0");
    push_code(&mut bits, "0011011011");
    push_code(&mut bits, "000000000001");
    let data = bits_to_bytes(&bits);
    let p = params(1, 8, Some(2));
    assert_eq!(decode(&data, &p).unwrap(), vec![0xFF, 0xF0]);
}

#[test]
fn truncated_first_row_errors() {
    // 8 像素却只有白 4 的码，且无后续数据/EOB：首行失败。
    let data = bits_to_bytes(&code_bits("1011"));
    let p = params(0, 8, Some(1));
    assert!(decode(&data, &p).is_err());
}

#[test]
fn g3_1d_makeup_run() {
    // 128 像素全白：makeup 128（10010）+ 终止 0（00110101）。
    let mut bits = Vec::new();
    push_code(&mut bits, "10010");
    push_code(&mut bits, "00110101");
    let data = bits_to_bytes(&bits);
    let p = params(0, 128, Some(1));
    assert_eq!(decode(&data, &p).unwrap(), vec![0xFF; 16]);
}

#[test]
fn g3_1d_encoded_byte_align() {
    // 两行全白（宽 8），行间以 0 填充到字节边界；EncodedByteAlign=true。
    let mut bits = Vec::new();
    push_code(&mut bits, "10011");
    while bits.len() % 8 != 0 {
        bits.push(0);
    }
    push_code(&mut bits, "10011");
    let data = bits_to_bytes(&bits);
    let mut p = params(0, 8, Some(2));
    p.encoded_byte_align = true;
    assert_eq!(decode(&data, &p).unwrap(), vec![0xFF, 0xFF]);
}

/// libtiff 编码的 8×3 Group 4 条带（TIFF Photometric=MinIsWhite）：
/// 第 1 行全白，第 2/3 行前 4 像素白、后 4 像素黑。
/// 条带字节见测试；输出与 libtiff 解码像素一致。
#[test]
fn g4_matches_libtiff_sample() {
    let data = [0x9Bu8, 0x78, 0x00, 0x80, 0x08];
    let p = params(-1, 8, Some(3));
    assert_eq!(decode(&data, &p).unwrap(), vec![0xFF, 0xF0, 0xF0]);
}

/// 更复杂的 libtiff 样本（16×8，包含全白/全黑/不同游程），覆盖
/// Pass/垂直/水平模式；期望字节由 libtiff 解码后的像素行打包得到。
#[test]
fn g4_matches_libtiff_varied_sample() {
    let data = [
        0x9Bu8, 0x70, 0x88, 0x4B, 0x08, 0x82, 0xE6, 0xA6, 0x62, 0x91, 0xC1, 0xE3, 0x3D, 0xC0,
        0x04, 0x00, 0x40,
    ];
    let p = params(-1, 16, Some(8));
    assert_eq!(
        decode(&data, &p).unwrap(),
        vec![
            0xFF, 0xFF, 0xF0, 0xFF, 0xC0, 0x0F, 0x00, 0x00, 0xFF, 0x00, 0x80, 0x01, 0xFF, 0xFF,
            0xFC, 0xFF,
        ]
    );
}

/// Pass 模式（0001）跨越参考行的黑色游程，然后 V0 收尾。
#[test]
fn g4_pass_mode() {
    // 第 1 行：白 4 + 黑 4 + 白 8（宽 16）→ 水平 + V0。
    // 第 2 行：全白，先 Pass 到 b2=8，再 V0 到行尾。
    let mut bits = Vec::new();
    push_code(&mut bits, "0011011011"); // 水平 + 白4 + 黑4
    push_code(&mut bits, "1"); // V0 到行尾
    push_code(&mut bits, "0001"); // Pass：a0 -> b2=8
    push_code(&mut bits, "1"); // V0 到行尾
    for _ in 0..2 {
        push_code(&mut bits, "000000000001");
    }
    let data = bits_to_bytes(&bits);
    let p = params(-1, 16, Some(2));
    assert_eq!(decode(&data, &p).unwrap(), vec![0xF0, 0xFF, 0xFF, 0xFF]);
}

/// 负向垂直模式（Vl1=010）：第 2 行相对第 1 行向左 1 像素。
#[test]
fn g4_vertical_left() {
    // 第 1 行：白 4 + 黑 4（宽 8）→ 水平。
    // 第 2 行：白 3 + 黑 5 → Vl1 + V0。
    let mut bits = Vec::new();
    push_code(&mut bits, "0011011011");
    push_code(&mut bits, "010"); // Vl1：新边界 4-1=3
    push_code(&mut bits, "1"); // V0 到行尾
    for _ in 0..2 {
        push_code(&mut bits, "000000000001");
    }
    let data = bits_to_bytes(&bits);
    let p = params(-1, 8, Some(2));
    assert_eq!(decode(&data, &p).unwrap(), vec![0xF0, 0xE0]);
}

#[test]
fn g4_rows_unknown_stops_at_eofb() {
    let mut bits = Vec::new();
    push_code(&mut bits, "1"); // 全白行
    for _ in 0..2 {
        push_code(&mut bits, "000000000001");
    }
    let data = bits_to_bytes(&bits);
    let p = params(-1, 8, None);
    assert_eq!(decode(&data, &p).unwrap(), vec![0xFF]);
}

#[test]
fn g4_black_is_1_inverts_output() {
    let mut bits = Vec::new();
    push_code(&mut bits, "1");
    for _ in 0..2 {
        push_code(&mut bits, "000000000001");
    }
    let data = bits_to_bytes(&bits);
    let mut p = params(-1, 8, Some(1));
    p.black_is_1 = true;
    assert_eq!(decode(&data, &p).unwrap(), vec![0x00]);
}

#[test]
fn rejects_oversized_dimensions() {
    let p = params(-1, 8, Some(usize::MAX / 4));
    assert!(decode(&[0], &p).is_err());
}
