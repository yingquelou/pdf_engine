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

use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::core::types::{Dict, Document, StreamData, Value};

use super::font::{SimpleFontEncoding, TrueTypeFont, Type0Font, Type1Font};

mod tokenizer;

use tokenizer::{CsToken, tokenize_content_stream};

// ── Extraction limits ─────────────────────────────────────
//
// Content streams are untrusted input: decoded bytes are commonly much larger
// than the file and a page can reference arbitrarily many streams (directly or
// through nested form XObjects). The limits below bound CPU and memory use so
// a hostile PDF cannot exhaust the process.

/// Maximum decoded content bytes per page, including nested form XObjects.
const MAX_CONTENT_BYTES: usize = 64 * 1024 * 1024;
/// Maximum content streams per page, including nested form XObjects.
const MAX_CONTENT_STREAMS: usize = 4096;
/// Maximum page tree (`/Kids`) nesting depth.
const MAX_PAGE_TREE_DEPTH: usize = 64;
/// Maximum `/Parent` chain length when inheriting page resources.
const MAX_PARENT_CHAIN_DEPTH: usize = 64;
/// Maximum form XObject nesting depth.
const MAX_FORM_DEPTH: usize = 32;

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

// ── Extraction errors ────────────────────────────────────

/// Failure modes of [`Document::try_extract_page_text`].
///
/// The infallible [`Document::extract_page_text`] collapses every error to
/// `None`; callers that need to distinguish malformed input from a resource
/// limit can use the fallible entry point instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextExtractionError {
    /// The requested page index is absent, or the page tree cannot be walked
    /// (cycle, excessive depth, or malformed page tree).
    PageNotFound,
    /// `/Contents`, `/Resources`, or a referenced object is missing or cannot
    /// be decoded.
    MalformedContent,
    /// The page's content streams (including nested form XObjects) exceed
    /// the extraction budget (64 MiB or 4096 streams).
    ContentTooLarge,
}

impl fmt::Display for TextExtractionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::PageNotFound => "page not found",
            Self::MalformedContent => "malformed page content",
            Self::ContentTooLarge => "page content exceeds extraction limits",
        };
        f.write_str(message)
    }
}

impl std::error::Error for TextExtractionError {}

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
    let mut context = ExtractionContext::new(document);
    // This convenience entry point has no error channel; exceeding the
    // extraction budget stops parsing and returns whatever was collected so
    // far. Use `Document::try_extract_page_text` to observe the error.
    let _ = context.extract_from_content(content_bytes, resources);
    context.state.output
}

/// Per-page extraction state shared by the page content stream and any form
/// XObjects it invokes.
struct ExtractionContext<'a> {
    document: &'a Document,
    state: TextState,
    budget: ContentBudget,
    /// Indirect form XObject references already rendered on this page, keyed
    /// by `(object number, generation)`. Prevents cyclic form invocations.
    visited_forms: HashSet<(i64, i64)>,
    form_depth: usize,
}

impl<'a> ExtractionContext<'a> {
    fn new(document: &'a Document) -> Self {
        Self {
            document,
            state: TextState::new(),
            budget: ContentBudget::new(),
            visited_forms: HashSet::new(),
            form_depth: 0,
        }
    }

    /// Parse one content stream (page content or a form XObject body) and
    /// append its text to the shared output.
    fn extract_from_content(
        &mut self,
        content_bytes: &[u8],
        resources: &Dict,
    ) -> Result<(), TextExtractionError> {
        resolve_fonts(resources, self.document, &mut self.state);

        let tokens = tokenize_content_stream(content_bytes);
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
                    if kw == "Do" {
                        if let Some(Operand::Name(name)) = operands.last() {
                            self.draw_form(name, resources)?;
                        }
                    } else {
                        execute_operator(kw, &operands, &mut self.state);
                    }
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
        Ok(())
    }

