//! LZW/Flate 共用的 Predictor（1/2/10–15）。

use crate::core::types::Dict;

use super::{DecodeError, Result};

/// 预测器参数（`/DecodeParms` 中与预测相关的字段，取缺省值）。
#[derive(Debug, Clone, Copy)]
struct Predictor {
    /// 预测器种类：1 无、2 TIFF、10–15 PNG。
    ///
    /// 缺省 1。若 `DecodeParms` 存在但未给 /Predictor，规范默认 1。
    predictor: i64,
    /// 颜色分量数（每像素分量数），缺省 1。
    colors: i64,
    /// 每分量位数，缺省 8。
    bits: i64,
    /// 每行像素数（列数），缺省 1。
    columns: i64,
}

impl Predictor {
    fn from_dict(d: &Dict) -> Self {
        Self {
            predictor: d.get("Predictor").and_then(|v| v.as_integer()).unwrap_or(1),
            colors: d.get("Colors").and_then(|v| v.as_integer()).unwrap_or(1),
            bits: d
                .get("BitsPerComponent")
                .and_then(|v| v.as_integer())
                .unwrap_or(8),
            columns: d.get("Columns").and_then(|v| v.as_integer()).unwrap_or(1),
        }
    }
}

/// 应用预测器（LZW/Flate 共用）。无参数（`None`）视为 Predictor 1。
pub(super) fn apply_predictor(data: Vec<u8>, params: Option<&Dict>) -> Result<Vec<u8>> {
    let p = match params {
        None => return Ok(data),
        Some(d) => Predictor::from_dict(d),
    };
    match p.predictor {
        1 => Ok(data),
        2 => tiff_predictor(&data, &p),
        10..=15 => png_predictor(&data, &p),
        other => Err(DecodeError::new(format!(
            "unsupported Predictor {other} (cannot decode)"
        ))),
    }
}

/// 校验并计算每行字节数与每像素字节数。返回 `(row_bytes, bpp)`。
fn row_geometry(p: &Predictor) -> Result<(usize, usize)> {
    if p.colors <= 0 || p.bits <= 0 || p.columns <= 0 {
        return Err(DecodeError::new(format!(
            "invalid DecodeParms: Colors={} BitsPerComponent={} Columns={}",
            p.colors, p.bits, p.columns
        )));
    }
    let bits_per_pixel = p.colors * p.bits; // 每像素位数
    let row_bits = bits_per_pixel * p.columns; // 每行位数
    let row_bytes = (row_bits + 7) / 8;
    let bpp = (bits_per_pixel + 7) / 8;
    if row_bytes <= 0 || row_bytes > i64::from(u32::MAX) {
        return Err(DecodeError::new("DecodeParms row length out of range"));
    }
    Ok((row_bytes as usize, bpp.max(1) as usize))
}

/// PNG 预测器（10–15）。10–14 为整幅统一的过滤类型（tag = predictor-10），
/// 15 为「最优」：每行首字节给出该行的过滤类型。
fn png_predictor(data: &[u8], p: &Predictor) -> Result<Vec<u8>> {
    let (row_bytes, bpp) = row_geometry(p)?;
    let mut out = Vec::with_capacity(data.len());
    let mut prev = vec![0u8; row_bytes];
    let mut pos = 0usize;
    while pos < data.len() {
        let tag = if p.predictor == 15 {
            let t = data[pos];
            pos += 1;
            if t > 4 {
                return Err(DecodeError::new(format!(
                    "PNG predictor: bad row filter {t}"
                )));
            }
            t
        } else {
            (p.predictor - 10) as u8
        };
        let end = (pos + row_bytes).min(data.len());
        let mut row = data[pos..end].to_vec();
        pos = end;
        let cur_len = row.len();
        for i in 0..cur_len {
            let a = if i >= bpp { row[i - bpp] } else { 0 };
            let b = prev[i];
            let c = if i >= bpp { prev[i - bpp] } else { 0 };
            let add = match tag {
                0 => 0,                                 // None
                1 => a,                                 // Sub
                2 => b,                                 // Up
                3 => ((a as u16 + b as u16) / 2) as u8, // Average
                4 => paeth(a, b, c),                    // Paeth
                _ => 0,
            };
            row[i] = row[i].wrapping_add(add);
        }
        // 更新上一行基准：整行取本行，末行不足则余下补 0。
        prev = vec![0u8; row_bytes];
        prev[..cur_len].copy_from_slice(&row);
        out.extend_from_slice(&row);
    }
    Ok(out)
}

