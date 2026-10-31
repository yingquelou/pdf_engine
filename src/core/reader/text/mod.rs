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
//! Marked content (14.6) affects the output: `BMC`/`BDC` sections resolve
//! their property lists (inline dictionary or `/Properties` name), a section
//! whose optional-content group (8.11.2) is off by the default configuration
//! is not extracted, `/ActualText` replaces the section's shown text, and
//! `/Artifact` sections (14.8.2.2.1) are skipped entirely. Unbalanced `EMC` is
//! ignored.
//!
//! [`Document::try_extract_page_text_fragments`] returns the same text split
//! into positioned runs ([`TextFragment`]), tracking the text matrix and the
//! current transformation matrix instead of only inserting separators.
//!
//! The extractor uses an operand stack: non-operator tokens (numbers,
//! strings, names, arrays) are pushed onto the stack; when an operator
//! keyword is encountered, it consumes the relevant operands. This matches
//! the PDF content stream model where operands precede their operator.

use std::collections::{HashMap, HashSet};
use std::fmt;

use crate::core::reader::content::{parse_dictionary_inner, text_value};
use crate::core::reader::optional_content::OptionalContentProperties;
use crate::core::types::{Dict, Document, StreamData, Value};

use super::font::{SimpleFontEncoding, SimpleFontToUnicode, TrueTypeFont, Type0Font, Type1Font};

pub mod tokenizer;

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
/// Maximum saved text/graphics states from `q` operators.
const MAX_STATE_DEPTH: usize = 256;
/// Maximum `/OC` name/reference redirection depth.
const MAX_OC_DEPTH: usize = 16;

// ── Positioned fragments ──────────────────────────────────

/// One run of extracted text at its approximate position.
///
/// The position is the text-space origin of the run transformed by the text
/// matrix (`Tm`/`Td`/`TD`/`T*`, including `TL` leading and `Ts` rise) and the
/// current transformation matrix (`cm`, `q`/`Q`, and a form XObject's
/// `/Matrix`). Glyph advances inside a shown string are not modeled, so
/// consecutive runs on one line may share a position.
#[derive(Debug, Clone, PartialEq)]
pub struct TextFragment {
    /// Decoded Unicode text of the run.
    pub text: String,
    /// X coordinate in default user space.
    pub x: f64,
    /// Y coordinate in default user space.
    pub y: f64,
}

/// Affine transform in PDF's row-vector convention (`[x y 1] × M`).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Matrix {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    e: f64,
    f: f64,
}

impl Matrix {
    const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    fn from_values(values: [f64; 6]) -> Self {
        Self {
            a: values[0],
            b: values[1],
            c: values[2],
            d: values[3],
            e: values[4],
            f: values[5],
        }
    }

    fn translate(tx: f64, ty: f64) -> Self {
        Self {
            e: tx,
            f: ty,
            ..Self::IDENTITY
        }
    }

    /// Concatenation: applies `self` first, then `other`.
    fn concat(self, other: Self) -> Self {
        Self {
            a: self.a * other.a + self.b * other.c,
            b: self.a * other.b + self.b * other.d,
            c: self.c * other.a + self.d * other.c,
            d: self.c * other.b + self.d * other.d,
            e: self.e * other.a + self.f * other.c + other.e,
            f: self.e * other.b + self.f * other.d + other.f,
        }
    }

    fn transform(self, x: f64, y: f64) -> (f64, f64) {
        (
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        )
    }
}

// ── Operand stack & text state ────────────────────────────

#[derive(Debug, Clone)]
enum Operand {
    Number(f64),
    String(Vec<u8>),
    Name(String),
    Array(Vec<Operand>),
    Dict(Dict),
}

fn token_to_operand(token: &CsToken) -> Option<Operand> {
    match token {
        CsToken::Number(n) => Some(Operand::Number(*n)),
        CsToken::LiteralString(b) | CsToken::HexString(b) => Some(Operand::String(b.clone())),
        CsToken::Name(n) => Some(Operand::Name(n.clone())),
        CsToken::Dictionary(inner) => Some(Operand::Dict(parse_dictionary_inner(inner))),
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
        to_unicode: Option<SimpleFontToUnicode>,
    },
}

/// One open `BMC`/`BDC` marked-content section while extracting text.
#[derive(Debug, Clone)]
struct MarkedFrame {
    /// `true` when this section (or an enclosing one) is an `/Artifact` or
    /// belongs to an optional-content group that the default configuration
    /// turns off.
    hidden: bool,
    /// `/ActualText` replacing the section's rendered text, when present.
    actual_text: Option<String>,
    /// `true` once `actual_text` has been emitted (later shows in the
    /// section are dropped — the replacement covers the whole section).
    actual_emitted: bool,
}