    /// Handle `Do` for a form XObject: resolve it through the current
    /// `/XObject` resource dictionary and append its text in place.
    ///
    /// Missing, malformed, image, and already visited XObjects are skipped so
    /// one bad resource does not fail the whole page. The content budget error
    /// is deliberately propagated.
    fn draw_form(&mut self, name: &str, resources: &Dict) -> Result<(), TextExtractionError> {
        let Some(xobject_map) = resources
            .get("XObject")
            .and_then(|value| resolve_value(self.document, value))
        else {
            return Ok(());
        };
        let Some(xobject_map) = xobject_map.as_dict() else {
            return Ok(());
        };
        let Some(xobject_value) = xobject_map.get(name) else {
            return Ok(());
        };
        let Some(resolved) = resolve_value(self.document, xobject_value) else {
            return Ok(());
        };
        let Some(dict) = resolved.as_dict() else {
            return Ok(());
        };
        let subtype = dict
            .get("Subtype")
            .and_then(|value| resolve_value(self.document, value));
        if subtype.as_ref().and_then(Value::as_name) != Some("Form") {
            return Ok(());
        }

        if let Some(key) = xobject_value.as_ref()
            && !self.visited_forms.insert(key)
        {
            return Ok(());
        }
        if self.form_depth >= MAX_FORM_DEPTH {
            return Ok(());
        }

        // A form without its own `/Resources` inherits the caller's resources
        // (page or enclosing form), per ISO 32000-1 8.10.
        let form_resources = match dict.get("Resources") {
            Some(value) => match resolve_value(self.document, value)
                .and_then(|value| value.as_dict().cloned())
            {
                Some(dict) => dict,
                None => return Ok(()),
            },
            None => resources.clone(),
        };

        let bytes = match self.decode_stream(&resolved) {
            Ok(bytes) => bytes,
            Err(TextExtractionError::ContentTooLarge) => {
                return Err(TextExtractionError::ContentTooLarge);
            }
            Err(_) => return Ok(()),
        };

        self.form_depth += 1;
        let result = self.extract_from_content(&bytes, &form_resources);
        self.form_depth -= 1;
        result
    }

    /// Collect the page `/Contents` bytes, charging every stream and byte to
    /// the shared budget.
    fn collect_content_bytes(&mut self, page: &Dict) -> Result<Vec<u8>, TextExtractionError> {
        let contents = page
            .get("Contents")
            .ok_or(TextExtractionError::MalformedContent)?;
        let contents =
            resolve_value(self.document, contents).ok_or(TextExtractionError::MalformedContent)?;
        match contents {
            Value::Stream(_, _) => self.decode_stream(&contents),
            Value::Array(array) => {
                if array.len() > self.budget.streams_remaining {
                    return Err(TextExtractionError::ContentTooLarge);
                }
                let mut out = Vec::new();
                for item in array.iter() {
                    let item = resolve_value(self.document, item)
                        .ok_or(TextExtractionError::MalformedContent)?;
                    let bytes = self.decode_stream(&item)?;
                    if !out.is_empty() {
                        out.push(b'\n');
                    }
                    out.extend_from_slice(&bytes);
                }
                Ok(out)
            }
            _ => Err(TextExtractionError::MalformedContent),
        }
    }

    /// Decode one stream while charging it to the page-wide budget.
    ///
    /// The raw size is checked first when it is already in memory so an
    /// oversized stream is rejected before decoding; the decoded size is
    /// checked as well because filters can expand data substantially.
    fn decode_stream(&mut self, value: &Value) -> Result<Vec<u8>, TextExtractionError> {
        if self.budget.streams_remaining == 0 {
            return Err(TextExtractionError::ContentTooLarge);
        }
        // Reject oversized streams before reading or cloning their raw bytes.
        let raw_len = match value {
            Value::Stream(_, StreamData::Owned(raw)) => Some(raw.len()),
            Value::Stream(
                _,
                StreamData::Ref { len, .. } | StreamData::EncryptedRef { len, .. },
            ) => usize::try_from(*len).ok(),
            _ => None,
        };
        if raw_len.is_some_and(|len| len > self.budget.bytes_remaining) {
            return Err(TextExtractionError::ContentTooLarge);
        }
        self.budget.streams_remaining -= 1;
        let bytes = self
            .document
            .decode_stream_bytes(value)
            .map_err(|_| TextExtractionError::MalformedContent)?;
        if bytes.len() > self.budget.bytes_remaining {
            return Err(TextExtractionError::ContentTooLarge);
        }
        self.budget.bytes_remaining -= bytes.len();
        Ok(bytes)
    }
}

/// Byte/stream budget shared by page content and nested form XObjects.
struct ContentBudget {
    bytes_remaining: usize,
    streams_remaining: usize,
}

impl ContentBudget {
    fn new() -> Self {
        Self {
            bytes_remaining: MAX_CONTENT_BYTES,
            streams_remaining: MAX_CONTENT_STREAMS,
        }
    }
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
    /// Returns `None` when the page index is out of range, the page structure
    /// is malformed, or the content exceeds the extraction limits. Use
    /// [`Document::try_extract_page_text`] to distinguish those cases.
    pub fn extract_page_text(&self, page_index: usize) -> Option<String> {
        self.try_extract_page_text(page_index).ok()
    }

