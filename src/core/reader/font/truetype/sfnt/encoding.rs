//! `cmap` 派生的映射接口：码位↔字形查询、反向映射与内置单字节编码。

use super::super::cmap::CmapFormat;
use super::{FontError, TrueTypeFont, invalid_sfnt};

impl TrueTypeFont {
    /// Return the selected Unicode character-map subtable format.
    pub fn cmap_format(&self) -> CmapFormat {
        self.cmap.format()
    }

    /// Map a Unicode codepoint to a glyph ID.
    ///
    /// Format 4 supports BMP codepoints; format 12 also supports supplementary
    /// Unicode planes. A codepoint absent from the selected map returns `None`.
    pub fn glyph_id_for_unicode(&self, codepoint: u32) -> Result<Option<u16>, FontError> {
        if char::from_u32(codepoint).is_none() {
            return Ok(None);
        }
        let cmap_data = self
            .table(b"cmap")
            .ok_or_else(|| invalid_sfnt("missing required `cmap` table"))?;
        self.cmap
            .glyph_id(cmap_data, codepoint, self.metadata.glyph_count)
    }

    /// Map a glyph ID to its Unicode codepoint using the font's built-in encoding.
    ///
    /// Reverses the cmap to find which Unicode character a glyph represents.
    /// When multiple codepoints map to the same glyph, returns the first encountered.
    /// Returns `None` for glyph ID 0 (the .notdef glyph) or unmapped glyphs.
    pub fn unicode_for_glyph(&self, glyph_id: u16) -> Result<Option<u32>, FontError> {
        if glyph_id == 0 {
            return Ok(None);
        }
        let cmap_data = self
            .table(b"cmap")
            .ok_or_else(|| invalid_sfnt("missing required `cmap` table"))?;
        let reverse = self
            .cmap
            .reverse_cmap(cmap_data, self.metadata.glyph_count)?;
        Ok(reverse.get(&glyph_id).copied())
    }

    /// Return the built-in single-byte encoding derived from the `cmap` table.
    ///
    /// Entry `i` is the Unicode scalar for character code `i` (0–255), resolved
    /// through glyph ID the same way PDF maps character codes for TrueType fonts
    /// without an `/Encoding` entry (ISO 32000-1 §9.6.6.4, §9.10.2). Codes with
    /// no glyph, or glyphs with no reverse cmap entry, are `None`.
    ///
    /// Symbolic (3, 0) subtables commonly address their glyphs through one of
    /// three bases above the ASCII range: `0xF000`, `0xF100`, or `0xF200`.
    /// When the direct lookup for a code fails, those offsets are probed in
    /// that order and the first mapped glyph wins, so the widespread `0xF000`
    /// convention keeps priority over the `0xF100` and `0xF200` variants.
    pub fn builtin_encoding(&self) -> Result<Vec<Option<char>>, FontError> {
        let cmap_data = self
            .table(b"cmap")
            .ok_or_else(|| invalid_sfnt("missing required `cmap` table"))?;
        let reverse = self
            .cmap
            .reverse_cmap(cmap_data, self.metadata.glyph_count)?;
        let mut encoding = vec![None; 256];
        for code in 0..=u8::MAX {
            let glyph =
                match self
                    .cmap
                    .glyph_id(cmap_data, u32::from(code), self.metadata.glyph_count)?
                {
                    Some(glyph) => Some(glyph),
                    None if self.cmap.is_symbolic() => self.symbolic_glyph(cmap_data, code)?,
                    None => None,
                };
            if let Some(glyph) = glyph {
                encoding[usize::from(code)] =
                    reverse.get(&glyph).and_then(|cp| char::from_u32(*cp));
            }
        }
        Ok(encoding)
    }

    /// Resolve a symbolic character code through the common `0xF000`-based
    /// offsets, trying `0xF000`, `0xF100`, then `0xF200` and stopping at the
    /// first mapped glyph.
    fn symbolic_glyph(&self, cmap_data: &[u8], code: u8) -> Result<Option<u16>, FontError> {
        for base in [0xF000u32, 0xF100, 0xF200] {
            let glyph =
                self.cmap
                    .glyph_id(cmap_data, base | u32::from(code), self.metadata.glyph_count)?;
            if glyph.is_some() {
                return Ok(glyph);
            }
        }
        Ok(None)
    }
}
