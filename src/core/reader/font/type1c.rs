//! Parsing of name-keyed CFF programs embedded in simple Type 1 fonts.
//!
//! ISO 32000-1 §9.9 stores a compact Type 1 font program in a
//! `/FontFile3` stream whose `/Subtype` is `/Type1C`. The program is a bare
//! CFF1 font without an SFNT wrapper; [`RawCffFont`] parses it and exposes
//! glyph names, widths and outlines by glyph ID. CID-keyed CFF programs use
//! `/CIDFontType0C` and belong to CIDFontType0 fonts instead.

use super::cff::RawCffFont;
use super::{FontError, FontProgramSource, FontResult};
use crate::core::types::{Document, Value};

impl Document {
    /// Parse the name-keyed CFF program of a simple Type 1 font.
    ///
    /// Resolves the font's `/FontDescriptor /FontFile3` stream when its
    /// `/Subtype` is `/Type1C`. The returned [`RawCffFont`] exposes glyph
    /// names ([`RawCffFont::glyph_name`], [`RawCffFont::glyph_for_name`]),
    /// widths and outlines by glyph ID.
    ///
    /// Returns `Ok(None)` for non-Type-1 fonts, fonts without an embedded
    /// program, and Type 1 fonts that embed a `/FontFile` program (use
    /// [`Document::type1_program`] for those). A `/FontFile3` stream with a
    /// different `/Subtype`, a CID-keyed CFF program, and a malformed program
    /// are reported as errors.
    pub fn type1c_program(&self, value: &Value) -> FontResult<Option<RawCffFont>> {
        let Some(program) = self.type1_font_program(value)? else {
            return Ok(None);
        };
        if program.source != FontProgramSource::FontFile3 {
            return Ok(None);
        }
        if program.subtype.as_deref() != Some("Type1C") {
            return Err(FontError::Invalid(format!(
                "/Subtype /Type1 /FontFile3 stream requires /Subtype /Type1C, found /{}",
                program.subtype.as_deref().unwrap_or_default()
            )));
        }
        let font = RawCffFont::parse(program.data)?;
        if font.is_cid_keyed() {
            return Err(FontError::Invalid(
                "simple Type 1 /FontFile3 /Type1C program is CID-keyed; use a CIDFontType0 font for /CIDFontType0C".into(),
            ));
        }
        Ok(Some(font))
    }
}
