//! Type 1 明文头与 Private 字典共用的 PostScript 词法扫描器。

use super::super::FontResult;
use super::error::invalid_type1;

pub(super) enum Token {
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

pub(super) struct Scanner<'a> {
    data: &'a [u8],
    pub(super) position: usize,
}

impl<'a> Scanner<'a> {
    pub(super) fn new(data: &'a [u8]) -> Self {
        Self { data, position: 0 }
    }

    pub(super) fn next_token(&mut self) -> FontResult<Option<Token>> {
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
    pub(super) fn skip_single_gap(&mut self) {
        match self.data.get(self.position) {
            Some(b'\r') if self.data.get(self.position + 1) == Some(&b'\n') => {
                self.position += 2;
            }
            Some(&byte) if is_whitespace(byte) => self.position += 1,
            _ => {}
        }
    }

    /// Read an `RD`/`-|` binary segment, whose leading separator is optional.
    pub(super) fn read_binary(&mut self, length: usize) -> FontResult<&'a [u8]> {
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

    pub(super) fn skip_ignorable(&mut self) {
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