    /// Fallible variant of [`Document::extract_page_text`].
    ///
    /// Page tree traversal and `/Parent` resource inheritance are bounded (64
    /// levels) and cycle-safe. Page content plus nested form XObjects share a
    /// 64 MiB / 4096-stream budget; exceeding it returns
    /// [`TextExtractionError::ContentTooLarge`] instead of truncating.
    pub fn try_extract_page_text(&self, page_index: usize) -> Result<String, TextExtractionError> {
        let root = self
            .trailer
            .dict
            .get("Root")
            .ok_or(TextExtractionError::PageNotFound)?;
        let root = resolve_value(self, root).ok_or(TextExtractionError::PageNotFound)?;
        let root = root.as_dict().ok_or(TextExtractionError::PageNotFound)?;
        let pages_value = root.get("Pages").ok_or(TextExtractionError::PageNotFound)?;
        let pages_key = pages_value.as_ref();
        let pages = resolve_value(self, pages_value).ok_or(TextExtractionError::PageNotFound)?;
        let pages = pages.as_dict().ok_or(TextExtractionError::PageNotFound)?;

        let page_value = find_page_node(self, pages, pages_key, page_index)
            .ok_or(TextExtractionError::PageNotFound)?;
        let page = page_value
            .as_dict()
            .ok_or(TextExtractionError::PageNotFound)?;

        let resources =
            resolve_page_resources(self, page).ok_or(TextExtractionError::MalformedContent)?;
        let resources = resources
            .as_dict()
            .ok_or(TextExtractionError::MalformedContent)?;

        let mut context = ExtractionContext::new(self);
        let content_bytes = context.collect_content_bytes(page)?;
        context.extract_from_content(&content_bytes, resources)?;
        Ok(context.state.output)
    }
}

/// Find the `index`th page with an iterative depth-first traversal.
///
/// `pages_ref` is the indirect reference the root `/Pages` node was resolved
/// from (if any) and seeds the visited set so a `/Kids` entry pointing back at
/// the root is rejected. Only indirect references are tracked; directly
/// embedded dictionaries cannot form a cycle because a value cannot contain
/// itself.
fn find_page_node(
    document: &Document,
    pages: &Dict,
    pages_ref: Option<(i64, i64)>,
    index: usize,
) -> Option<Value> {
    let mut visited: HashSet<(i64, i64)> = HashSet::new();
    if let Some(key) = pages_ref {
        visited.insert(key);
    }

    let mut stack = vec![(Value::Dict(pages.clone()), 0usize)];
    let mut page_number = 0usize;
    while let Some((node_value, depth)) = stack.pop() {
        if depth > MAX_PAGE_TREE_DEPTH {
            continue;
        }
        // Resolve refs lazily, but never enqueue the same ref twice. This is
        // what makes a cyclic `/Kids` graph terminate.
        let Some(node) = resolve_value(document, &node_value) else {
            continue;
        };
        let Some(node) = node.as_dict() else {
            continue;
        };
        match node.get("Type").and_then(Value::as_name).unwrap_or("") {
            "Page" => {
                if page_number == index {
                    return Some(Value::Dict(node.clone()));
                }
                page_number += 1;
            }
            "Pages" => {
                let Some(kids) = node.get("Kids") else {
                    continue;
                };
                let Some(kids) = resolve_value(document, kids) else {
                    continue;
                };
                let Some(kids) = kids.as_array() else {
                    continue;
                };
                for kid in kids.iter().rev() {
                    if let Some(key) = kid.as_ref()
                        && !visited.insert(key)
                    {
                        continue;
                    }
                    stack.push((kid.clone(), depth + 1));
                }
            }
            _ => {}
        }
    }
    None
}

/// Resolve a page's `/Resources`, walking `/Parent` links with a visited set
/// and a depth bound. A cycle stops the walk; returning the nearest ancestor
/// resources found so far.
fn resolve_page_resources(document: &Document, page: &Dict) -> Option<Value> {
    let mut current = page.clone();
    let mut visited: HashSet<(i64, i64)> = HashSet::new();
    for _ in 0..=MAX_PARENT_CHAIN_DEPTH {
        if let Some(resources) = current.get("Resources") {
            return resolve_value(document, resources);
        }
        let parent = current.get("Parent")?;
        if let Some(key) = parent.as_ref()
            && !visited.insert(key)
        {
            return None;
        }
        let parent = resolve_value(document, parent)?;
        current = parent.as_dict()?.clone();
    }
    None
}

#[cfg(test)]
mod tests;