/// PNG Paeth 预测值。
fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let p = a as i32 + b as i32 - c as i32;
    let pa = (p - a as i32).abs();
    let pb = (p - b as i32).abs();
    let pc = (p - c as i32).abs();
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

/// TIFF 预测器（Predictor 2）：同一行的每个采样值加上其左侧相邻采样值
/// （同分量），按 2^BitsPerComponent 取模。要求 BitsPerComponent 为 8 的
/// 整数倍（常见 8 / 16）；其它位宽不支持，返回 Err。
fn tiff_predictor(data: &[u8], p: &Predictor) -> Result<Vec<u8>> {
    if p.bits % 8 != 0 {
        return Err(DecodeError::new(format!(
            "TIFF Predictor requires BitsPerComponent multiple of 8, got {}",
            p.bits
        )));
    }
    let (row_bytes, _) = row_geometry(p)?;
    let sample_bytes = (p.bits / 8) as usize; // 每采样字节数
    let stride = p.colors as usize * sample_bytes; // 左侧相邻采样（同分量）距离
    let mask: u32 = if p.bits >= 32 {
        u32::MAX
    } else {
        (1u32 << p.bits) - 1
    };
    let samples_per_row = p.columns as usize * p.colors as usize;

    let mut out = data.to_vec();
    let mut row_start = 0usize;
    while row_start < out.len() {
        let row_end = (row_start + row_bytes).min(out.len());
        let mut s = p.colors as usize;
        while s < samples_per_row {
            let off = row_start + s * sample_bytes;
            if off + sample_bytes > row_end {
                break; // 末行不完整
            }
            let prev_off = off - stride;
            let a = read_be(&out, prev_off, sample_bytes);
            let b = read_be(&out, off, sample_bytes);
            write_be(&mut out, off, sample_bytes, (a.wrapping_add(b)) & mask);
            s += 1;
        }
        row_start += row_bytes;
    }
    Ok(out)
}

/// 从 `bytes[off..]` 读 `n`（1..=4）字节大端整数。
fn read_be(bytes: &[u8], off: usize, n: usize) -> u32 {
    let mut v = 0u32;
    for i in 0..n {
        v = (v << 8) | u32::from(bytes[off + i]);
    }
    v
}

