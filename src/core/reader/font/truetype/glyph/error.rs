use crate::core::reader::font::FontError;

pub(super) const MAX_COMPOSITE_DEPTH: usize = 64;
pub(super) const MAX_EXPANDED_POINTS: usize = 1_000_000;

pub(super) fn invalid_glyph(message: &str) -> FontError {
    FontError::Invalid(format!("invalid TrueType glyph data: {message}"))
}
