//! PostScript Type 1 font program (`/FontFile`) parsing.

use std::collections::{BTreeMap, VecDeque};

use crate::core::types::{Document, Value};

use super::{FontError, FontResult, embedded_program::FontProgramSource};

mod charstring;
mod subset;

pub use charstring::{Type1CharString, Type1PathSegment, Type1Seac};
pub use subset::subset_type1;

use charstring::interpret;

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

/// A parsed Type 1 font program.
///
/// `/CharStrings` and `/Subrs` are decrypted according to the Private
/// dictionary's `/lenIV`. [`Type1FontProgram::glyph`] interprets an individual
/// charstring into widths and absolute path segments.
#[derive(Debug, Clone, PartialEq)]
pub struct Type1FontProgram {
    /// Metric metadata from the cleartext header.
    pub header: Type1Header,
    /// The Private dictionary's `/lenIV`; `-1` means charstrings are not
    /// encrypted.
    pub len_iv: i32,
    /// Decrypted charstring bytes keyed by glyph name.
    pub charstrings: BTreeMap<String, Vec<u8>>,
    /// Decrypted subroutine bytes keyed by subroutine index.
    pub subrs: BTreeMap<u32, Vec<u8>>,
}

impl Type1FontProgram {
    /// Parse a decoded Type 1 font program.
    ///
    /// This understands the cleartext header, the `eexec`-encrypted Private
    /// dictionary, and the encrypted `/CharStrings` and `/Subrs` string
    /// segments. It does not execute PostScript or parse CFF and OpenType data
    /// (which use `/FontFile3`).
    pub fn parse(data: &[u8]) -> FontResult<Self> {
        let mut scanner = Scanner::new(data);
        let header = parse_header(&mut scanner)?;
        scanner.skip_single_gap();
        let encrypted = eexec_section(&data[scanner.position..]);
        if encrypted.len() < 4 {
            return Err(invalid_type1("eexec section is truncated"));
        }
        let plaintext = decrypt(encrypted, 55665, 4);
        let private = parse_private(&plaintext)?;
        Ok(Self {
            header,
            len_iv: private.len_iv,
            charstrings: private.charstrings,
            subrs: private.subrs,
        })
    }

    /// Interpret a glyph charstring into its width, side bearing, and path.
    ///
    /// Returns `Ok(None)` when the program has no charstring for the glyph.
    /// `seac` composites are flattened into their base and accent outlines by
    /// resolving the component character codes through StandardEncoding, and
    /// flex `callothersubr`/`pop` sequences are flattened into curve segments.
    /// Hint replacement and other `callothersubr` procedures are rejected.
    pub fn glyph(&self, glyph_name: &str) -> FontResult<Option<Type1CharString>> {
        let Some(charstring) = self.charstrings.get(glyph_name) else {
            return Ok(None);
        };
        interpret(charstring, &self.charstrings, &self.subrs, glyph_name).map(Some)
    }
}

impl Document {
    /// Resolve and parse the embedded Type 1 font program in a font dictionary.
    ///
    /// Returns `Ok(None)` when the font has no embedded program. The program
    /// must be stored as `/FontFile`: `/FontFile3` streams (Type 1C CFF or
    /// OpenType) are not Type 1 programs and are reported as errors.
    pub fn type1_program(&self, value: &Value) -> FontResult<Option<Type1FontProgram>> {
        let Some(program) = self.type1_font_program(value)? else {
            return Ok(None);
        };
        if program.source != FontProgramSource::FontFile {
            return Err(FontError::Invalid(
                "embedded Type 1 font programs must use /FontFile; /FontFile3 is not parsed as Type 1"
                    .to_string(),
            ));
        }
        Type1FontProgram::parse(&program.data).map(Some)
    }
}

fn invalid_type1(message: impl Into<String>) -> FontError {
    FontError::Invalid(format!("invalid Type 1 font program: {}", message.into()))
}

struct PrivateDict {
    len_iv: i32,
    charstrings: BTreeMap<String, Vec<u8>>,
    subrs: BTreeMap<u32, Vec<u8>>,
}

