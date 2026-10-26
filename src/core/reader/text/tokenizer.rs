//! Content-stream tokenizer used by text extraction.

#[derive(Debug, Clone)]
pub enum CsToken {
    Number(f64),
    LiteralString(Vec<u8>),
    HexString(Vec<u8>),
    Name(String),
    Keyword(String),
    ArrayOpen,
    ArrayClose,
}

pub fn tokenize_content_stream(input: &[u8]) -> Vec<CsToken> {
    let mut tokens = Vec::new();
    let mut pos = 0;
    while pos < input.len() {
        skip_ws_and_comments(input, &mut pos);
        if pos >= input.len() {
            break;
        }
        let b = input[pos];
        match b {
            b'(' => {
                let (s, next) = parse_literal_string(input, pos);
                tokens.push(CsToken::LiteralString(s));
                pos = next;
            }
            b'<' => {
                if input.get(pos + 1) == Some(&b'<') {
                    pos += 2;
                    while pos < input.len()
                        && !(input[pos] == b'>' && input.get(pos + 1) == Some(&b'>'))
                    {
                        pos += 1;
                    }
                    if pos < input.len() {
                        pos += 2;
                    }
                } else {
                    let (s, next) = parse_hex_string(input, pos);
                    tokens.push(CsToken::HexString(s));
                    pos = next;
                }
            }
            b'/' => {
                pos += 1;
                let start = pos;
                while pos < input.len() && is_regular(input[pos]) {
                    pos += 1;
                }
                let name = decode_name_escapes(&input[start..pos]);
                tokens.push(CsToken::Name(name));
            }
            b'[' => {
                tokens.push(CsToken::ArrayOpen);
                pos += 1;
            }
            b']' => {
                tokens.push(CsToken::ArrayClose);
                pos += 1;
            }
            b'0'..=b'9' | b'+' | b'-' | b'.' => {
                let start = pos;
                if matches!(input[pos], b'+' | b'-') {
                    pos += 1;
                }
                while pos < input.len() && matches!(input[pos], b'0'..=b'9' | b'.') {
                    pos += 1;
                }
                let num: f64 = String::from_utf8_lossy(&input[start..pos])
                    .parse()
                    .unwrap_or(0.0);
                tokens.push(CsToken::Number(num));
            }
            _ if is_delimiter(b) => {
                pos += 1;
            }
            _ => {
                let start = pos;
                while pos < input.len() && is_regular(input[pos]) && !is_delimiter(input[pos]) {
                    pos += 1;
                }
                let kw = String::from_utf8_lossy(&input[start..pos]).into_owned();
                if kw == "BI" {
                    // Inline images embed arbitrary binary bytes between `ID`
                    // and `EI`; skipping the whole span prevents image data
                    // from being tokenized as text operators or from opening a
                    // string/comment that swallows the following content.
                    pos = skip_inline_image(input, pos);
                } else if !kw.is_empty() {
                    tokens.push(CsToken::Keyword(kw));
                }
            }
        }
    }
    tokens
}

