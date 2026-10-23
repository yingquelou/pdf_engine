use crate::core::reader::font::FontError;

/// A single segment in a CFF glyph outline.
#[derive(Debug, Clone, PartialEq)]
pub enum CffPathSegment {
    /// Move to absolute position (x, y).
    MoveTo { x: f32, y: f32 },
    /// Line to absolute position (x, y).
    LineTo { x: f32, y: f32 },
    /// Cubic Bezier curve with two control points and an endpoint.
    CurveTo {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        x: f32,
        y: f32,
    },
    /// Close the current sub-path.
    ClosePath,
}

/// Parsed glyph outline with width and segments.
#[derive(Debug, Clone, PartialEq)]
pub struct CffGlyphOutline {
    /// Glyph width in glyph-space units (typically 1000 units per em).
    pub width: f64,
    /// Path segments defining the glyph outline.
    pub segments: Vec<CffPathSegment>,
}

/// Trait for parsing CFF font outlines.
///
/// Implementations must handle CFF version 1 charstrings, subroutines,
/// and global subroutines. This trait abstracts the parsing backend so
/// that alternative implementations (e.g., custom parsers) can be substituted.
pub trait CffOutlineParser: Send + Sync {
    /// Parse a glyph outline by its glyph ID (GID).
    ///
    /// Returns the glyph width and path segments, or an error if the glyph
    /// cannot be parsed.
    fn parse_glyph(&self, gid: u16) -> Result<CffGlyphOutline, FontError>;

    /// Return the number of glyphs in the font.
    fn glyph_count(&self) -> u16;
}
