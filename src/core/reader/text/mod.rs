//! Content stream text extraction.
//!
//! Parses text-related operators from a content stream and decodes character
//! codes to Unicode through the page's font resources:
//!
//! - **Type 0** composite fonts: decoded through the encoding CMap, then
//!   mapped to Unicode via `/ToUnicode` (bfchar/bfrange).
//! - **Simple fonts** (Type 1, MMType1, TrueType): decoded through the
//!   `/Encoding` (WinAnsi, MacRoman, Standard, Symbol, ZapfDingbats) with
//!   AGL 2.0 fallback for `/Differences` glyph names. TrueType fonts without
//!   `/Encoding` first use the embedded program's `cmap` (its built-in
//!   encoding) and fall back to the predefined encoding when parsing fails.
//! - **Fallback**: raw bytes interpreted as Latin-1 when no font metadata
//!   is available.
//!
//! The extractor uses an operand stack: non-operator tokens (numbers,
//! strings, names, arrays) are pushed onto the stack; when an operator
//! keyword is encountered, it consumes the relevant operands. This matches
//! the PDF content stream model where operands precede their operator.

use std::collections::HashMap;

use crate::core::types::{Dict, Document, Value};

use super::font::{SimpleFontEncoding, TrueTypeFont, Type0Font, Type1Font};

mod tokenizer;

use tokenizer::{CsToken, tokenize_content_stream};

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
#[allow(clippy::large_enum_variant)]
enum FontHandle {
    Type0(Type0Font),
    Simple {
        #[allow(dead_code)]
        font: Option<Type1Font>,
        encoding: Option<SimpleFontEncoding>,
        builtin: Option<Vec<Option<char>>>,
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
            FontHandle::Simple {
                encoding, builtin, ..
            } => {
                for &byte in bytes {
                    let builtin_char = builtin.as_ref().and_then(|table| table[usize::from(byte)]);
                    match simple_byte_text(byte, builtin_char, encoding.as_ref()) {
                        Some(text) => self.output.push_str(&text),
                        None => self.output.push(byte as char),
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

/// Whether `ch` lies in the Unicode Private Use Area (U+E000–U+F8FF).
fn is_private_use(ch: char) -> bool {
    matches!(ch, '\u{E000}'..='\u{F8FF}')
}

/// Resolve the text for one byte of a simple font.
///
/// A symbolic TrueType cmap maps codes into the Private Use Area, so a
/// predefined or `/Differences` encoding is preferred over a PUA built-in
/// character; a regular built-in character still wins over the encoding.
/// Returns `None` when neither source produces text.
fn simple_byte_text(
    byte: u8,
    builtin: Option<char>,
    encoding: Option<&SimpleFontEncoding>,
) -> Option<String> {
    if let Some(ch) = builtin
        && !is_private_use(ch)
    {
        return Some(ch.to_string());
    }
    if let Some(sequence) = encoding.and_then(|encoding| encoding.unicode_sequence_for(byte)) {
        return Some(sequence);
    }
    builtin.map(String::from)
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
        } else {
            let font = document.type1_font(&font_val).ok().flatten();
            let encoding = document.simple_font_encoding(&font_val).ok().flatten();
            let builtin = true_type_builtin_encoding(document, &font_val);
            if font.is_some() || encoding.is_some() || font_val.as_dict().is_some() {
                state.fonts.insert(
                    name.clone(),
                    FontHandle::Simple {
                        font,
                        encoding,
                        builtin,
                    },
                );
            }
        }
    }
}

/// Resolve the embedded program's built-in single-byte encoding for a
/// TrueType simple font that has no `/Encoding` entry.
///
/// Returns `None` when `/Encoding` is present, the font is not `/TrueType`,
/// or the embedded `/FontFile2` program cannot be parsed, so callers keep the
/// predefined-encoding fallback.
fn true_type_builtin_encoding(document: &Document, font_val: &Value) -> Option<Vec<Option<char>>> {
    let dict = font_val.as_dict()?;
    if dict.get("Encoding").is_some() {
        return None;
    }
    if dict.get("Subtype").and_then(Value::as_name) != Some("TrueType") {
        return None;
    }
    let font: TrueTypeFont = document.truetype_font(font_val).ok().flatten()?;
    font.builtin_encoding().ok()
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

#[cfg(test)]
mod tests;
