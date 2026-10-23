//! CFF (Compact Font Format) outline parsing.
//!
//! This module provides CFF charstring interpretation for CIDFontType0 and
//! FontFile3 fonts. The parsing backend is abstracted behind the
//! [`CffOutlineParser`] trait, with a default implementation using `ttf-parser`.

mod outline;
mod ttf_parser;

pub use outline::{CffGlyphOutline, CffOutlineParser, CffPathSegment};
pub use ttf_parser::TtfParserCffBackend;
