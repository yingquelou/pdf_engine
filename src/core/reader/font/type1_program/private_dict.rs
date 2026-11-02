//! `eexec` 解密后的 Private 字典解析：`/lenIV`、`/CharStrings` 与 `/Subrs`
//! 的 `RD`/`-|` 段定位与解密调度。

use std::collections::BTreeMap;

use super::super::FontResult;
use super::crypto::decrypt_segment;
use super::error::invalid_type1;
use super::scanner::{Scanner, Token};

pub(super) struct PrivateDict {
    pub(super) len_iv: i32,
    pub(super) charstrings: BTreeMap<String, Vec<u8>>,
    pub(super) subrs: BTreeMap<u32, Vec<u8>>,
}

pub(super) fn parse_private(data: &[u8]) -> FontResult<PrivateDict> {
    let mut scanner = Scanner::new(data);
    let mut operands: Vec<Operand> = Vec::new();
    let mut section = Section::None;
    let mut len_iv = 4i32;
    let mut expect_len_iv = false;
    let mut saw_charstrings = false;
    let mut charstrings = BTreeMap::new();
    let mut subrs = BTreeMap::new();

    while let Some(token) = scanner.next_token()? {
        match token {
            Token::Name(name) => {
                expect_len_iv = name == "lenIV";
                match name.as_str() {
                    "Subrs" => section = Section::Subrs,
                    "CharStrings" => {
                        section = Section::CharStrings;
                        saw_charstrings = true;
                    }
                    _ => {}
                }
                operands.push(Operand::Name(name));
            }
            Token::Number(value) => {
                if expect_len_iv {
                    len_iv = i32::try_from(integer(value, "/lenIV")?)
                        .map_err(|_| invalid_type1("/lenIV is out of range"))?;
                    expect_len_iv = false;
                }
                operands.push(Operand::Number(value));
            }
            Token::Keyword(keyword) => {
                expect_len_iv = false;
                match keyword.as_str() {
                    "RD" | "-|" => {
                        let (key, length) = segment_operands(&operands)?;
                        let ciphertext = scanner.read_binary(length)?;
                        let segment = decrypt_segment(ciphertext, len_iv)?;
                        match (section, key) {
                            (Section::CharStrings, SegmentKey::Name(name)) => {
                                charstrings.insert(name, segment);
                            }
                            (Section::Subrs, SegmentKey::Index(index)) => {
                                subrs.insert(index, segment);
                            }
                            (Section::CharStrings, SegmentKey::Index(_)) => {
                                return Err(invalid_type1(
                                    "CharStrings entry is missing a glyph name",
                                ));
                            }
                            (Section::Subrs, SegmentKey::Name(_)) => {
                                return Err(invalid_type1(
                                    "Subrs entry is missing a subroutine index",
                                ));
                            }
                            (Section::None, _) => {}
                        }
                    }
                    "mark" | "cleartomark" | "closefile" => break,
                    _ => {}
                }
                operands.clear();
            }
            _ => {
                expect_len_iv = false;
                operands.clear();
            }
        }
    }

    if !saw_charstrings {
        return Err(invalid_type1("Private dictionary is missing /CharStrings"));
    }
    Ok(PrivateDict {
        len_iv,
        charstrings,
        subrs,
    })
}

#[derive(Debug, Clone, Copy)]
enum Section {
    None,
    Subrs,
    CharStrings,
}

enum Operand {
    Name(String),
    Number(f64),
}

enum SegmentKey {
    Name(String),
    Index(u32),
}

fn segment_operands(operands: &[Operand]) -> FontResult<(SegmentKey, usize)> {
    let length = operands
        .last()
        .ok_or_else(|| invalid_type1("RD segment is missing a length"))?;
    let Operand::Number(length) = length else {
        return Err(invalid_type1("RD segment length is not a number"));
    };
    let length = usize::try_from(integer(*length, "RD segment length")?)
        .map_err(|_| invalid_type1("RD segment length is negative"))?;
    let key = operands
        .get(operands.len().saturating_sub(2))
        .ok_or_else(|| invalid_type1("RD segment is missing a name or index"))?;
    let key = match key {
        Operand::Name(name) => SegmentKey::Name(name.clone()),
        Operand::Number(index) => SegmentKey::Index(
            u32::try_from(integer(*index, "Subrs index")?)
                .map_err(|_| invalid_type1("Subrs index is out of range"))?,
        ),
    };
    Ok((key, length))
}

pub(super) fn integer(value: f64, what: &str) -> FontResult<i64> {
    if !value.is_finite() || value.fract() != 0.0 || value.abs() > 9_007_199_254_740_992.0 {
        return Err(invalid_type1(format!("{what} is not an integer")));
    }
    Ok(value as i64)
}
