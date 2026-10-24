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
    /// Glyph width in glyph-space units (typographically derived from the
    /// font's `unitsPerEm`, typically 1000 units per em).
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
    /// Parse a glyph outline by the identifier expected by the backend.
    ///
    /// For name-keyed CFF programs the argument is a glyph ID (GID). For
    /// CID-keyed programs ([`Self::is_cid_keyed`] returns `true`) it is a CID,
    /// which the backend maps to a GID first. Parsers that consume
    /// CIDFontType0C data therefore follow the CIDFontType0 semantics where
    /// glyphs are selected by CID.
    ///
    /// Returns the glyph width and path segments, or an error if the glyph
    /// cannot be parsed.
    fn parse_glyph(&self, gid: u16) -> Result<CffGlyphOutline, FontError>;

    /// Return the number of glyphs in the font.
    fn glyph_count(&self) -> u16;

    /// Return whether [`Self::parse_glyph`] takes a CID instead of a GID.
    ///
    /// CID-keyed CFF programs map CIDs to glyphs through their charset.
    fn is_cid_keyed(&self) -> bool {
        false
    }

    /// Map a CID to its glyph ID.
    ///
    /// Returns `None` for name-keyed programs and for CIDs that the
    /// CID-keyed charset does not contain.
    fn glyph_for_cid(&self, _cid: u16) -> Option<u16> {
        None
    }
}

/// A backend that reports a construction failure from every outline request.
///
/// This keeps accessors that cannot return errors (`CidFontType0::cff_outline_parser`)
/// infallible while still surfacing the deferred parse failure on use. Prefer
/// the fallible accessor (for example `CidFontType0::try_cff_outline_parser`)
/// when the error needs to be handled before outlining.
#[derive(Debug, Clone)]
pub struct UnavailableCffBackend {
    message: String,
}

impl UnavailableCffBackend {
    /// Create a backend that reports `message` from every outline request.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl CffOutlineParser for UnavailableCffBackend {
    fn parse_glyph(&self, _gid: u16) -> Result<CffGlyphOutline, FontError> {
        Err(FontError::Invalid(self.message.clone()))
    }

    fn glyph_count(&self) -> u16 {
        0
    }
}
