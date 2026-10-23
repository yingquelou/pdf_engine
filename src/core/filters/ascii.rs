//! `ASCIIHexDecode` / `ASCII85Decode` / `RunLengthDecode` 与字符辅助函数。
//!
//! 从 `filters` 根模块拆出，保持每个过滤器实现可独立阅读与测试。

use crate::core::types::Dict;

use super::{DecodeError, Result, StreamFilter};

/// `ASCIIHexDecode`。
pub(super) struct AsciiHexFilter;

impl StreamFilter for AsciiHexFilter {
    fn name(&self) -> &'static str {
        "ASCIIHexDecode"
    }

    fn decode(&self, data: &[u8], _params: Option<&Dict>) -> Result<Vec<u8>> {
        ascii_hex_decode(data)
    }
}

/// `ASCII85Decode`。
pub(super) struct Ascii85Filter;

impl StreamFilter for Ascii85Filter {
    fn name(&self) -> &'static str {
        "ASCII85Decode"
    }

    fn decode(&self, data: &[u8], _params: Option<&Dict>) -> Result<Vec<u8>> {
        ascii85_decode(data)
    }
}

/// `RunLengthDecode`。
pub(super) struct RunLengthFilter;

impl StreamFilter for RunLengthFilter {
    fn name(&self) -> &'static str {
        "RunLengthDecode"
    }

    fn decode(&self, data: &[u8], _params: Option<&Dict>) -> Result<Vec<u8>> {
        run_length_decode(data)
    }
}

fn ascii_hex_decode(data: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(data.len() / 2);
    let mut hi: Option<u8> = None;
    for &b in data {
        if b == b'>' {
            break;
        }
        if is_whitespace(b) {
            continue;
        }
        let v = hex_value(b).ok_or_else(|| {
            DecodeError::new(format!("ASCIIHexDecode: invalid character 0x{b:02x}"))
        })?;
        match hi {
            None => hi = Some(v),
            Some(h) => {
                out.push((h << 4) | v);
                hi = None;
            }
        }
    }
    if let Some(h) = hi {
        out.push(h << 4);
    }
    Ok(out)
}

/// `ASCII85Decode`：`!`–`u` 五字符一组还原 4 字节；`z` 表示 4 个零字节；
/// 空白忽略；遇 `~>` 结束。可含可选的 `<~` 前缀。末组不足 5 字符时按
/// 规范以 `u`（84）补齐、并只输出有效字节。
fn ascii85_decode(data: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(data.len() * 4 / 5);
    let mut group = [0u32; 5];
    let mut n = 0usize;
    let mut i = 0usize;
    // 跳过可选的 `<~` 前缀
    if data.starts_with(b"<~") {
        i = 2;
    }
    while i < data.len() {
        let b = data[i];
        i += 1;
        if is_whitespace(b) {
            continue;
        }
        if b == b'~' {
            // 结束标记 `~>`（单独 `~` 亦容忍为终止）
            break;
        }
        if b == b'z' {
            if n != 0 {
                return Err(DecodeError::new(
                    "ASCII85Decode: 'z' inside a partial group",
                ));
            }
            out.extend_from_slice(&[0, 0, 0, 0]);
            continue;
        }
        if !(b'!'..=b'u').contains(&b) {
            return Err(DecodeError::new(format!(
                "ASCII85Decode: invalid character 0x{b:02x}"
            )));
        }
        group[n] = (b - b'!') as u32;
        n += 1;
        if n == 5 {
            emit_ascii85_group(&group, 4, &mut out)?;
            n = 0;
        }
    }
    if n > 0 {
        // 末组：以 84（'u'）补齐到 5 个字符，输出 n-1 字节。
        if n == 1 {
            return Err(DecodeError::new(
                "ASCII85Decode: final group has a single character",
            ));
        }
        for slot in group.iter_mut().skip(n) {
            *slot = 84;
        }
        emit_ascii85_group(&group, n - 1, &mut out)?;
    }
    Ok(out)
}

/// 一组 5 个 base85 数字 → 至多 `keep` 个字节（大端）。数值超出 u32 报错。
fn emit_ascii85_group(group: &[u32; 5], keep: usize, out: &mut Vec<u8>) -> Result<()> {
    let mut v: u64 = 0;
    for &d in group {
        v = v * 85 + d as u64;
    }
    if v > u32::MAX as u64 {
        return Err(DecodeError::new(
            "ASCII85Decode: group value overflows 32 bits",
        ));
    }
    let bytes = (v as u32).to_be_bytes();
    out.extend_from_slice(&bytes[..keep]);
    Ok(())
}

/// `RunLengthDecode`：长度字节 `l`——`0..=127` 后随 `l+1` 个原样字节；
/// `129..=255` 后随 1 字节并重复 `257-l` 次；`128` 为 EOD。
fn run_length_decode(data: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < data.len() {
        let l = data[i];
        i += 1;
        match l {
            128 => break, // EOD
            0..=127 => {
                let n = l as usize + 1;
                if i + n > data.len() {
                    return Err(DecodeError::new(
                        "RunLengthDecode: literal run exceeds data length",
                    ));
                }
                out.extend_from_slice(&data[i..i + n]);
                i += n;
            }
            _ => {
                let n = 257 - l as usize;
                let b = *data
                    .get(i)
                    .ok_or_else(|| DecodeError::new("RunLengthDecode: missing repeat byte"))?;
                i += 1;
                out.extend(std::iter::repeat_n(b, n));
            }
        }
    }
    Ok(out)
}

fn is_whitespace(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n' | 0x0c | 0x00)
}

fn hex_value(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_hex_ignores_whitespace_and_pads_odd() {
        assert_eq!(ascii_hex_decode(b"48 65 6C 6C 6F>").unwrap(), b"Hello");
        // 奇数个数字：末位补 0
        assert_eq!(ascii_hex_decode(b"7>").unwrap(), vec![0x70]);
    }

    #[test]
    fn ascii85_decodes_basic_and_z_shorthand() {
        // "Hello" 的 ASCII85：87cURDZ？用标准 "Man " 示例：
        // "Man " → 9jqo^
        assert_eq!(ascii85_decode(b"9jqo^~>").unwrap(), b"Man ");
        // z 短路 = 4 个零字节
        assert_eq!(ascii85_decode(b"z~>").unwrap(), vec![0, 0, 0, 0]);
        // 带 <~ 前缀
        assert_eq!(ascii85_decode(b"<~9jqo^~>").unwrap(), b"Man ");
    }

    #[test]
    fn runlength_decodes_literal_and_repeat() {
        // 0x00 → 1 个字面字节 'A'；0xFE(254) → 重复 3 次 'B'；0x80 EOD
        let data = [0x00u8, b'A', 0xFE, b'B', 0x80];
        assert_eq!(run_length_decode(&data).unwrap(), b"ABBB");
    }
}
