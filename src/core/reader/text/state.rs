//! Operand stack, font handles, marked-content frames and the per-page
//! [`TextState`] accumulation machine that decodes shown strings.

use std::collections::{BTreeMap, HashMap};

use crate::core::reader::content::parse_dictionary_inner;
use crate::core::types::Dict;

use super::super::font::{SimpleFontEncoding, SimpleFontToUnicode, Type0Font, Type1Font};
use super::MAX_STATE_DEPTH;
use super::matrix::Matrix;
use super::tokenizer::CsToken;

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

#[derive(Debug, Clone)]
pub(super) enum Operand {
    Number(f64),
    String(Vec<u8>),
    Name(String),
    Array(Vec<Operand>),
    Dict(Dict),
}

pub(super) fn token_to_operand(token: &CsToken) -> Option<Operand> {
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
pub(super) enum FontHandle {
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
pub(super) struct MarkedFrame {
    /// `true` when this section (or an enclosing one) is an `/Artifact` or
    /// belongs to an optional-content group that the default configuration
    /// turns off.
    hidden: bool,
    /// `true` when this section (or an enclosing one) carries the
    /// `/ReversedChars` tag (14.8.2.3.3): each shown string's characters
    /// are in reverse page-content order and are restored on output.
    reversed: bool,
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
    font_size: Option<f64>,
    ctm: Matrix,
    leading: f64,
    rise: f64,
}

pub(super) struct TextState {
    pub(super) font_name: Option<String>,
    /// Font size operand of the last `Tf` (text space units, 9.3.4), or `None`
    /// when the operand was absent or not a number.
    pub(super) font_size: Option<f64>,
    pub(super) fonts: HashMap<String, FontHandle>,
    pub(super) output: String,
    pub(super) fragments: Vec<TextFragment>,
    /// Table 332 `FontSize`: the typeface height of every font that actually
    /// showed a show string, keyed by font resource name (see
    /// [`TextState::record_font_size`]).
    pub(super) font_sizes: BTreeMap<String, f64>,
    /// Current run being accumulated; flushed when the position changes or a
    /// separator is emitted.
    pending_fragment: Option<TextFragment>,
    /// States saved by `q` and restored by `Q`. Only the selected font and the
    /// text parameters that affect positioning are modeled.
    saved_states: Vec<SavedState>,
    /// Open marked-content sections (`BMC`/`BDC` … `EMC` nesting).
    pub(super) marked: Vec<MarkedFrame>,
    /// Current transformation matrix (`cm`, `q`/`Q`, form `/Matrix`).
    pub(super) ctm: Matrix,
    /// Text matrix `Tm` and text line matrix `Tlm` (9.4.2).
    pub(super) text_matrix: Matrix,
    pub(super) line_matrix: Matrix,
    /// Leading `TL` and text rise `Ts`.
    pub(super) leading: f64,
    pub(super) rise: f64,
}

impl TextState {
    pub(super) fn new() -> Self {
        Self {
            font_name: None,
            font_size: None,
            fonts: HashMap::new(),
            output: String::new(),
            fragments: Vec::new(),
            font_sizes: BTreeMap::new(),
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

    pub(super) fn begin_marked(
        &mut self,
        hidden: bool,
        reversed: bool,
        actual_text: Option<String>,
    ) {
        let parent_hidden = self.marked.last().is_some_and(|frame| frame.hidden);
        let parent_reversed = self.marked.last().is_some_and(|frame| frame.reversed);
        self.marked.push(MarkedFrame {
            hidden: parent_hidden || hidden,
            reversed: parent_reversed || reversed,
            actual_text,
            actual_emitted: false,
        });
    }

    /// Close the innermost section; unbalanced `EMC` is ignored.
    pub(super) fn end_marked(&mut self) {
        self.marked.pop();
    }

    fn marked_hidden(&self) -> bool {
        self.marked.last().is_some_and(|frame| frame.hidden)
    }

    pub(super) fn set_font(&mut self, name: &str, size: Option<f64>) {
        self.font_size = size;
        if self.fonts.contains_key(name) {
            self.font_name = Some(name.to_owned());
        }
    }

    pub(super) fn save_state(&mut self) {
        if self.saved_states.len() < MAX_STATE_DEPTH {
            self.saved_states.push(SavedState {
                font_name: self.font_name.clone(),
                font_size: self.font_size,
                ctm: self.ctm,
                leading: self.leading,
                rise: self.rise,
            });
        }
    }

    pub(super) fn restore_state(&mut self) {
        if let Some(saved) = self.saved_states.pop() {
            self.font_name = saved.font_name;
            self.font_size = saved.font_size;
            self.ctm = saved.ctm;
            self.leading = saved.leading;
            self.rise = saved.rise;
        }
    }

    /// `BT` resets both text matrices; text state parameters are preserved.
    pub(super) fn begin_text(&mut self) {
        self.text_matrix = Matrix::IDENTITY;
        self.line_matrix = Matrix::IDENTITY;
    }

    pub(super) fn set_text_matrix(&mut self, matrix: Matrix) {
        self.text_matrix = matrix;
        self.line_matrix = matrix;
    }

    /// `Td`/`TD`/`T*`: move the line matrix and make it the text matrix.
    pub(super) fn move_line(&mut self, tx: f64, ty: f64) {
        self.line_matrix = Matrix::translate(tx, ty).concat(self.line_matrix);
        self.text_matrix = self.line_matrix;
    }

    pub(super) fn next_line(&mut self) {
        self.move_line(0.0, -self.leading);
    }

    /// `cm`: concatenate a matrix with the current transformation matrix.
    pub(super) fn concatenate_ctm(&mut self, matrix: Matrix) {
        self.ctm = matrix.concat(self.ctm);
    }

    /// Text-space origin of the next shown run in default user space.
    fn position(&self) -> (f64, f64) {
        self.text_matrix.concat(self.ctm).transform(0.0, self.rise)
    }

    /// Table 332 `FontSize` for the current show: the font size set by `Tf`
    /// (text space units, 9.3.4) scaled by the vertical factor of the current
    /// text matrix `Tm`.
    ///
    /// ISO 32000-1 Table 332 words the derivation as "derived from the a, b,
    /// c, and d fields of the current text matrix" and requires "a positive
    /// number specifying the height of the typeface in points". The `a`/`b`/`c`/`d`
    /// fields alone are not the height: 9.4.4 scales the glyph outline by the
    /// `Tf` size *inside* text space, so a writer that emits
    /// `1 0 0 1 x y Tm /F1 12 Tf` (the common idiom) would yield a `FontSize`
    /// of `1.0` under a literal reading. This library therefore multiplies the
    /// two factors and uses `hypot(c, d)` — the image of the text-space y axis,
    /// i.e. the height direction, so a pure rotation keeps the size and
    /// anisotropic `Tm` scaling follows the height rather than the width. The
    /// value is magnitude only (a negative size or matrix mirrors the glyphs
    /// without changing the height) and is not derivable — hence `None` — when
    /// either factor is absent, zero or non-finite, because the table requires
    /// a positive number. The CTM (`cm`, `q`/`Q`, a form's `/Matrix`) is
    /// deliberately **not** folded in: the table names the text matrix.
    fn effective_font_size(&self) -> Option<f64> {
        let size = self.font_size?;
        let scale = self.text_matrix.vertical_scale();
        let height = (size * scale).abs();
        (height > 0.0 && height.is_finite()).then_some(height)
    }

    /// Register the show-time `FontSize` of the font in the text state.
    ///
    /// Called only for decoded show strings: 14.8.2.4.3 states the
    /// characteristics "do not exist for alternate description text (`/Alt`),
    /// replacement text (`/ActualText`), or abbreviation expansion text (`/E`)",
    /// so a section whose shown text was replaced never registers a size. A
    /// font shown at several sizes on one page keeps the largest observed
    /// value because the map is keyed by resource name only.
    fn record_font_size(&mut self) {
        let Some(name) = self.font_name.clone() else {
            return;
        };
        let Some(size) = self.effective_font_size() else {
            return;
        };
        let current = self.font_sizes.entry(name).or_insert(size);
        *current = current.max(size);
    }

    pub(super) fn show_bytes(&mut self, bytes: &[u8]) {
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
        let mut text = self.decode_text(bytes);
        if text.is_empty() {
            return;
        }
        // 14.8.2.3.3：`ReversedChars` 序列内的显示串按字符反序存储，
        // 逐串反转回自然顺序；串与串之间的自然顺序不变。
        if self.marked.last().is_some_and(|frame| frame.reversed) {
            text = text.chars().rev().collect();
        }
        self.output.push_str(&text);
        self.record_font_size();
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

    pub(super) fn flush_fragment(&mut self) {
        if let Some(fragment) = self.pending_fragment.take() {
            self.fragments.push(fragment);
        }
    }

    pub(super) fn add_break(&mut self, kind: BreakKind) {
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

pub(super) enum BreakKind {
    Space,
    Newline,
}

fn lossy_latin1(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| b as char).collect()
}

/// Whether `ch` lies in the Unicode Private Use Area (U+E000–U+F8FF).
pub(super) fn is_private_use(ch: char) -> bool {
    matches!(ch, '\u{E000}'..='\u{F8FF}')
}

/// Resolve the text for one byte of a simple font.
///
/// A symbolic TrueType cmap maps codes into the Private Use Area, so a
/// predefined or `/Differences` encoding is preferred over a PUA built-in
/// character; a regular built-in character still wins over the encoding.
/// Returns `None` when neither source produces text.
pub(super) fn simple_byte_text(
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