/// Text/graphics state saved by `q` and restored by `Q`.
#[derive(Clone)]
struct SavedState {
    font_name: Option<String>,
    ctm: Matrix,
    leading: f64,
    rise: f64,
}

struct TextState {
    font_name: Option<String>,
    fonts: HashMap<String, FontHandle>,
    output: String,
    fragments: Vec<TextFragment>,
    /// Current run being accumulated; flushed when the position changes or a
    /// separator is emitted.
    pending_fragment: Option<TextFragment>,
    /// States saved by `q` and restored by `Q`. Only the selected font and the
    /// text parameters that affect positioning are modeled.
    saved_states: Vec<SavedState>,
    /// Open marked-content sections (`BMC`/`BDC` … `EMC` nesting).
    marked: Vec<MarkedFrame>,
    /// Current transformation matrix (`cm`, `q`/`Q`, form `/Matrix`).
    ctm: Matrix,
    /// Text matrix `Tm` and text line matrix `Tlm` (9.4.2).
    text_matrix: Matrix,
    line_matrix: Matrix,
    /// Leading `TL` and text rise `Ts`.
    leading: f64,
    rise: f64,
}

impl TextState {
    fn new() -> Self {
        Self {
            font_name: None,
            fonts: HashMap::new(),
            output: String::new(),
            fragments: Vec::new(),
            pending_fragment: None,
            saved_states: Vec::new(),
            marked: Vec::new(),
            ctm: Matrix::IDENTITY,
            text_matrix: Matrix::IDENTITY,
            line_matrix: Matrix::IDENTITY,
            leading: 0.0,
            rise: 0.0,
        }
    }

    fn begin_marked(&mut self, hidden: bool, actual_text: Option<String>) {
        let parent_hidden = self.marked.last().is_some_and(|frame| frame.hidden);
        self.marked.push(MarkedFrame {
            hidden: parent_hidden || hidden,
            actual_text,
            actual_emitted: false,
        });
    }

    /// Close the innermost section; unbalanced `EMC` is ignored.
    fn end_marked(&mut self) {
        self.marked.pop();
    }

    fn marked_hidden(&self) -> bool {
        self.marked.last().is_some_and(|frame| frame.hidden)
    }

    fn set_font(&mut self, name: &str) {
        if self.fonts.contains_key(name) {
            self.font_name = Some(name.to_owned());
        }
    }

    fn save_state(&mut self) {
        if self.saved_states.len() < MAX_STATE_DEPTH {
            self.saved_states.push(SavedState {
                font_name: self.font_name.clone(),
                ctm: self.ctm,
                leading: self.leading,
                rise: self.rise,
            });
        }
    }

    fn restore_state(&mut self) {
        if let Some(saved) = self.saved_states.pop() {
            self.font_name = saved.font_name;
            self.ctm = saved.ctm;
            self.leading = saved.leading;
            self.rise = saved.rise;
        }
    }

    /// `BT` resets both text matrices; text state parameters are preserved.
    fn begin_text(&mut self) {
        self.text_matrix = Matrix::IDENTITY;
        self.line_matrix = Matrix::IDENTITY;
    }

    fn set_text_matrix(&mut self, matrix: Matrix) {
        self.text_matrix = matrix;
        self.line_matrix = matrix;
    }

    /// `Td`/`TD`/`T*`: move the line matrix and make it the text matrix.
    fn move_line(&mut self, tx: f64, ty: f64) {
        self.line_matrix = Matrix::translate(tx, ty).concat(self.line_matrix);
        self.text_matrix = self.line_matrix;
    }

    fn next_line(&mut self) {
        self.move_line(0.0, -self.leading);
    }

    /// `cm`: concatenate a matrix with the current transformation matrix.
    fn concatenate_ctm(&mut self, matrix: Matrix) {
        self.ctm = matrix.concat(self.ctm);
    }

    /// Text-space origin of the next shown run in default user space.
    fn position(&self) -> (f64, f64) {
        self.text_matrix.concat(self.ctm).transform(0.0, self.rise)
    }