/// 把 `v` 的低 `n` 字节按大端写回 `bytes[off..]`。
fn write_be(bytes: &mut [u8], off: usize, n: usize, v: u32) {
    for i in 0..n {
        let shift = 8 * (n - 1 - i);
        bytes[off + i] = ((v >> shift) & 0xff) as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::Value;

    /// PNG Up（Predictor 12）：按行累加还原。
    #[test]
    fn png_up_predictor_roundtrips() {
        // 2 行 × 4 列，逐行做 Up 编码（enc[i] = raw[i] - prev[i]）
        let raw: Vec<u8> = vec![10, 20, 30, 40, /* row2 */ 5, 250, 1, 2];
        let mut enc = Vec::new();
        let rowlen = 4;
        for r in 0..2 {
            for c in 0..rowlen {
                let cur = raw[r * rowlen + c];
                let up = if r == 0 { 0 } else { raw[(r - 1) * rowlen + c] };
                enc.push(cur.wrapping_sub(up));
            }
        }
        let parms = Dict::new()
            .insert("Predictor", Value::Integer(12))
            .insert("Columns", Value::Integer(4));
        assert_eq!(apply_predictor(enc, Some(&parms)).unwrap(), raw);
    }

    /// PNG Paeth（Predictor 14）与每行过滤字节（Predictor 15）解码。
    #[test]
    fn png_paeth_and_per_row_filters() {
        // 手工按 PNG 规则编码再解码，验证可逆。
        let rowlen = 4usize;
        let bpp = 1usize;
        let rows: [Vec<u8>; 2] = [vec![1, 2, 3, 4], vec![9, 8, 7, 6]];
        // 使用 tag=4（Paeth）编码两行
        let mut enc = Vec::new();
        for (r, row) in rows.iter().enumerate() {
            let mut enc_row = row.clone();
            for i in 0..rowlen {
                let a = if i >= bpp { row[i - bpp] } else { 0 };
                let b = if r == 0 { 0 } else { rows[r - 1][i] };
                let c = if i >= bpp && r > 0 {
                    rows[r - 1][i - bpp]
                } else {
                    0
                };
                enc_row[i] = enc_row[i].wrapping_sub(paeth(a, b, c));
            }
            enc.extend_from_slice(&enc_row);
        }
        // 固定 Paeth
        let parms = Dict::new()
            .insert("Predictor", Value::Integer(14))
            .insert("Columns", Value::Integer(4));
        let decoded = apply_predictor(enc.clone(), Some(&parms)).unwrap();
        assert_eq!(&decoded, &rows.concat());

        // Predictor 15：每行前置过滤字节 4（Paeth）
        let mut enc15 = Vec::new();
        for (r, row) in rows.iter().enumerate() {
            let mut enc_row = row.clone();
            for i in 0..rowlen {
                let a = if i >= bpp { row[i - bpp] } else { 0 };
                let b = if r == 0 { 0 } else { rows[r - 1][i] };
                let c = if i >= bpp && r > 0 {
                    rows[r - 1][i - bpp]
                } else {
                    0
                };
                enc_row[i] = enc_row[i].wrapping_sub(paeth(a, b, c));
            }
            enc15.push(4);
            enc15.extend_from_slice(&enc_row);
        }
        let parms15 = Dict::new()
            .insert("Predictor", Value::Integer(15))
            .insert("Columns", Value::Integer(4));
        assert_eq!(
            apply_predictor(enc15, Some(&parms15)).unwrap(),
            rows.concat()
        );
    }

    /// TIFF（Predictor 2，8bpc）按行逐字节累加还原。
    #[test]
    fn tiff_predictor_roundtrips() {
        let raw: Vec<u8> = vec![1, 2, 3, 4, /* row2 */ 5, 6, 7, 8];
        let rowlen = 4usize;
        let mut enc = Vec::new();
        for r in 0..2 {
            for c in 0..rowlen {
                let cur = raw[r * rowlen + c];
                let left = if c == 0 { 0 } else { raw[r * rowlen + c - 1] };
                enc.push(cur.wrapping_sub(left));
            }
        }
        let parms = Dict::new()
            .insert("Predictor", Value::Integer(2))
            .insert("Columns", Value::Integer(4));
        assert_eq!(apply_predictor(enc, Some(&parms)).unwrap(), raw);
    }

    /// PNG Sub（Predictor 11，tag=1）与 Average（Predictor 13，tag=3）：
    /// 手工按规则编码再解码，验证可逆（Average 覆盖 `(left+up)/2` 向零
    /// 取整的奇数和场景）。
    #[test]
    fn png_sub_and_average_predictors() {
        let rowlen = 3usize;
        let bpp = 1usize;
        let rows: [Vec<u8>; 2] = [vec![1, 2, 3], vec![10, 25, 40]];

        // Sub：enc[i] = raw[i] - left（同行左侧已重构采样）
        let mut enc_sub = Vec::new();
        for row in &rows {
            let mut enc_row = row.clone();
            for i in 0..rowlen {
                let left = if i >= bpp { row[i - bpp] } else { 0 };
                enc_row[i] = enc_row[i].wrapping_sub(left);
            }
            enc_sub.extend_from_slice(&enc_row);
        }
        let parms_sub = Dict::new()
            .insert("Predictor", Value::Integer(11))
            .insert("Columns", Value::Integer(3));
        assert_eq!(
            apply_predictor(enc_sub, Some(&parms_sub)).unwrap(),
            rows.concat()
        );

        // Average：enc[i] = raw[i] - (left + up) / 2（u16 和后整除）
        let mut enc_avg = Vec::new();
        for (r, row) in rows.iter().enumerate() {
            let mut enc_row = row.clone();
            for i in 0..rowlen {
                let left = if i >= bpp { row[i - bpp] } else { 0 };
                let up = if r > 0 { rows[r - 1][i] } else { 0 };
                let pred = ((left as u16 + up as u16) / 2) as u8;
                enc_row[i] = enc_row[i].wrapping_sub(pred);
            }
            enc_avg.extend_from_slice(&enc_row);
        }
        let parms_avg = Dict::new()
            .insert("Predictor", Value::Integer(13))
            .insert("Columns", Value::Integer(3));
        assert_eq!(
            apply_predictor(enc_avg, Some(&parms_avg)).unwrap(),
            rows.concat()
        );
    }
}
