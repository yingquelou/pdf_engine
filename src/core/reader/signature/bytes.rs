//! PDF 字符串值的字节解码（FieldMDP `/Annots`、签名 `/Contents`）。

use crate::core::types::{Dict, Value};

/// 位串字符串（字面/十六进制/ASCII85）解码为字节。
///
/// 十六进制串按高/低半字节解析，奇数长度按 PDF 惯例尾部补 `0`；
/// 其他值类型返回 `None`。
pub(super) fn pdf_string_bytes(item: &Value) -> Option<Vec<u8>> {
    match item {
        Value::String(text) => Some(text.as_bytes().to_vec()),
        Value::HexString(hex) => {
            let mut padded = hex.clone();
            if padded.len() % 2 == 1 {
                padded.push('0');
            }
            (0..padded.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&padded[i..i + 2], 16).ok())
                .collect()
        }
        Value::Ascii85String(raw) => {
            let dict = Dict::new().insert("Filter", Value::Name("ASCII85Decode".into()));
            crate::core::filters::decode_stream(&dict, raw.as_bytes()).ok()
        }
        _ => None,
    }
}