    fn show_bytes(&mut self, bytes: &[u8]) {
        if self.marked_hidden() {
            return;
        }
        // 已经输出过 ActualText 的区段覆盖当前 show（包括它内部的所有
        // 嵌套区段），同一替换只发出一次。
        if self.marked.iter().any(|frame| frame.actual_emitted) {
            return;
        }
        // 最内层带 /ActualText 的区段是胜者；外层 ActualText 覆盖内部
        // 没有自己替换文本的嵌套区段。
        if let Some(winner) = self
            .marked
            .iter()
            .rposition(|frame| frame.actual_text.is_some())
        {
            let replacement = self.marked[winner].actual_text.clone().unwrap_or_default();
            for frame in &mut self.marked {
                if frame.actual_text.is_some() {
                    frame.actual_emitted = true;
                }
            }
            if !replacement.is_empty() {
                self.flush_fragment();
                let (x, y) = self.position();
                self.output.push_str(&replacement);
                self.fragments.push(TextFragment {
                    text: replacement,
                    x,
                    y,
                });
            }
            return;
        }
        let text = self.decode_text(bytes);
        if text.is_empty() {
            return;
        }
        self.output.push_str(&text);
        let (x, y) = self.position();
        self.push_fragment_run(text, x, y);
    }

    /// Decode one shown string through the selected font, falling back to
    /// Latin-1 when no font metadata is available.
    fn decode_text(&self, bytes: &[u8]) -> String {
        let Some(font_name) = &self.font_name else {
            return lossy_latin1(bytes);
        };
        let Some(handle) = self.fonts.get(font_name) else {
            return lossy_latin1(bytes);
        };
        let mut text = String::new();
        match handle {
            FontHandle::Type0(font) => match font.decode_codes(bytes) {
                Ok(codes) => {
                    for code in codes {
                        if let Some(unicode) = &code.unicode {
                            text.push_str(unicode);
                        } else {
                            text.push_str(&lossy_latin1(&code.code));
                        }
                    }
                }
                Err(_) => text.push_str(&lossy_latin1(bytes)),
            },
            FontHandle::Simple {
                encoding,
                builtin,
                to_unicode,
                ..
            } => {
                for &byte in bytes {
                    // `/ToUnicode` 是字符码到 Unicode 的权威映射，优先于
                    // 内置编码与 /Encoding（PDF 32000-1 §9.10.3）。
                    if let Some(mapped) = to_unicode.as_ref().and_then(|map| map.unicode_for(byte))
                    {
                        text.push_str(mapped);
                        continue;
                    }
                    let builtin_char = builtin.as_ref().and_then(|table| table[usize::from(byte)]);
                    match simple_byte_text(byte, builtin_char, encoding.as_ref()) {
                        Some(decoded) => text.push_str(&decoded),
                        None => text.push(byte as char),
                    }
                }
            }
        }
        text
    }

    fn push_fragment_run(&mut self, text: String, x: f64, y: f64) {
        if let Some(pending) = self.pending_fragment.as_mut()
            && pending.x == x
            && pending.y == y
        {
            pending.text.push_str(&text);
            return;
        }
        self.flush_fragment();
        self.pending_fragment = Some(TextFragment { text, x, y });
    }

    fn flush_fragment(&mut self) {
        if let Some(fragment) = self.pending_fragment.take() {
            self.fragments.push(fragment);
        }
    }

    fn add_break(&mut self, kind: BreakKind) {
        if self.marked_hidden() || self.output.is_empty() {
            return;
        }
        self.flush_fragment();
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
    /// Indirect form XObject references currently being executed, keyed by
    /// `(object number, generation)`. Only the active recursion chain is
    /// tracked so a form may be drawn repeatedly on the same page.
    active_forms: HashSet<(i64, i64)>,
    form_depth: usize,
    /// Catalog `/OCProperties`, loaded once so `BMC`/`BDC` sections can be
    /// filtered against the default optional-content configuration.
    oc_properties: Option<OptionalContentProperties>,
}

impl<'a> ExtractionContext<'a> {
    fn new(document: &'a Document) -> Self {
        Self {
            document,
            state: TextState::new(),
            budget: ContentBudget::new(),
            active_forms: HashSet::new(),
            form_depth: 0,
            oc_properties: document.optional_content_properties().ok().flatten(),
        }
    }