/// Decode `#xx` escapes in a PDF name (ISO 32000-1 7.3.5).
///
/// Invalid escapes are kept literally, matching the spec's guidance that
/// malformed `#` sequences are treated as ordinary characters.
fn decode_name_escapes(raw: &[u8]) -> String {
    let mut decoded = Vec::with_capacity(raw.len());
    let mut index = 0;
    while index < raw.len() {
        if raw[index] == b'#'
            && let (Some(high), Some(low)) = (
                raw.get(index + 1).copied().and_then(hex_value),
                raw.get(index + 2).copied().and_then(hex_value),
            )
        {
            decoded.push(high * 16 + low);
            index += 3;
            continue;
        }
        decoded.push(raw[index]);
        index += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Skip an inline image starting just after the `BI` keyword.
///
/// Scans the image dictionary for the `ID` keyword, then discards the binary
/// image data up to the closing `EI`. Returns the index just past `EI`, or the
/// end of the input when the image is truncated.
fn skip_inline_image(input: &[u8], mut pos: usize) -> usize {
    while pos < input.len() {
        skip_ws_and_comments(input, &mut pos);
        if pos >= input.len() {
            return pos;
        }
        match input[pos] {
            b'(' => {
                let (_, next) = parse_literal_string(input, pos);
                pos = next;
            }
            b'<' => {
                if input.get(pos + 1) == Some(&b'<') {
                    pos += 2;
                } else {
                    let (_, next) = parse_hex_string(input, pos);
                    pos = next;
                }
            }
            b'/' => {
                pos += 1;
                while pos < input.len() && is_regular(input[pos]) && !is_delimiter(input[pos]) {
                    pos += 1;
                }
            }
            _ if is_delimiter(input[pos]) => pos += 1,
            _ => {
                let start = pos;
                while pos < input.len() && is_regular(input[pos]) && !is_delimiter(input[pos]) {
                    pos += 1;
                }
                if &input[start..pos] == b"ID" {
                    if pos >= input.len() {
                        return pos;
                    }
                    if is_whitespace(input[pos]) {
                        return skip_inline_image_data(input, pos + 1);
                    }
                }
            }
        }
    }
    pos
}

/// Find the `EI` terminator of inline image data.
///
/// PDF requires `EI` to be preceded by white space and followed by white
/// space, a delimiter, or end of input (ISO 32000-1 8.9.7).
fn skip_inline_image_data(input: &[u8], start: usize) -> usize {
    let mut pos = start;
    while pos + 1 < input.len() {
        if input[pos] == b'E' && input[pos + 1] == b'I' {
            let preceded_by_space = pos > 0 && is_whitespace(input[pos - 1]);
            let followed_by_separator = pos + 2 >= input.len()
                || is_whitespace(input[pos + 2])
                || is_delimiter(input[pos + 2]);
            if preceded_by_space && followed_by_separator {
                return pos + 2;
            }
        }
        pos += 1;
    }
    input.len()
}

fn skip_ws_and_comments(input: &[u8], pos: &mut usize) {
    while *pos < input.len() {
        let b = input[*pos];
        if is_whitespace(b) {
            *pos += 1;
        } else if b == b'%' {
            while *pos < input.len() && input[*pos] != b'\n' && input[*pos] != b'\r' {
                *pos += 1;
            }
        } else {
            break;
        }
    }
}

fn is_whitespace(b: u8) -> bool {
    matches!(b, 0x00 | 0x09 | 0x0A | 0x0C | 0x0D | 0x20)
}

fn is_delimiter(b: u8) -> bool {
    matches!(
        b,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    )
}

fn is_regular(b: u8) -> bool {
    b > 0x20 && !matches!(b, 0x7F)
}

fn parse_literal_string(input: &[u8], start: usize) -> (Vec<u8>, usize) {
    let mut pos = start + 1;
    let mut depth = 1u32;
    let mut bytes = Vec::new();
    while pos < input.len() && depth > 0 {
        match input[pos] {
            b'\\' => {
                pos += 1;
                if pos < input.len() {
                    match input[pos] {
                        b'n' => {
                            bytes.push(b'\n');
                            pos += 1;
                        }
                        b'r' => {
                            bytes.push(b'\r');
                            pos += 1;
                        }
                        b't' => {
                            bytes.push(b'\t');
                            pos += 1;
                        }
                        b'b' => {
                            bytes.push(0x08);
                            pos += 1;
                        }
                        b'f' => {
                            bytes.push(0x0C);
                            pos += 1;
                        }
                        b'(' => {
                            bytes.push(b'(');
                            pos += 1;
                        }
                        b')' => {
                            bytes.push(b')');
                            pos += 1;
                        }
                        b'\\' => {
                            bytes.push(b'\\');
                            pos += 1;
                        }
                        b'\r' => {
                            pos += 1;
                            if input.get(pos) == Some(&b'\n') {
                                pos += 1;
                            }
                        }
                        b'\n' => {
                            pos += 1;
                        }
                        b'0'..=b'7' => {
                            let (code, consumed) = parse_octal(input, pos);
                            bytes.push(code);
                            pos += consumed;
                        }
                        _ => {
                            bytes.push(input[pos]);
                            pos += 1;
                        }
                    }
                }
            }
            b'(' => {
                depth += 1;
                bytes.push(b'(');
                pos += 1;
            }
            b')' => {
                depth -= 1;
                if depth > 0 {
                    bytes.push(b')');
                }
                pos += 1;
            }
            b => {
                bytes.push(b);
                pos += 1;
            }
        }
    }
    (bytes, pos)
}

fn parse_octal(input: &[u8], start: usize) -> (u8, usize) {
    let mut code: u16 = 0;
    let mut consumed = 0;
    for i in 0..3 {
        match input.get(start + i) {
            Some(&b @ b'0'..=b'7') if i == 0 || consumed == i => {
                code = code * 8 + (b - b'0') as u16;
                consumed = i + 1;
            }
            _ => break,
        }
    }
    (code as u8, consumed)
}

fn parse_hex_string(input: &[u8], start: usize) -> (Vec<u8>, usize) {
    let mut pos = start + 1;
    let mut bytes = Vec::new();
    let mut hi: Option<u8> = None;
    while pos < input.len() && input[pos] != b'>' {
        if let Some(d) = hex_digit(input[pos]) {
            match hi {
                None => hi = Some(d),
                Some(h) => {
                    bytes.push(h * 16 + d);
                    hi = None;
                }
            }
        }
        pos += 1;
    }
    if let Some(h) = hi {
        bytes.push(h * 16);
    }
    if pos < input.len() {
        pos += 1;
    }
    (bytes, pos)
}

fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}
