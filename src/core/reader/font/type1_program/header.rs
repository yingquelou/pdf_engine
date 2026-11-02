//! Type 1 程序明文头（`/FontInfo`、`/FontMatrix`、`/Encoding` 等）的解析与
//! 类型化元数据模型。

use std::collections::{BTreeMap, VecDeque};

use super::super::FontResult;
use super::error::invalid_type1;
use super::scanner::{Scanner, Token};

/// The encoding declared in a Type 1 program's cleartext header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type1BuiltinEncoding {
    /// A predefined PostScript encoding referenced by name.
    Predefined(String),
    /// Explicit `dup <code> /<glyph> put` entries of a built-in array.
    ///
    /// Codes absent from the map are `.notdef` in the font program.
    Custom(BTreeMap<u8, String>),
}

/// Metadata from the cleartext PostScript header of a Type 1 font program.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Type1Header {
    /// `/FontName`, without the leading slash.
    pub font_name: Option<String>,
    /// `/FullName` from `/FontInfo`.
    pub full_name: Option<String>,
    /// `/FamilyName` from `/FontInfo`.
    pub family_name: Option<String>,
    /// `/Weight` from `/FontInfo`.
    pub weight: Option<String>,
    /// `/ItalicAngle`, in degrees.
    pub italic_angle: Option<f64>,
    /// `/isFixedPitch`.
    pub is_fixed_pitch: Option<bool>,
    /// `/UnderlinePosition`.
    pub underline_position: Option<f64>,
    /// `/UnderlineThickness`.
    pub underline_thickness: Option<f64>,
    /// `/FontBBox` as `[llx lly urx ury]`.
    pub font_bbox: Option<[f64; 4]>,
    /// `/FontMatrix` as six values.
    pub font_matrix: Option<[f64; 6]>,
    /// The font program's built-in `/Encoding`.
    pub encoding: Option<Type1BuiltinEncoding>,
}

pub(super) fn parse_header(scanner: &mut Scanner<'_>) -> FontResult<Type1Header> {
    let mut header = Type1Header::default();
    let mut encoding_array = false;
    let mut window: VecDeque<WindowToken> = VecDeque::with_capacity(4);
    loop {
        let Some(token) = scanner.next_token()? else {
            return Err(invalid_type1("missing `eexec` section"));
        };
        if matches!(&token, Token::Keyword(keyword) if keyword == "eexec") {
            return Ok(header);
        }
        if let Token::Name(name) = &token {
            match name.as_str() {
                "FontName" => header.font_name = name_value(scanner.next_token()?),
                "FullName" => header.full_name = text_value(scanner.next_token()?),
                "FamilyName" => header.family_name = text_value(scanner.next_token()?),
                "Weight" => header.weight = text_value(scanner.next_token()?),
                "ItalicAngle" => header.italic_angle = number_value(scanner.next_token()?),
                "isFixedPitch" => header.is_fixed_pitch = boolean_value(scanner.next_token()?),
                "UnderlinePosition" => {
                    header.underline_position = number_value(scanner.next_token()?)
                }
                "UnderlineThickness" => {
                    header.underline_thickness = number_value(scanner.next_token()?)
                }
                "FontBBox" => header.font_bbox = number_array(scanner, 4)?.and_then(array),
                "FontMatrix" => header.font_matrix = number_array(scanner, 6)?.and_then(array),
                "Encoding" => match scanner.next_token()? {
                    // 预定义编码在 PostScript 中以裸标识符引用（如
                    // `StandardEncoding`），不带前导斜杠。
                    Some(Token::Name(name)) | Some(Token::Keyword(name)) => {
                        header.encoding = Some(Type1BuiltinEncoding::Predefined(name));
                    }
                    Some(Token::Number(_)) => {
                        encoding_array = true;
                        header.encoding = Some(Type1BuiltinEncoding::Custom(BTreeMap::new()));
                    }
                    _ => {}
                },
                _ => {}
            }
        }
        if encoding_array {
            window.push_back(WindowToken::from(&token));
            if window.len() > 4 {
                window.pop_front();
            }
            if let Some((code, glyph)) = encoding_entry(&window)
                && let Some(Type1BuiltinEncoding::Custom(entries)) = &mut header.encoding
            {
                entries.insert(code, glyph);
            }
        }
    }
}

fn name_value(token: Option<Token>) -> Option<String> {
    match token {
        Some(Token::Name(name)) => Some(name),
        _ => None,
    }
}

fn text_value(token: Option<Token>) -> Option<String> {
    match token {
        Some(Token::Name(name)) => Some(name),
        Some(Token::String(bytes)) | Some(Token::HexString(bytes)) => {
            Some(String::from_utf8_lossy(&bytes).into_owned())
        }
        _ => None,
    }
}

fn number_value(token: Option<Token>) -> Option<f64> {
    match token {
        Some(Token::Number(value)) => Some(value),
        _ => None,
    }
}

fn boolean_value(token: Option<Token>) -> Option<bool> {
    match token {
        Some(Token::Keyword(keyword)) if keyword == "true" => Some(true),
        Some(Token::Keyword(keyword)) if keyword == "false" => Some(false),
        _ => None,
    }
}

fn number_array(scanner: &mut Scanner<'_>, expected: usize) -> FontResult<Option<Vec<f64>>> {
    let mut numbers = Vec::with_capacity(expected);
    match scanner.next_token()? {
        Some(Token::ArrayStart) | Some(Token::ProcStart) => loop {
            match scanner.next_token()? {
                Some(Token::Number(value)) => numbers.push(value),
                Some(Token::ArrayEnd) | Some(Token::ProcEnd) | None => break,
                Some(_) => {}
            }
        },
        Some(Token::Number(value)) => {
            numbers.push(value);
            while numbers.len() < expected {
                match scanner.next_token()? {
                    Some(Token::Number(value)) => numbers.push(value),
                    _ => break,
                }
            }
        }
        _ => {}
    }
    Ok((numbers.len() >= expected).then(|| numbers[..expected].to_vec()))
}

fn array<const N: usize>(values: Vec<f64>) -> Option<[f64; N]> {
    values.try_into().ok()
}

enum WindowToken {
    Number(f64),
    Name(String),
    Keyword(String),
    Other,
}

impl From<&Token> for WindowToken {
    fn from(token: &Token) -> Self {
        match token {
            Token::Number(value) => Self::Number(*value),
            Token::Name(name) => Self::Name(name.clone()),
            Token::Keyword(keyword) => Self::Keyword(keyword.clone()),
            _ => Self::Other,
        }
    }
}

fn encoding_entry(window: &VecDeque<WindowToken>) -> Option<(u8, String)> {
    let tokens: Vec<&WindowToken> = window.iter().collect();
    match tokens.as_slice() {
        [
            WindowToken::Keyword(dup),
            WindowToken::Number(code),
            WindowToken::Name(glyph),
            WindowToken::Keyword(put),
        ] if dup == "dup" && put == "put" => {
            if code.fract() != 0.0 || !(0.0..=255.0).contains(code) {
                return None;
            }
            Some((*code as u8, glyph.clone()))
        }
        _ => None,
    }
}