    /// Open a marked-content section for `BMC` (no operand) or `BDC`
    /// (inline dictionary or `/Properties` resource name).
    ///
    /// `/Artifact` sections are pagination or layout decorations and are
    /// excluded from extracted text (ISO 32000-1 14.8.2.2.1), as are sections
    /// whose optional-content group is off in the default configuration.
    fn open_marked(
        &mut self,
        tag: Option<&str>,
        property_operand: Option<&Operand>,
        resources: &Dict,
    ) {
        let properties = match property_operand {
            Some(Operand::Dict(dict)) => Some(dict.clone()),
            Some(Operand::Name(name)) => resolve_property_list(self.document, resources, name),
            _ => None,
        };
        let hidden = self.operand_visibility(property_operand, resources) == Some(false);
        let actual_text = properties
            .as_ref()
            .and_then(|dict| dict.get("ActualText"))
            .and_then(text_value);
        self.state
            .begin_marked(hidden || tag == Some("Artifact"), actual_text);
    }

    /// Resolve the optional-content visibility of a `BDC`/`DP` property
    /// operand (ISO 32000-1 14.6.2, 8.11.2).
    ///
    /// The operand is an inline property list or a `/Properties` resource
    /// name. A named entry may be the OCG dictionary itself (the pattern
    /// written by this crate's builder) or a property list carrying `/OC`.
    /// `/OC` values may be OCG references, inline OCMDs, or indirect
    /// references to either; OCMDs are evaluated through their `/P` policy or
    /// `/VE` expression against the default configuration.
    fn operand_visibility(&self, operand: Option<&Operand>, resources: &Dict) -> Option<bool> {
        match operand? {
            Operand::Dict(dict) => self.property_list_visibility(dict, resources),
            Operand::Name(name) => {
                let entry = properties_entry(self.document, resources, name)?;
                let resolved = resolve_value(self.document, &entry);
                let is_inline_group = resolved.as_ref().is_some_and(|value| {
                    value
                        .as_dict()
                        .is_some_and(|dict| is_ocg_dictionary(dict) && dict.get("OC").is_none())
                });
                if is_inline_group {
                    return match &entry {
                        Value::Ref(object, _) => self.group_visible(*object),
                        _ => None,
                    };
                }
                self.oc_visibility(&entry, resources, 0)
            }
            _ => None,
        }
    }

    /// Visibility of a property list dictionary carrying `/OC`.
    fn property_list_visibility(&self, dict: &Dict, resources: &Dict) -> Option<bool> {
        self.oc_visibility(dict.get("OC")?, resources, 0)
    }

    /// Follow `/OC` values: names through `/Properties`, references through
    /// the document, and inline dictionaries directly.
    fn oc_visibility(&self, value: &Value, resources: &Dict, depth: usize) -> Option<bool> {
        if depth > MAX_OC_DEPTH {
            return None;
        }
        match value {
            Value::Name(name) => {
                let entry = properties_entry(self.document, resources, name)?;
                self.oc_visibility(&entry, resources, depth + 1)
            }
            Value::Ref(object, generation) => {
                self.object_visibility(*object, *generation, resources, depth + 1)
            }
            Value::Dict(dict) => self.dictionary_visibility(dict, resources, depth + 1),
            _ => None,
        }
    }

    /// Visibility of an indirect `/OC` target, which may be an OCG, an OCMD,
    /// or (tolerantly) a property list that itself carries `/OC`.
    fn object_visibility(
        &self,
        object: i64,
        _generation: i64,
        resources: &Dict,
        depth: usize,
    ) -> Option<bool> {
        // 按对象号取最新修订，忽略引用中的代号（增量更新后引用可能写旧代）。
        let resolved = self
            .document
            .get_object(object)
            .ok()
            .flatten()
            .map(|entry| entry.value);
        match resolved {
            Some(Value::Dict(dict)) if dict.get("OC").is_some() => {
                self.dictionary_visibility(&dict, resources, depth)
            }
            Some(Value::Dict(dict)) if is_membership_dictionary(&dict) => {
                self.membership_visible(&dict)
            }
            _ => self.group_visible(object),
        }
    }

    fn dictionary_visibility(&self, dict: &Dict, resources: &Dict, depth: usize) -> Option<bool> {
        if is_membership_dictionary(dict) {
            self.membership_visible(dict)
        } else if let Some(oc) = dict.get("OC") {
            self.oc_visibility(oc, resources, depth)
        } else {
            // Inline OCG dictionary: its object number is unknown.
            None
        }
    }

    fn membership_visible(&self, dict: &Dict) -> Option<bool> {
        self.oc_properties.as_ref()?.membership_visible(dict)
    }

