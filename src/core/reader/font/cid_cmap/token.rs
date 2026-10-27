//! CMap/ToUnicode 共用的 PostScript 词法与段辅助。

use super::{FontResult, MAX_CMAP_ENTRIES, MAX_UNICODE_BYTES, invalid_cmap};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::core::reader::font) enum Token {
    Integer(i64),
    Hex(Vec<u8>),
    Word(String),
}

pub(in crate::core::reader::font) fn tokenize(data: &[u8]) -> FontResult<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < data.len() {
        match data[index] {
            byte if is_whitespace(byte) => index += 1,
            b'%' => {
                while index < data.len() && data[index] != b'\n' && data[index] != b'\r' {
                    index += 1;
                }
            }
            b'<' if data.get(index + 1) == Some(&b'<') => {
                tokens.push(Token::Word("<<".into()));
                index += 2;
            }
            b'>' if data.get(index + 1) == Some(&b'>') => {
                tokens.push(Token::Word(">>".into()));
                index += 2;
            }
            b'<' => {
                index += 1;
                let mut digits = Vec::new();
                loop {
                    let Some(&byte) = data.get(index) else {
                        return Err(invalid_cmap("unterminated hex string"));
                    };
                    index += 1;
                    if byte == b'>' {
                        break;
                    }
                    if is_whitespace(byte) {
                        continue;
                    }
                    if !byte.is_ascii_hexdigit() {
                        return Err(invalid_cmap("invalid hex string"));
                    }
                    digits.push(byte);
                    if digits.len() > MAX_UNICODE_BYTES * 2 {
                        return Err(invalid_cmap("hex string exceeds the supported limit"));
                    }
                }
                if digits.len() % 2 != 0 {
                    digits.push(b'0');
                }
                let bytes = digits
                    .chunks(2)
                    .map(|pair| {
                        let high = hex_value(pair[0]);
                        let low = hex_value(pair[1]);
                        (high << 4) | low
                    })
                    .collect();
                tokens.push(Token::Hex(bytes));
            }
            b'(' => {
                index += 1;
                let mut depth = 1usize;
                while index < data.len() && depth > 0 {
                    match data[index] {
                        b'\\' => index = (index + 2).min(data.len()),
                        b'(' => {
                            depth += 1;
                            index += 1;
                        }
                        b')' => {
                            depth -= 1;
                            index += 1;
                        }
                        _ => index += 1,
                    }
                }
                if depth != 0 {
                    return Err(invalid_cmap("unterminated literal string"));
                }
            }
            byte if is_delimiter(byte) => {
                tokens.push(Token::Word(char::from(byte).to_string()));
                index += 1;
            }
            _ => {
                let start = index;
                while index < data.len()
                    && !is_whitespace(data[index])
                    && !is_delimiter(data[index])
                {
                    index += 1;
                }
                let word = std::str::from_utf8(&data[start..index])
                    .map_err(|_| invalid_cmap("CMap token is not ASCII"))?;
                if let Ok(integer) = word.parse::<i64>() {
                    tokens.push(Token::Integer(integer));
                } else {
                    tokens.push(Token::Word(word.into()));
                }
            }
        }
        if tokens.len() > MAX_CMAP_ENTRIES * 4 {
            return Err(invalid_cmap("CMap contains too many tokens"));
        }
    }
    Ok(tokens)
}

pub(in crate::core::reader::font) fn section_count(
    tokens: &[Token],
    operator_index: usize,
) -> FontResult<usize> {
    let Some(Token::Integer(count)) = operator_index
        .checked_sub(1)
        .and_then(|index| tokens.get(index))
    else {
        return Err(invalid_cmap("CMap section has no entry count"));
    };
    let count =
        usize::try_from(*count).map_err(|_| invalid_cmap("negative section entry count"))?;
    if count > MAX_CMAP_ENTRIES {
        return Err(invalid_cmap("CMap section contains too many entries"));
    }
    Ok(count)
}

pub(in crate::core::reader::font) fn section_end(
    tokens: &[Token],
    start: usize,
    count: usize,
    width: usize,
    end_marker: &str,
) -> FontResult<usize> {
    let entries_length = count
        .checked_mul(width)
        .and_then(|length| start.checked_add(1 + length))
        .ok_or_else(|| invalid_cmap("CMap section length overflows"))?;
    if !matches!(tokens.get(entries_length), Some(Token::Word(marker)) if marker == end_marker) {
        return Err(invalid_cmap("CMap section is truncated or malformed"));
    }
    Ok(entries_length)
}

pub(in crate::core::reader::font) fn source_code(token: &Token) -> FontResult<(usize, u32)> {
    let Token::Hex(bytes) = token else {
        return Err(invalid_cmap("CMap source code is not a hex string"));
    };
    if bytes.is_empty() || bytes.len() > 4 {
        return Err(invalid_cmap("CMap source code must contain 1 to 4 bytes"));
    }
    Ok((bytes.len(), bytes_to_code(bytes)))
}

pub(in crate::core::reader::font) fn cid_number(token: &Token) -> FontResult<u16> {
    match token {
        Token::Integer(cid) => {
            u16::try_from(*cid).map_err(|_| invalid_cmap("CID is outside 0..=65535"))
        }
        _ => Err(invalid_cmap("CID mapping target is not an integer")),
    }
}

pub(in crate::core::reader::font) fn bytes_to_code(bytes: &[u8]) -> u32 {
    bytes
        .iter()
        .fold(0u32, |value, byte| (value << 8) | u32::from(*byte))
}

fn is_whitespace(byte: u8) -> bool {
    matches!(byte, 0 | 9 | 10 | 12 | 13 | 32)
}

fn is_delimiter(byte: u8) -> bool {
    matches!(
        byte,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}

fn hex_value(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        b'A'..=b'F' => byte - b'A' + 10,
        _ => unreachable!(),
    }
}
