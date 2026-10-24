mod cmap;
mod glyph;
mod sfnt;
mod subset;
mod tables;

pub use cmap::CmapFormat;
pub use glyph::{GlyphPoint, TrueTypeGlyph};
pub use sfnt::{SfntFlavor, TrueTypeFont};
pub use subset::subset_truetype;

use crate::core::types::{Document, Value};

use super::FontResult;

impl Document {
    /// Resolve, decode, and validate an embedded TrueType font program.
    ///
    /// The program may be a `/FontFile2` stream or an OpenType program stored
    /// as `/FontFile3` with stream `/Subtype /OpenType` (ISO 32000-1 §9.9).
    /// Returns `Ok(None)` when the value is not a TrueType font or has no
    /// embedded program. The parsed font exposes SFNT tables, metrics, and
    /// character mapping; TrueType-flavored programs also expose expanded
    /// glyph contours, while CFF-flavored (`OTTO`) programs without
    /// `loca`/`glyf` return no outlines. Nothing is rendered.
    pub fn truetype_font(&self, value: &Value) -> FontResult<Option<TrueTypeFont>> {
        let Some(program) = self.truetype_font_program(value)? else {
            return Ok(None);
        };
        TrueTypeFont::parse(program.data).map(Some)
    }
}
