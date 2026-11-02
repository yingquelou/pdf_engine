//! `loca`/`glyf` 字形轮廓访问。

use super::super::glyph::TrueTypeGlyph;
use super::{FontError, TrueTypeFont, invalid_sfnt};

impl TrueTypeFont {
    /// Return a glyph record and its header bounds by glyph ID.
    ///
    /// Empty glyphs and IDs outside the font's glyph range return `None`.
    /// Composite glyph components are recursively expanded into the returned
    /// contour points; `data` retains the original encoded record.
    ///
    /// A CFF-flavored OpenType program (`OTTO`) without `loca`/`glyf` tables
    /// has no TrueType outlines, so every lookup returns `None`.
    pub fn glyph(&self, glyph_id: u16) -> Result<Option<TrueTypeGlyph<'_>>, FontError> {
        let Some(glyph_locations) = &self.glyph_locations else {
            return Ok(None);
        };
        let glyf = self
            .table(b"glyf")
            .ok_or_else(|| invalid_sfnt("missing required `glyf` table"))?;
        glyph_locations.glyph(glyf, glyph_id)
    }
}
