//! Content stream text extraction.
//!
//! Parses text-related operators from a content stream and decodes character
//! codes to Unicode through the page's font resources:
//!
//! - **Type 0** composite fonts: decoded through the encoding CMap, then
//!   mapped to Unicode via `/ToUnicode` (bfchar/bfrange).
//! - **Type 1** simple fonts: decoded through the `/Encoding` (WinAnsi,
//!   MacRoman, Standard, Symbol, ZapfDingbats) with AGL 2.0 fallback.
//! - **Fallback**: raw bytes interpreted as Latin-1 when no font metadata
//!   is available.
//!
//! The extractor uses an operand stack: non-operator tokens (numbers,
//! strings, names, arrays) are pushed onto the stack; when an operator
//! keyword is encountered, it consumes the relevant operands. This matches
//! the PDF content stream model where operands precede their operator.

use std::collections::HashMap;

use crate::pdf::types::{Dict, Document, Value};

use super::font::{SimpleFontEncoding, Type0Font, Type1Font};

// ── Content stream tokenizer ──────────────────────────────

#[derive(Debug, Clone)]
enum CsToken {
    Number(f64),
    LiteralString(Vec<u8>),
    HexString(Vec<u8>),
    Name(String),
    Keyword(String),
    ArrayOpen,
    ArrayClose,
}

fn tokenize_content_stream(input: &[u8]) -> Vec<CsToken> {
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
                let name = String::from_utf8_lossy(&input[start..pos]).into_owned();
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
                if !kw.is_empty() {
                    tokens.push(CsToken::Keyword(kw));
                }
            }
        }
    }
    tokens
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

// ── Operand stack & text state ────────────────────────────

#[derive(Debug, Clone)]
enum Operand {
    Number(f64),
    String(Vec<u8>),
    Name(String),
    Array(Vec<Operand>),
}

fn token_to_operand(token: &CsToken) -> Option<Operand> {
    match token {
        CsToken::Number(n) => Some(Operand::Number(*n)),
        CsToken::LiteralString(b) | CsToken::HexString(b) => Some(Operand::String(b.clone())),
        CsToken::Name(n) => Some(Operand::Name(n.clone())),
        _ => None,
    }
}

#[derive(Clone)]
enum FontHandle {
    Type0(Type0Font),
    Type1 {
        #[allow(dead_code)]
        font: Option<Type1Font>,
        encoding: Option<SimpleFontEncoding>,
    },
}

struct TextState {
    font_name: Option<String>,
    fonts: HashMap<String, FontHandle>,
    output: String,
}

impl TextState {
    fn new() -> Self {
        Self {
            font_name: None,
            fonts: HashMap::new(),
            output: String::new(),
        }
    }

    fn set_font(&mut self, name: &str) {
        if self.fonts.contains_key(name) {
            self.font_name = Some(name.to_owned());
        }
    }

    fn show_bytes(&mut self, bytes: &[u8]) {
        let Some(font_name) = &self.font_name else {
            self.output.push_str(&lossy_latin1(bytes));
            return;
        };
        let Some(handle) = self.fonts.get(font_name) else {
            self.output.push_str(&lossy_latin1(bytes));
            return;
        };
        match handle {
            FontHandle::Type0(font) => match font.decode_codes(bytes) {
                Ok(codes) => {
                    for code in codes {
                        if let Some(unicode) = &code.unicode {
                            self.output.push_str(unicode);
                        } else {
                            self.output.push_str(&lossy_latin1(&code.code));
                        }
                    }
                }
                Err(_) => self.output.push_str(&lossy_latin1(bytes)),
            },
            FontHandle::Type1 { encoding, .. } => {
                if let Some(enc) = encoding {
                    for &byte in bytes {
                        match enc.unicode_sequence_for(byte) {
                            Some(s) => self.output.push_str(&s),
                            None => self.output.push(byte as char),
                        }
                    }
                } else {
                    for &byte in bytes {
                        self.output.push(byte as char);
                    }
                }
            }
        }
    }

