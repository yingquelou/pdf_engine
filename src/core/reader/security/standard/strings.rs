//! 加密字典 PDF 字面串与十六进制串的编解码辅助（ literal/hex string
//! ↔ bytes），供认证解析与解密路径共用。

use crate::core::types::Value;

pub(super) fn pdf_string(value: &Value) -> Result<Vec<u8>, String> {
    match value {
        Value::String(value) => Ok(value.chars().map(|c| c as u8).collect()),
        Value::HexString(value) => {
            let mut digits = value.bytes().filter(|b| !b.is_ascii_whitespace());
            let mut bytes = Vec::with_capacity(value.len().div_ceil(2));
            while let Some(high) = digits.next() {
                let high = hex_digit(high)?;
                let low = digits.next().map(hex_digit).transpose()?.unwrap_or(0);
                bytes.push((high << 4) | low);
            }
            Ok(bytes)
        }
        _ => Err("encryption values must be PDF strings".to_string()),
    }
}

pub(super) fn decode_hex(hex: &str) -> Result<Vec<u8>, String> {
    let digits: Vec<u8> = hex.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    let mut output = Vec::with_capacity(digits.len().div_ceil(2));
    for pair in digits.chunks(2) {
        let high = hex_digit(pair[0])?;
        let low = pair.get(1).map(|b| hex_digit(*b)).transpose()?.unwrap_or(0);
        output.push((high << 4) | low);
    }
    Ok(output)
}

pub(super) fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02X}")).collect()
}

fn hex_digit(digit: u8) -> Result<u8, String> {
    match digit {
        b'0'..=b'9' => Ok(digit - b'0'),
        b'a'..=b'f' => Ok(digit - b'a' + 10),
        b'A'..=b'F' => Ok(digit - b'A' + 10),
        _ => Err("invalid hexadecimal PDF string".to_string()),
    }
}