fn parse_header(scanner: &mut Scanner<'_>) -> FontResult<Type1Header> {
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

fn parse_private(data: &[u8]) -> FontResult<PrivateDict> {
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

fn integer(value: f64, what: &str) -> FontResult<i64> {
    if !value.is_finite() || value.fract() != 0.0 || value.abs() > 9_007_199_254_740_992.0 {
        return Err(invalid_type1(format!("{what} is not an integer")));
    }
    Ok(value as i64)
}

fn decrypt_segment(ciphertext: &[u8], len_iv: i32) -> FontResult<Vec<u8>> {
    if len_iv < 0 {
        return Ok(ciphertext.to_vec());
    }
    let skip = usize::try_from(len_iv).expect("non-negative /lenIV");
    if ciphertext.len() < skip {
        return Err(invalid_type1("CharString or Subr is shorter than /lenIV"));
    }
    Ok(decrypt(ciphertext, 4330, skip))
}

/// Type 1 encryption: the high byte of the running key XORs the plaintext.
fn decrypt(ciphertext: &[u8], key: u16, skip: usize) -> Vec<u8> {
    let mut running = key;
    let mut plaintext = Vec::with_capacity(ciphertext.len().saturating_sub(skip));
    for (index, &cipher) in ciphertext.iter().enumerate() {
        let plain = cipher ^ (running >> 8) as u8;
        running = u16::from(cipher)
            .wrapping_add(running)
            .wrapping_mul(52845)
            .wrapping_add(22719);
        if index >= skip {
            plaintext.push(plain);
        }
    }
    plaintext
}

/// Split a Type 1 program into the `/FontFile` length entries required by
/// Table 127: clear-text header, `eexec` encrypted section, and trailing
/// fixed content. Programs without a locatable `eexec` section are reported
/// entirely as clear text.
pub(crate) fn font_file_lengths(data: &[u8]) -> (usize, usize, usize) {
    let mut scanner = Scanner::new(data);
    if parse_header(&mut scanner).is_err() {
        return (data.len(), 0, 0);
    }
    scanner.skip_single_gap();
    let length1 = scanner.position.min(data.len());
    let length2 = eexec_section(&data[length1..]).len();
    let length3 = data.len() - length1 - length2;
    (length1, length2, length3)
}

/// The eexec data ends at the 512-zero guard that precedes `cleartomark`.
fn eexec_section(data: &[u8]) -> &[u8] {
    let mut run = 0usize;
    for (index, &byte) in data.iter().enumerate() {
        if byte == b'0' {
            run += 1;
            if run == 512 {
                return &data[..index + 1 - 512];
            }
        } else {
            run = 0;
        }
    }
    data
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

enum Token {
    Name(String),
    Number(f64),
    Keyword(String),
    String(Vec<u8>),
    HexString(Vec<u8>),
    ArrayStart,
    ArrayEnd,
    ProcStart,
    ProcEnd,
    DictStart,
    DictEnd,
}

struct Scanner<'a> {
    data: &'a [u8],
    position: usize,
}

impl<'a> Scanner<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, position: 0 }
    }

    fn next_token(&mut self) -> FontResult<Option<Token>> {
        self.skip_ignorable();
        let Some(&byte) = self.data.get(self.position) else {
            return Ok(None);
        };
        let token = match byte {
            b'/' => {
                self.position += 1;
                Token::Name(self.read_word())
            }
            b'(' => Token::String(self.read_string()?),
            b'<' => {
                if self.data.get(self.position + 1) == Some(&b'<') {
                    self.position += 2;
                    Token::DictStart
                } else {
                    Token::HexString(self.read_hex_string()?)
                }
            }
            b'>' => {
                if self.data.get(self.position + 1) == Some(&b'>') {
                    self.position += 2;
                    Token::DictEnd
                } else {
                    return Err(invalid_type1("unexpected `>` in PostScript data"));
                }
            }
            b'[' => {
                self.position += 1;
                Token::ArrayStart
            }
            b']' => {
                self.position += 1;
                Token::ArrayEnd
            }
            b'{' => {
                self.position += 1;
                Token::ProcStart
            }
            b'}' => {
                self.position += 1;
                Token::ProcEnd
            }
            b')' => return Err(invalid_type1("unexpected `)` in PostScript data")),
            _ => {
                let word = self.read_word();
                match classify_number(&word) {
                    Some(value) => Token::Number(value),
                    None => Token::Keyword(word),
                }
            }
        };
        Ok(Some(token))
    }

    /// Skip the single whitespace byte that separates an `eexec` keyword from
    /// its encrypted data.
    fn skip_single_gap(&mut self) {
        match self.data.get(self.position) {
            Some(b'\r') if self.data.get(self.position + 1) == Some(&b'\n') => {
                self.position += 2;
            }
            Some(&byte) if is_whitespace(byte) => self.position += 1,
            _ => {}
        }
    }

    /// Read an `RD`/`-|` binary segment, whose leading separator is optional.
    fn read_binary(&mut self, length: usize) -> FontResult<&'a [u8]> {
        self.skip_single_gap();
        let end = self
            .position
            .checked_add(length)
            .ok_or_else(|| invalid_type1("RD segment length overflows"))?;
        if end > self.data.len() {
            return Err(invalid_type1("RD segment is truncated"));
        }
        let bytes = &self.data[self.position..end];
        self.position = end;
        Ok(bytes)
    }

    fn read_word(&mut self) -> String {
        let start = self.position;
        while let Some(&byte) = self.data.get(self.position) {
            if is_whitespace(byte) || is_delimiter(byte) {
                break;
            }
            self.position += 1;
        }
        String::from_utf8_lossy(&self.data[start..self.position]).into_owned()
    }

    fn read_string(&mut self) -> FontResult<Vec<u8>> {
        self.position += 1;
        let mut output = Vec::new();
        let mut depth = 1usize;
        while let Some(&byte) = self.data.get(self.position) {
            self.position += 1;
            match byte {
                b'\\' => {
                    let Some(&escaped) = self.data.get(self.position) else {
                        return Err(invalid_type1("unterminated string in PostScript data"));
                    };
                    self.position += 1;
                    match escaped {
                        b'n' => output.push(b'\n'),
                        b'r' => output.push(b'\r'),
                        b't' => output.push(b'\t'),
                        b'b' => output.push(8),
                        b'f' => output.push(12),
                        b'(' | b')' | b'\\' => output.push(escaped),
                        b'\r' => {
                            if self.data.get(self.position) == Some(&b'\n') {
                                self.position += 1;
                            }
                        }
                        b'\n' => {}
                        b'0'..=b'7' => {
                            let mut value = u16::from(escaped - b'0');
                            for _ in 0..2 {
                                match self.data.get(self.position) {
                                    Some(&digit @ b'0'..=b'7') => {
                                        value = value * 8 + u16::from(digit - b'0');
                                        self.position += 1;
                                    }
                                    _ => break,
                                }
                            }
                            output.push(value as u8);
                        }
                        _ => output.push(escaped),
                    }
                }
                b'(' => {
                    depth += 1;
                    output.push(byte);
                }
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(output);
                    }
                    output.push(byte);
                }
                _ => output.push(byte),
            }
        }
        Err(invalid_type1("unterminated string in PostScript data"))
    }

    fn read_hex_string(&mut self) -> FontResult<Vec<u8>> {
        self.position += 1;
        let mut digits = Vec::new();
        loop {
            let Some(&byte) = self.data.get(self.position) else {
                return Err(invalid_type1(
                    "unterminated hexadecimal string in PostScript data",
                ));
            };
            self.position += 1;
            match byte {
                b'>' => break,
                byte if is_whitespace(byte) => {}
                byte if byte.is_ascii_hexdigit() => digits.push(byte),
                _ => {
                    return Err(invalid_type1(
                        "invalid byte in hexadecimal string in PostScript data",
                    ));
                }
            }
        }
        let mut output = Vec::with_capacity(digits.len().div_ceil(2));
        for pair in digits.chunks(2) {
            let high = hex_digit(pair[0]);
            let low = pair.get(1).map_or(0, |&byte| hex_digit(byte));
            output.push(high << 4 | low);
        }
        Ok(output)
    }

    fn skip_ignorable(&mut self) {
        while let Some(&byte) = self.data.get(self.position) {
            if is_whitespace(byte) {
                self.position += 1;
            } else if byte == b'%' {
                self.position += 1;
                while let Some(&byte) = self.data.get(self.position) {
                    self.position += 1;
                    if byte == b'\r' || byte == b'\n' {
                        break;
                    }
                }
            } else {
                break;
            }
        }
    }
}

