use ttf_parser::{Face, GlyphId, OutlineBuilder};

use super::outline::{CffGlyphOutline, CffOutlineParser, CffPathSegment};
use crate::core::reader::font::{FontError, FontResult};

/// Adapter that implements `CffOutlineParser` using the `ttf-parser` crate.
///
/// This isolates the third-party dependency behind the trait boundary,
/// allowing alternative implementations without changing call sites.
pub struct TtfParserCffBackend {
    data: Vec<u8>,
}

impl TtfParserCffBackend {
    /// Create a new backend from raw CFF font bytes.
    pub fn new(data: Vec<u8>) -> Self {
        Self { data }
    }
}

struct PathBuilder {
    segments: Vec<CffPathSegment>,
}

impl PathBuilder {
    fn new() -> Self {
        Self {
            segments: Vec::new(),
        }
    }
}

impl OutlineBuilder for PathBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        self.segments.push(CffPathSegment::MoveTo { x, y });
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.segments.push(CffPathSegment::LineTo { x, y });
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        // CFF only has cubic curves; convert quadratic to cubic.
        // This is a rare case in CFF but ttf-parser may emit it.
        let x1 = (2.0 / 3.0) * x1 + (1.0 / 3.0) * 0.0;
        let y1 = (2.0 / 3.0) * y1 + (1.0 / 3.0) * 0.0;
        let x2 = (2.0 / 3.0) * x1 + (1.0 / 3.0) * x;
        let y2 = (2.0 / 3.0) * y1 + (1.0 / 3.0) * y;
        self.segments.push(CffPathSegment::CurveTo {
            x1,
            y1,
            x2,
            y2,
            x,
            y,
        });
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.segments.push(CffPathSegment::CurveTo {
            x1,
            y1,
            x2,
            y2,
            x,
            y,
        });
    }

    fn close(&mut self) {
        self.segments.push(CffPathSegment::ClosePath);
    }
}

impl CffOutlineParser for TtfParserCffBackend {
    fn parse_glyph(&self, gid: u16) -> FontResult<CffGlyphOutline> {
        let face = Face::parse(&self.data, 0)
            .map_err(|e| FontError::Invalid(format!("failed to parse CFF font: {e}")))?;

        let glyph_id = GlyphId(gid);
        let width = face
            .glyph_hor_advance(glyph_id)
            .ok_or_else(|| FontError::Invalid(format!("glyph {gid} has no horizontal advance")))?
            as f64;

        let mut builder = PathBuilder::new();
        face.outline_glyph(glyph_id, &mut builder)
            .ok_or_else(|| FontError::Invalid(format!("glyph {gid} has no outline")))?;

        Ok(CffGlyphOutline {
            width,
            segments: builder.segments,
        })
    }

    fn glyph_count(&self) -> u16 {
        Face::parse(&self.data, 0)
            .map(|f| f.number_of_glyphs())
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Mock backend for testing the trait boundary (engineering convention #3).
    struct MockCffBackend {
        glyph_count: u16,
    }

    impl CffOutlineParser for MockCffBackend {
        fn parse_glyph(&self, gid: u16) -> FontResult<CffGlyphOutline> {
            if gid >= self.glyph_count {
                return Err(FontError::Invalid(format!("glyph {gid} out of range")));
            }
            Ok(CffGlyphOutline {
                width: 500.0,
                segments: vec![
                    CffPathSegment::MoveTo { x: 0.0, y: 0.0 },
                    CffPathSegment::LineTo { x: 100.0, y: 0.0 },
                    CffPathSegment::LineTo { x: 100.0, y: 100.0 },
                    CffPathSegment::ClosePath,
                ],
            })
        }

        fn glyph_count(&self) -> u16 {
            self.glyph_count
        }
    }

    #[test]
    fn mock_backend_satisfies_trait_boundary() {
        let mock = MockCffBackend { glyph_count: 10 };
        assert_eq!(mock.glyph_count(), 10);
        let outline = mock.parse_glyph(0).unwrap();
        assert_eq!(outline.width, 500.0);
        assert_eq!(outline.segments.len(), 4);
    }

    #[test]
    fn mock_backend_rejects_out_of_range_glyph() {
        let mock = MockCffBackend { glyph_count: 5 };
        assert!(mock.parse_glyph(10).is_err());
    }
}