    fn add_break(&mut self, kind: BreakKind) {
        if self.output.is_empty() {
            return;
        }
        let last = self.output.as_bytes().last().copied();
        match kind {
            BreakKind::Space => {
                if last != Some(b' ') && last != Some(b'\n') {
                    self.output.push(' ');
                }
            }
            BreakKind::Newline => {
                if last != Some(b'\n') {
                    self.output.push('\n');
                }
            }
        }
    }
}

enum BreakKind {
    Space,
    Newline,
}

fn lossy_latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| b as char).collect()
}

// ── Public entry point ────────────────────────────────────

/// Extract Unicode text from content stream bytes given a resource dictionary.
///
/// The resource dictionary's `/Font` sub-dictionary is consulted to resolve
/// font names used by `Tf`. Both Type 0 composite fonts (with `/ToUnicode`)
/// and Type 1 simple fonts (with predefined encodings) are decoded.
///
/// Positional operators (`Td`, `TD`, `T*`, `Tm`) insert space or newline
/// separators so that the output reads naturally. The `'` and `"` operators
/// are also handled.
pub fn extract_text(content_bytes: &[u8], resources: &Dict, document: &Document) -> String {
    let tokens = tokenize_content_stream(content_bytes);
    let mut state = TextState::new();
    resolve_fonts(resources, document, &mut state);

    let mut operands: Vec<Operand> = Vec::new();
    for token in &tokens {
        match token {
            CsToken::ArrayOpen => {
                operands.push(Operand::Array(Vec::new()));
            }
            CsToken::ArrayClose => {
                let array = match operands.pop() {
                    Some(Operand::Array(a)) => Operand::Array(a),
                    _ => Operand::Array(Vec::new()),
                };
                operands.push(array);
            }
            CsToken::Keyword(kw) => {
                execute_operator(kw, &operands, &mut state);
                operands.clear();
            }
            _ => {
                if let Some(op) = token_to_operand(token) {
                    if let Some(Operand::Array(arr)) = operands.last_mut() {
                        arr.push(op);
                    } else {
                        operands.push(op);
                    }
                }
            }
        }
    }
    state.output
}

