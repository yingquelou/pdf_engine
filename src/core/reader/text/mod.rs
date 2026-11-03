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
//! their property lists (inline dictionary, `/Properties` name, or — as
//! interoperability tolerance for producers that write it, including this
//! crate's own content-stream builder — an indirect reference operand
//! `num gen R`), a section whose optional-content group (8.11) is off by the
//! default configuration is not extracted, `/ActualText` replaces the
//! section's shown text, a `/ReversedChars` section (14.8.2.3.3) has each shown
//! string's characters reversed back to natural order, and `/Artifact` sections
//! (14.8.2.2.1) are skipped entirely. Unbalanced `EMC` is ignored.
//!
//! [`Document::try_extract_page_text_fragments`] returns the same text split
//! into positioned runs ([`TextFragment`]), tracking the text matrix and the
//! current transformation matrix instead of only inserting separators.
//!
//! The same walk keeps the `Tf` font size in the text state and records, for
//! every font that draws a show string, the show-time typeface height (that
//! size scaled by the vertical factor of the text matrix) so 14.8.2.4.3
//! Table 332 `FontSize` can be derived; see the crate-internal
//! `Document::page_font_effective_sizes`.
//!
//! The extractor uses an operand stack: non-operator tokens (numbers,
//! strings, names, arrays) are pushed onto the stack; when an operator
//! keyword is encountered, it consumes the relevant operands. This matches
//! the PDF content stream model where operands precede their operator.
//! The lone exception is the `num gen R` triplet, folded back into a single
//! `Operand::Reference` by `state::fold_reference_operand` rather than treated
//! as an operator.

use crate::core::types::{Dict, Document};

pub mod tokenizer;

mod context;
mod document;
mod error;
mod fonts;
mod matrix;
mod oc_visibility;
mod operator;
mod resolve;
mod state;

use context::ExtractionContext;

pub use error::TextExtractionError;
pub use state::TextFragment;

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

#[cfg(test)]
mod tests;
