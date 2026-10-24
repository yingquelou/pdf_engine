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
    /// Returns `Ok(None)` when the value is not a TrueType font or has no
    /// embedded program. The parsed font exposes SFNT tables and expanded
    /// glyph contours, but does not render text.
    pub fn truetype_font(&self, value: &Value) -> FontResult<Option<TrueTypeFont>> {
        let Some(program) = self.truetype_font_program(value)? else {
            return Ok(None);
        };
        TrueTypeFont::parse(program.data).map(Some)
    }
}
