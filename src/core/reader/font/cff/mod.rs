//! CFF (Compact Font Format) outline parsing.
//!
//! This module provides CFF charstring interpretation for CIDFontType0 and
//! FontFile3 fonts. The parsing backend is abstracted behind the
//! [`CffOutlineParser`] trait:
//!
//! * [`TtfParserCffBackend`] for SFNT (`OTTO`) programs, using `ttf-parser`;
//! * [`RawCffBackend`] for bare CFF1 programs (`/CIDFontType0C`, `/Type1C`),
//!   which parses the CFF structure and wraps it in a synthetic face.

mod outline;
mod raw;
mod strings;
mod subset;
mod ttf_parser;

#[cfg(test)]
pub(crate) mod fixtures;
#[cfg(test)]
pub(crate) mod name_keyed_fixtures;

pub use outline::{CffGlyphOutline, CffOutlineParser, CffPathSegment, UnavailableCffBackend};
pub use raw::{RawCffBackend, RawCffFont};
pub use subset::{subset_cff, subset_cff_type1c};
pub use ttf_parser::TtfParserCffBackend;