fn classify_number(word: &str) -> Option<f64> {
    let value: f64 = word.parse().ok()?;
    value.is_finite().then_some(value)
}

fn hex_digit(byte: u8) -> u8 {
    match byte {
        b'0'..=b'9' => byte - b'0',
        b'a'..=b'f' => byte - b'a' + 10,
        b'A'..=b'F' => byte - b'A' + 10,
        _ => 0,
    }
}

fn is_whitespace(byte: u8) -> bool {
    matches!(byte, 0x00 | b'\t' | b'\n' | 0x0c | b'\r' | b' ')
}

fn is_delimiter(byte: u8) -> bool {
    matches!(
        byte,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encrypt(plaintext: &[u8], key: u16) -> Vec<u8> {
        let mut running = key;
        let mut ciphertext = Vec::with_capacity(plaintext.len());
        for &plain in plaintext {
            let cipher = plain ^ (running >> 8) as u8;
            running = u16::from(cipher)
                .wrapping_add(running)
                .wrapping_mul(52845)
                .wrapping_add(22719);
            ciphertext.push(cipher);
        }
        ciphertext
    }

    fn segment(charstring: &[u8], len_iv: i32) -> Vec<u8> {
        if len_iv < 0 {
            return charstring.to_vec();
        }
        let mut padded = vec![0x41u8; len_iv as usize];
        padded.extend_from_slice(charstring);
        encrypt(&padded, 4330)
    }

    fn push_segment(
        output: &mut Vec<u8>,
        prefix: &str,
        charstring: &[u8],
        len_iv: i32,
        suffix: &str,
    ) {
        let data = segment(charstring, len_iv);
        output.extend_from_slice(format!("{prefix}{} RD ", data.len()).as_bytes());
        output.extend_from_slice(&data);
        output.extend_from_slice(suffix.as_bytes());
    }

    /// Build a complete Type 1 program with a two-subr Private dictionary and
    /// three charstrings. `len_iv` of `None` omits `/lenIV` (default 4).
    fn sample_program(len_iv: Option<i32>) -> Vec<u8> {
        let value = len_iv.unwrap_or(4);
        let mut private = Vec::new();
        private.extend_from_slice(b"dup /Private 8 dict dup begin\n");
        private
            .extend_from_slice(b"/RD {string currentfile exch readstring pop} executeonly def\n");
        private.extend_from_slice(b"/ND {noaccess def} executeonly def\n");
        private.extend_from_slice(b"/NP {noaccess put} executeonly def\n");
        private.extend_from_slice(b"/password 5839 def\n");
        if let Some(len_iv) = len_iv {
            private.extend_from_slice(format!("/lenIV {len_iv} def\n").as_bytes());
        }
        private.extend_from_slice(b"/Subrs 2 array\n");
        push_segment(&mut private, "dup 0 ", b"\x8b\x0c", value, " NP\n");
        push_segment(&mut private, "dup 1 ", b"\x8b\x0e\x0c", value, " NP\n");
        private.extend_from_slice(b"def\n");
        private.extend_from_slice(b"2 index /CharStrings 3 dict dup begin\n");
        push_segment(&mut private, "/.notdef ", b"\x8b\x8b\x0c", value, " ND\n");
        push_segment(&mut private, "/A ", b"\x8b\x0e\x0c", value, " ND\n");
        push_segment(&mut private, "/B ", b"\x8b\x10\x0c", value, " ND\n");
        private.extend_from_slice(b"end\nend\nreadonly put\nnoaccess put\n");
        private.extend_from_slice(b"dup /FontName get exch definefont pop\n");
        private.extend_from_slice(b"mark currentfile closefile\n");

        let mut eexec_plain = vec![0x5Au8; 4];
        eexec_plain.extend_from_slice(&private);
        let eexec = encrypt(&eexec_plain, 55665);

        let mut program = Vec::new();
        program.extend_from_slice(b"%!PS-AdobeFont-1.0: ExampleSans 001.000\n");
        program.extend_from_slice(b"11 dict begin\n");
        program.extend_from_slice(b"/FontInfo 10 dict dup begin\n");
        program.extend_from_slice(b"/FamilyName (Example Sans) def\n");
        program.extend_from_slice(b"/FullName (Example Sans Regular) def\n");
        program.extend_from_slice(b"/Weight (Regular) def\n");
        program.extend_from_slice(b"/ItalicAngle -12.5 def\n");
        program.extend_from_slice(b"/isFixedPitch false def\n");
        program.extend_from_slice(b"/UnderlinePosition -100 def\n");
        program.extend_from_slice(b"/UnderlineThickness 50 def\n");
        program.extend_from_slice(b"end readonly def\n");
        program.extend_from_slice(b"/FontName /ExampleSans def\n");
        program.extend_from_slice(b"/PaintType 0 def\n/FontType 1 def\n");
        program.extend_from_slice(b"/FontMatrix [0.001 0 0 0.001 0 0] readonly def\n");
        program.extend_from_slice(b"/Encoding 256 array\n");
        program.extend_from_slice(b"0 1 255 {1 index exch /.notdef put} for\n");
        program.extend_from_slice(b"dup 65 /A put\ndup 66 /B put\n");
        program.extend_from_slice(b"readonly def\n");
        program.extend_from_slice(b"/FontBBox {0 -200 1000 900} readonly def\n");
        program.extend_from_slice(b"currentdict end\ncurrentfile eexec\n");
        program.extend_from_slice(&eexec);
        program.extend_from_slice(&[b'0'; 512]);
        program.extend_from_slice(b"\ncleartomark\n");
        program
    }

    fn parse_header_bytes(bytes: &[u8]) -> FontResult<Type1Header> {
        let mut scanner = Scanner::new(bytes);
        parse_header(&mut scanner)
    }

    #[test]
    fn font_file_lengths_split_clear_encrypted_and_fixed_sections() {
        let program = sample_program(None);
        let (length1, length2, length3) = font_file_lengths(&program);

        assert_eq!(length1 + length2 + length3, program.len());
        let eexec_end = program
            .windows(b"eexec".len())
            .position(|window| window == b"eexec")
            .expect("eexec keyword")
            + b"eexec".len();
        assert_eq!(
            length1,
            eexec_end + 1,
            "Length1 应覆盖明文头与 eexec 后的单个分隔符"
        );
        let guard = program
            .windows(512)
            .position(|window| window.iter().all(|&byte| byte == b'0'))
            .expect("512 零守卫");
        assert_eq!(length2, guard - length1, "Length2 应为加密段字节数");
        assert_eq!(length3, program.len() - guard, "Length3 应为尾部固定内容");
        assert!(length3 > 0);
    }

    #[test]
    fn font_file_lengths_without_eexec_are_all_clear_text() {
        let data = b"%!PS-AdobeFont-1.0: TestFont\n";
        assert_eq!(font_file_lengths(data), (data.len(), 0, 0));
    }

    fn custom_encoding(header: &Type1Header) -> &BTreeMap<u8, String> {
        match &header.encoding {
            Some(Type1BuiltinEncoding::Custom(entries)) => entries,
            other => panic!("expected a custom encoding, got {other:?}"),
        }
    }

    #[test]
    fn parses_header_private_dict_and_charstrings() {
        let program = Type1FontProgram::parse(&sample_program(None)).unwrap();

        assert_eq!(program.header.font_name.as_deref(), Some("ExampleSans"));
        assert_eq!(program.header.family_name.as_deref(), Some("Example Sans"));
        assert_eq!(
            program.header.full_name.as_deref(),
            Some("Example Sans Regular")
        );
        assert_eq!(program.header.weight.as_deref(), Some("Regular"));
        assert_eq!(program.header.italic_angle, Some(-12.5));
        assert_eq!(program.header.is_fixed_pitch, Some(false));
        assert_eq!(program.header.underline_position, Some(-100.0));
        assert_eq!(program.header.underline_thickness, Some(50.0));
        assert_eq!(program.header.font_bbox, Some([0.0, -200.0, 1000.0, 900.0]));
        assert_eq!(
            program.header.font_matrix,
            Some([0.001, 0.0, 0.0, 0.001, 0.0, 0.0])
        );

        // 生成式 `0 1 255 { ... put } for` 不产生显式条目。
        let encoding = custom_encoding(&program.header);
        assert_eq!(encoding.len(), 2);
        assert_eq!(encoding.get(&65).map(String::as_str), Some("A"));
        assert_eq!(encoding.get(&66).map(String::as_str), Some("B"));

        assert_eq!(program.len_iv, 4);
        assert_eq!(
            program.subrs.get(&0).map(Vec::as_slice),
            Some(b"\x8b\x0c".as_slice())
        );
        assert_eq!(
            program.subrs.get(&1).map(Vec::as_slice),
            Some(b"\x8b\x0e\x0c".as_slice())
        );
        assert_eq!(
            program.charstrings.get(".notdef").map(Vec::as_slice),
            Some(b"\x8b\x8b\x0c".as_slice())
        );
        assert_eq!(
            program.charstrings.get("A").map(Vec::as_slice),
            Some(b"\x8b\x0e\x0c".as_slice())
        );
        assert_eq!(
            program.charstrings.get("B").map(Vec::as_slice),
            Some(b"\x8b\x10\x0c".as_slice())
        );
    }

    #[test]
    fn defaults_len_iv_to_four() {
        let program = Type1FontProgram::parse(&sample_program(None)).unwrap();
        assert_eq!(program.len_iv, 4);
        assert_eq!(
            program.charstrings.get("A").map(Vec::as_slice),
            Some(b"\x8b\x0e\x0c".as_slice())
        );
    }

    #[test]
    fn len_iv_minus_one_keeps_charstrings_unencrypted() {
        let program = Type1FontProgram::parse(&sample_program(Some(-1))).unwrap();
        assert_eq!(program.len_iv, -1);
        assert_eq!(
            program.charstrings.get("A").map(Vec::as_slice),
            Some(b"\x8b\x0e\x0c".as_slice())
        );
        assert_eq!(
            program.subrs.get(&1).map(Vec::as_slice),
            Some(b"\x8b\x0e\x0c".as_slice())
        );
    }

    #[test]
    fn captures_predefined_encoding_name() {
        let header = parse_header_bytes(
            b"/FontName /Foo def\n/Encoding StandardEncoding def\ncurrentfile eexec\n",
        )
        .unwrap();
        assert_eq!(
            header.encoding,
            Some(Type1BuiltinEncoding::Predefined(
                "StandardEncoding".to_string()
            ))
        );
    }

    #[test]
    fn rejects_program_without_eexec() {
        let error =
            Type1FontProgram::parse(b"%!PS-AdobeFont-1.0: Missing\n/FontName /Missing def\n")
                .unwrap_err();
        assert!(error.to_string().contains("eexec"), "{error}");
    }

    #[test]
    fn rejects_private_dict_without_charstrings() {
        let mut plain = vec![0u8; 4];
        plain
            .extend_from_slice(b"dup /Private 8 dict dup begin\nend\nmark currentfile closefile\n");
        let encrypted = encrypt(&plain, 55665);
        let mut program = b"currentfile eexec\n".to_vec();
        program.extend_from_slice(&encrypted);
        program.extend_from_slice(&[b'0'; 512]);

        let error = Type1FontProgram::parse(&program).unwrap_err();
        assert!(error.to_string().contains("CharStrings"), "{error}");
    }

    #[test]
    fn rejects_truncated_rd_segment() {
        let mut plain = vec![0u8; 4];
        plain.extend_from_slice(
            b"dup /Private 8 dict dup begin\n/lenIV 0 def\n2 index /CharStrings 1 dict dup begin\n/.notdef 40 RD short",
        );
        let encrypted = encrypt(&plain, 55665);
        let mut program = b"currentfile eexec\n".to_vec();
        program.extend_from_slice(&encrypted);
        program.extend_from_slice(&[b'0'; 512]);

        let error = Type1FontProgram::parse(&program).unwrap_err();
        assert!(error.to_string().contains("truncated"), "{error}");
    }

    #[test]
    fn rejects_charstring_shorter_than_len_iv() {
        let mut plain = vec![0u8; 4];
        plain.extend_from_slice(
            b"dup /Private 8 dict dup begin\n/lenIV 4 def\n2 index /CharStrings 1 dict dup begin\n/.notdef 2 RD ab ND end",
        );
        let encrypted = encrypt(&plain, 55665);
        let mut program = b"currentfile eexec\n".to_vec();
        program.extend_from_slice(&encrypted);
        program.extend_from_slice(&[b'0'; 512]);

        let error = Type1FontProgram::parse(&program).unwrap_err();
        assert!(error.to_string().contains("lenIV"), "{error}");
    }
}