    fn group_visible(&self, object: i64) -> Option<bool> {
        self.oc_properties
            .as_ref()
            .and_then(|properties| properties.is_visible(object))
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
                    match kw.as_str() {
                        "Do" => {
                            if let Some(Operand::Name(name)) = operands.last() {
                                self.draw_form(name, resources)?;
                            }
                        }
                        "BMC" => self.open_marked(name_operand(operands.first()), None, resources),
                        "BDC" => self.open_marked(
                            name_operand(operands.first()),
                            operands.get(1),
                            resources,
                        ),
                        "EMC" => self.state.end_marked(),
                        _ => execute_operator(kw, &operands, &mut self.state),
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
        self.state.flush_fragment();
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

        if self.form_depth >= MAX_FORM_DEPTH {
            return Ok(());
        }
        let active_key = xobject_value.as_ref();
        if let Some(key) = active_key
            && self.active_forms.contains(&key)
        {
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

        // `Do` saves the graphics state (which includes the selected font),
        // runs the form, and restores it afterwards (ISO 32000-1 8.10.1).
        // Font resources are restored as well so a form's same-named font
        // cannot leak into the caller's `Tf` lookups. The form's `/Matrix` is
        // concatenated with the current transformation matrix, and its text
        // matrices start over because text objects cannot span streams.
        let form_matrix = form_matrix(self.document, dict.get("Matrix"));
        let saved_font_name = self.state.font_name.clone();
        let saved_fonts = self.state.fonts.clone();
        // Marked-content sections are lexically scoped to one stream; an
        // unclosed `BMC` in a form must not leak into the caller.
        let saved_marked = std::mem::take(&mut self.state.marked);
        let saved_ctm = self.state.ctm;
        let saved_text_matrix = self.state.text_matrix;
        let saved_line_matrix = self.state.line_matrix;
        self.state.flush_fragment();
        self.state.ctm = form_matrix.concat(saved_ctm);
        self.state.text_matrix = Matrix::IDENTITY;
        self.state.line_matrix = Matrix::IDENTITY;
        if let Some(key) = active_key {
            self.active_forms.insert(key);
        }
        self.form_depth += 1;
        let result = self.extract_from_content(&bytes, &form_resources);
        self.form_depth -= 1;
        if let Some(key) = active_key {
            self.active_forms.remove(&key);
        }
        self.state.flush_fragment();
        self.state.ctm = saved_ctm;
        self.state.text_matrix = saved_text_matrix;
        self.state.line_matrix = saved_line_matrix;
        self.state.font_name = saved_font_name;
        self.state.fonts = saved_fonts;
        self.state.marked = saved_marked;
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
            state.next_line();
            state.add_break(BreakKind::Newline);
            if let Some(bytes) = last_string(operands) {
                state.show_bytes(bytes);
            }
        }
        "\"" => {
            // The first two operands are word and character spacing, which
            // do not affect the run's origin.
            state.next_line();
            state.add_break(BreakKind::Newline);
            if let Some(bytes) = last_string(operands) {
                state.show_bytes(bytes);
            }
        }
        "Td" | "TD" => {
            if let Some([tx, ty]) = trailing_numbers::<2>(operands) {
                if kw == "TD" {
                    state.leading = -ty;
                }
                state.move_line(tx, ty);
            }
            state.add_break(BreakKind::Newline);
        }
        "T*" => {
            state.next_line();
            state.add_break(BreakKind::Newline);
        }
        "Tm" => {
            if let Some(values) = trailing_numbers::<6>(operands) {
                state.set_text_matrix(Matrix::from_values(values));
            }
            state.add_break(BreakKind::Newline);
        }
        "TL" => {
            if let Some([leading]) = trailing_numbers::<1>(operands) {
                state.leading = leading;
            }
        }
        "Ts" => {
            if let Some([rise]) = trailing_numbers::<1>(operands) {
                state.rise = rise;
            }
        }
        "BT" => {
            state.flush_fragment();
            state.begin_text();
        }
        "ET" => state.flush_fragment(),
        "cm" => {
            if let Some(values) = trailing_numbers::<6>(operands) {
                state.concatenate_ctm(Matrix::from_values(values));
            }
        }
        "q" => state.save_state(),
        "Q" => state.restore_state(),
        _ => {}
    }
}

fn name_operand(operand: Option<&Operand>) -> Option<&str> {
    match operand {
        Some(Operand::Name(name)) => Some(name.as_str()),
        _ => None,
    }
}

/// Parse the trailing `N` operands as numbers (`Td`, `Tm`, `cm`, `TL`, `Ts`).
fn trailing_numbers<const N: usize>(operands: &[Operand]) -> Option<[f64; N]> {
    if operands.len() < N {
        return None;
    }
    let mut values = [0.0; N];
    for (slot, operand) in values.iter_mut().zip(&operands[operands.len() - N..]) {
        let Operand::Number(number) = operand else {
            return None;
        };
        *slot = *number;
    }
    Some(values)
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
            let to_unicode = document.simple_font_to_unicode(&font_val);
            if font.is_some() || encoding.is_some() || font_val.as_dict().is_some() {
                state.fonts.insert(
                    name.clone(),
                    FontHandle::Simple {
                        font,
                        encoding,
                        builtin,
                        to_unicode,
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
        // 按对象号取最新修订，忽略引用中的代号（增量更新后引用可能写旧代）。
        Value::Ref(num, _) => document.get_object(*num).ok().flatten().map(|o| o.value),
        _ => Some(value.clone()),
    }
}

/// Read a form XObject's `/Matrix`, falling back to the identity matrix.
fn form_matrix(document: &Document, value: Option<&Value>) -> Matrix {
    let Some(value) = value.and_then(|value| resolve_value(document, value)) else {
        return Matrix::IDENTITY;
    };
    let Some(array) = value.as_array() else {
        return Matrix::IDENTITY;
    };
    if array.len() != 6 {
        return Matrix::IDENTITY;
    }
    let mut values = [0.0; 6];
    for (slot, item) in values.iter_mut().zip(array.iter()) {
        let Some(item) = resolve_value(document, item) else {
            return Matrix::IDENTITY;
        };
        *slot = match item {
            Value::Integer(number) => number as f64,
            Value::Float(number) => number,
            _ => return Matrix::IDENTITY,
        };
    }
    Matrix::from_values(values)
}

/// The raw `/Properties` resource entry a `BDC` property *name* operand
/// refers to (ISO 32000-1 14.6.2), before dereferencing.
fn properties_entry(document: &Document, resources: &Dict, name: &str) -> Option<Value> {
    let properties = resources
        .get("Properties")
        .and_then(|value| resolve_value(document, value))?;
    properties.as_dict()?.get(name).cloned()
}

/// Resolve a `BDC` property *name* operand to its property list dictionary
/// through the `/Properties` resource (ISO 32000-1 14.6.2).
fn resolve_property_list(document: &Document, resources: &Dict, name: &str) -> Option<Dict> {
    let entry = properties_entry(document, resources, name)?;
    resolve_value(document, &entry).and_then(|value| value.as_dict().cloned())
}

/// Whether a dictionary is an optional content membership dictionary
/// (8.11.2.2, Table 100) rather than an OCG or a property list.
fn is_membership_dictionary(dict: &Dict) -> bool {
    dict.get("Type").and_then(Value::as_name) == Some("OCMD")
        || dict.get("OCGs").is_some()
        || dict.get("VE").is_some()
}

fn is_ocg_dictionary(dict: &Dict) -> bool {
    dict.get("Type").and_then(Value::as_name) == Some("OCG")
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

    /// Extract positioned text runs from a single content stream.
    ///
    /// The same decoding rules as [`Document::extract_text`] apply, but the
    /// output is split into runs carrying the text-space origin in default
    /// user space (see [`TextFragment`]).
    pub fn extract_text_fragments(
        &self,
        content_bytes: &[u8],
        resources: &Dict,
    ) -> Vec<TextFragment> {
        let mut context = ExtractionContext::new(self);
        let _ = context.extract_from_content(content_bytes, resources);
        let mut state = context.state;
        state.flush_fragment();
        state.fragments
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
        self.extract_page_state(page_index)
            .map(|state| state.output)
    }

    /// Positioned variant of [`Document::try_extract_page_text`].
    ///
    /// Returns one [`TextFragment`] per contiguous text run; see
    /// [`Document::extract_text_fragments`] for the position model.
    pub fn try_extract_page_text_fragments(
        &self,
        page_index: usize,
    ) -> Result<Vec<TextFragment>, TextExtractionError> {
        self.extract_page_state(page_index)
            .map(|state| state.fragments)
    }

    /// Infallible variant of [`Document::try_extract_page_text_fragments`].
    pub fn extract_page_text_fragments(&self, page_index: usize) -> Option<Vec<TextFragment>> {
        self.try_extract_page_text_fragments(page_index).ok()
    }

    /// Page-tree walk plus content extraction shared by the text and fragment
    /// entry points.
    fn extract_page_state(&self, page_index: usize) -> Result<TextState, TextExtractionError> {
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
        let mut state = context.state;
        state.flush_fragment();
        Ok(state)
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