fn execute_operator(kw: &str, operands: &[Operand], state: &mut TextState) {
    match kw {
        "Tf" => {
            if let Some(Operand::Name(name)) = operands.first() {
                state.set_font(name);
            }
        }
        "Tj" => {
            if let Some(bytes) = last_string(operands) {
                state.show_bytes(bytes);
            }
        }
        "TJ" => {
            for op in operands {
                if let Operand::Array(items) = op {
                    for item in items {
                        match item {
                            Operand::String(bytes) => state.show_bytes(bytes),
                            Operand::Number(n) if *n < -100.0 => {
                                state.add_break(BreakKind::Space);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        "'" => {
            state.add_break(BreakKind::Newline);
            if let Some(bytes) = last_string(operands) {
                state.show_bytes(bytes);
            }
        }
        "\"" => {
            state.add_break(BreakKind::Newline);
            if let Some(bytes) = last_string(operands) {
                state.show_bytes(bytes);
            }
        }
        "Td" | "TD" | "T*" | "Tm" => {
            state.add_break(BreakKind::Newline);
        }
        _ => {}
    }
}

fn last_string(operands: &[Operand]) -> Option<&[u8]> {
    operands.iter().rev().find_map(|op| match op {
        Operand::String(b) => Some(b.as_slice()),
        _ => None,
    })
}

// ── Font resolution ───────────────────────────────────────

fn resolve_fonts(resources: &Dict, document: &Document, state: &mut TextState) {
    let font_dict = match resources
        .get("Font")
        .and_then(|v| resolve_value(document, v))
    {
        Some(Value::Dict(d)) => d,
        _ => return,
    };
    for (name, font_value) in &font_dict.0 {
        let Some(font_val) = resolve_value(document, font_value) else {
            continue;
        };
        if let Ok(Some(type0)) = document.type0_font(&font_val) {
            state.fonts.insert(name.clone(), FontHandle::Type0(type0));
        } else if let Ok(Some(type1)) = document.type1_font(&font_val) {
            let encoding = document.type1_font_encoding(&font_val).ok().flatten();
            state.fonts.insert(
                name.clone(),
                FontHandle::Type1 {
                    font: Some(type1),
                    encoding,
                },
            );
        } else if font_val.as_dict().is_some() {
            let encoding = document.type1_font_encoding(&font_val).ok().flatten();
            state.fonts.insert(
                name.clone(),
                FontHandle::Type1 {
                    font: None,
                    encoding,
                },
            );
        }
    }
}

fn resolve_value(document: &Document, value: &Value) -> Option<Value> {
    match value {
        Value::Ref(num, generation) => document
            .get_object(*num)
            .ok()
            .flatten()
            .filter(|o| o.generation == *generation)
            .map(|o| o.value),
        _ => Some(value.clone()),
    }
}

// ── Document-level API ────────────────────────────────────

impl Document {
    /// Extract Unicode text from a single content stream.
    ///
    /// `content_bytes` are the decoded stream bytes; `resources` is the
    /// page's (or parent's) `/Resources` dictionary. Font references in
    /// the content stream are resolved through `self`.
    pub fn extract_text(&self, content_bytes: &[u8], resources: &Dict) -> String {
        extract_text(content_bytes, resources, self)
    }

    /// Extract Unicode text from a page by its 0-based index.
    ///
    /// Traverses the page tree to find the `n`th page, resolves its
    /// `/Contents` (single stream or array of streams) and `/Resources`
    /// (with inheritance from parent page tree nodes), then extracts text.
    ///
    /// Returns `None` when the page index is out of range or the page
    /// structure is malformed.
    pub fn extract_page_text(&self, page_index: usize) -> Option<String> {
        let root = self.trailer.dict.get("Root")?;
        let root = resolve_value(self, root)?;
        let root = root.as_dict()?;
        let pages_ref = root.get("Pages")?;
        let pages = resolve_value(self, pages_ref)?;
        let pages = pages.as_dict()?;

        let page_value = find_page_node(self, pages, page_index)?;
        let page = page_value.as_dict()?;

        let resources = resolve_page_resources(self, page)?;
        let resources = resources.as_dict()?;

        let content_bytes = collect_content_bytes(self, page)?;
        Some(self.extract_text(&content_bytes, resources))
    }
}

fn find_page_node(document: &Document, pages: &Dict, index: usize) -> Option<Value> {
    let kids = pages.get("Kids")?;
    let kids = resolve_value(document, kids)?;
    let kids = kids.as_array()?;
    let mut remaining = index;
    for kid_value in kids.iter() {
        let kid = resolve_value(document, kid_value)?;
        let kid_dict = kid.as_dict()?;
        let type_name = kid_dict.get("Type").and_then(|v| v.as_name()).unwrap_or("");
        match type_name {
            "Page" => {
                if remaining == 0 {
                    return Some(kid);
                }
                remaining -= 1;
            }
            "Pages" => {
                let count = kid_dict
                    .get("Count")
                    .and_then(|v| v.as_integer())
                    .unwrap_or(0) as usize;
                if remaining < count {
                    return find_page_node(document, kid_dict, remaining);
                }
                remaining -= count;
            }
            _ => {}
        }
    }
    None
}

fn resolve_page_resources(document: &Document, page: &Dict) -> Option<Value> {
    if let Some(resources) = page.get("Resources") {
        return resolve_value(document, resources);
    }
    let parent = page.get("Parent")?;
    let parent = resolve_value(document, parent)?;
    let parent = parent.as_dict()?;
    resolve_page_resources(document, parent)
}

fn collect_content_bytes(document: &Document, page: &Dict) -> Option<Vec<u8>> {
    let contents = page.get("Contents")?;
    let contents = resolve_value(document, contents)?;
    match contents {
        Value::Stream(_, _) => document.decode_stream_bytes(&contents).ok(),
        Value::Array(array) => {
            let mut out = Vec::new();
            for item in array.iter() {
                let item = resolve_value(document, item)?;
                let bytes = document.decode_stream_bytes(&item).ok()?;
                if !out.is_empty() {
                    out.push(b'\n');
                }
                out.extend_from_slice(&bytes);
            }
            Some(out)
        }
        _ => None,
    }
}

// ── Tests ─────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use crate::pdf::types::{
        Array, Dict, Document, Object, StartXref, StreamData, Trailer, Value, Xref,
    };

    fn winansi_type1_font() -> Value {
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Font".into()))
                .insert("Subtype", Value::Name("Type1".into()))
                .insert("BaseFont", Value::Name("Helvetica".into()))
                .insert("Encoding", Value::Name("WinAnsiEncoding".into())),
        )
    }

    fn hello_to_unicode() -> Value {
        Value::Stream(
            Dict::new(),
            StreamData::Owned(
                b"1 begincodespacerange <0000> <FFFF> endcodespacerange \
                  4 beginbfchar \
                    <0048> <0048> \
                    <0065> <0065> \
                    <006C> <006C> \
                    <006F> <006F> \
                  endbfchar"
                    .to_vec(),
            ),
        )
    }

    fn cid_system_info() -> Value {
        Value::Dict(
            Dict::new()
                .insert("Registry", Value::String("Adobe".into()))
                .insert("Ordering", Value::String("Identity".into()))
                .insert("Supplement", Value::Integer(0)),
        )
    }

    fn type0_with_descendant(descendant: Value) -> Value {
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Font".into()))
                .insert("Subtype", Value::Name("Type0".into()))
                .insert("BaseFont", Value::Name("ExampleComposite".into()))
                .insert("Encoding", Value::Name("Identity-H".into()))
                .insert("ToUnicode", hello_to_unicode())
                .insert(
                    "DescendantFonts",
                    Value::Array(Array::new().push(descendant)),
                ),
        )
    }

    fn type0_with_to_unicode() -> Value {
        let cid_descendant = Value::Dict(
            Dict::new()
                .insert("Subtype", Value::Name("CIDFontType2".into()))
                .insert("BaseFont", Value::Name("ExampleCID".into()))
                .insert("CIDSystemInfo", cid_system_info())
                .insert(
                    "FontDescriptor",
                    Value::Dict(Dict::new().insert("Type", Value::Name("FontDescriptor".into()))),
                ),
        );
        type0_with_descendant(cid_descendant)
    }

    fn type0_with_cid_type0_descendant() -> Value {
        let cid_descendant = Value::Dict(
            Dict::new()
                .insert("Subtype", Value::Name("CIDFontType0".into()))
                .insert("BaseFont", Value::Name("ExampleCIDType0".into()))
                .insert("CIDSystemInfo", cid_system_info())
                .insert(
                    "FontDescriptor",
                    Value::Dict(
                        Dict::new()
                            .insert("Type", Value::Name("FontDescriptor".into()))
                            .insert(
                                "FontFile3",
                                Value::Stream(
                                    Dict::new()
                                        .insert("Subtype", Value::Name("CIDFontType0C".into())),
                                    StreamData::Owned(b"CFF ".to_vec()),
                                ),
                            ),
                    ),
                ),
        );
        type0_with_descendant(cid_descendant)
    }

    fn resources_with_font(name: &str, font: Value) -> Dict {
        Dict::new().insert("Font", Value::Dict(Dict::new().insert(name, font)))
    }

    #[test]
    fn extracts_type1_winansi_text() {
        let document = Document::new();
        let resources = resources_with_font("F1", winansi_type1_font());
        let content = b"BT /F1 12 Tf (Hello) Tj ET";
        let text = document.extract_text(content, &resources);
        assert_eq!(text, "Hello");
    }

    #[test]
    fn extracts_type0_with_to_unicode() {
        let document = Document::new();
        let resources = resources_with_font("F2", type0_with_to_unicode());
        let content = b"BT /F2 12 Tf <00480065006C006C006F> Tj ET";
        let text = document.extract_text(content, &resources);
        assert_eq!(text, "Hello");
    }

    #[test]
    fn extracts_type0_with_cid_font_type0_descendant() {
        let document = Document::new();
        let resources = resources_with_font("F3", type0_with_cid_type0_descendant());
        let content = b"BT /F3 12 Tf <00480065006C006C006F> Tj ET";
        let text = document.extract_text(content, &resources);
        assert_eq!(text, "Hello");
    }

    #[test]
    fn tj_array_with_positioning() {
        let document = Document::new();
        let resources = resources_with_font("F1", winansi_type1_font());
        let content = b"BT /F1 12 Tf [(He) -200 (llo)] TJ ET";
        let text = document.extract_text(content, &resources);
        assert_eq!(text, "He llo");
    }

    #[test]
    fn multiple_text_blocks() {
        let document = Document::new();
        let resources = resources_with_font("F1", winansi_type1_font());
        let content = b"BT /F1 12 Tf (First) Tj ET BT /F1 12 Tf (Second) Tj ET";
        let text = document.extract_text(content, &resources);
        assert_eq!(text, "FirstSecond");
    }

    #[test]
    fn td_inserts_newline() {
        let document = Document::new();
        let resources = resources_with_font("F1", winansi_type1_font());
        let content = b"BT /F1 12 Tf (Line1) Tj 0 -14 Td (Line2) Tj ET";
        let text = document.extract_text(content, &resources);
        assert_eq!(text, "Line1\nLine2");
    }

    #[test]
    fn hex_string_decoded() {
        let document = Document::new();
        let resources = resources_with_font("F1", winansi_type1_font());
        let content = b"BT /F1 12 Tf <48656C6C6F> Tj ET";
        let text = document.extract_text(content, &resources);
        assert_eq!(text, "Hello");
    }

    #[test]
    fn missing_font_falls_back_to_latin1() {
        let document = Document::new();
        let resources = Dict::new();
        let content = b"BT /F1 12 Tf (ABC) Tj ET";
        let text = document.extract_text(content, &resources);
        assert_eq!(text, "ABC");
    }

    #[test]
    fn tick_operator_inserts_newline() {
        let document = Document::new();
        let resources = resources_with_font("F1", winansi_type1_font());
        let content = b"BT /F1 12 Tf (A) Tj (B) ' ET";
        let text = document.extract_text(content, &resources);
        assert_eq!(text, "A\nB");
    }

    #[test]
    fn extract_page_text_from_constructed_document() {
        let font = winansi_type1_font();
        let resources = Value::Dict(resources_with_font("F1", font));
        let content = Value::Stream(
            Dict::new(),
            StreamData::Owned(b"BT /F1 12 Tf (Page1) Tj ET".to_vec()),
        );
        let page_dict = Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Page".into()))
                .insert("Contents", content)
                .insert("Resources", resources),
        );
        let pages_dict = Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Pages".into()))
                .insert("Kids", Value::Array(Array::new().push(Value::Ref(2, 0))))
                .insert("Count", Value::Integer(1)),
        );
        let catalog = Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Catalog".into()))
                .insert("Pages", pages_dict),
        );

        let mut trailer = Trailer::new();
        trailer.dict = Dict::new().insert("Root", Value::Ref(1, 0));

        let document = Document::from_objects(
            vec![Object::new(1, 0, catalog), Object::new(2, 0, page_dict)],
            Xref::new(),
            trailer,
            StartXref(0),
        );

        let text = document.extract_page_text(0).unwrap();
        assert_eq!(text, "Page1");
    }

    #[test]
    fn tokenizer_handles_comments_and_whitespace() {
        let document = Document::new();
        let resources = resources_with_font("F1", winansi_type1_font());
        let content = b"% this is a comment\nBT /F1 12 Tf (OK) Tj ET";
        let text = document.extract_text(content, &resources);
        assert_eq!(text, "OK");
    }

    #[test]
    fn empty_content_stream_returns_empty() {
        let document = Document::new();
        let resources = Dict::new();
        let text = document.extract_text(b"", &resources);
        assert_eq!(text, "");
    }
}
