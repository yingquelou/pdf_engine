//! TrueType glyph subsetting for embedded font programs (ISO 32000-1 §9.6.4).
//!
//! [`subset_truetype`] keeps glyph 0, the requested glyph IDs, and every
//! glyph reachable from them through composite-glyph component references.
//! Retained glyphs are renumbered in ascending original-ID order, so glyph 0
//! remains glyph 0 in the subset.
//!
//! The following tables are rewritten: `head`, `hhea`, `maxp`, `hmtx`,
//! `cmap`, `loca`, `glyf`, `vhea`, and `vmtx`. The character map keeps a
//! format 4 subtable for BMP codes, recorded as Unicode BMP or as Microsoft
//! Symbol when the source used that encoding, and appends a format 12
//! subtable when supplementary-plane mappings reach retained glyphs. Vertical
//! metrics are rebuilt for the new glyph IDs when the source carries both
//! `vhea` and `vmtx`: `numberOfVMetrics` is clamped to the retained glyph
//! count, the trailing top-side-bearing array is regenerated, and the
//! `advanceHeightMax`, `minTopSideBearing`, `minBottomSideBearing`, and
//! `yMaxExtent` summaries are recomputed from the retained glyphs. The
//! remaining `vhea` fields stay as they were in the source.
//!
//! The table whitelist is `head` `hhea` `maxp` `hmtx` `cmap` `loca` `glyf`
//! `vhea` `vmtx` `name` `OS/2` `cvt ` `fpgm` `prep` `gasp`. The first nine
//! are rewritten as described above; `name`, `OS/2`, and the hinting tables
//! are copied verbatim when present. A `vhea`/`vmtx` pair is dropped when
//! only one of the two is present, because both tables are optional. Tables
//! that contain glyph IDs, glyph counts, or glyph-indexed data are dropped:
//! `post`, `kern`, `GDEF`, `GSUB`, and `GPOS`, along with every other table
//! outside the whitelist.

use super::super::FontError;

mod cmap;
mod directory;
mod glyph;
mod metrics;
mod source;

#[cfg(test)]
mod tests;

use directory::assemble_sfnt;
use source::SourceFont;

/// Signature of a standard TrueType-outline SFNT.
const TRUETYPE_SIGNATURE: [u8; 4] = [0, 1, 0, 0];
/// Signature of an Apple TrueType-outline SFNT.
const APPLE_TRUETYPE_SIGNATURE: [u8; 4] = *b"true";
const HEAD_MAGIC: u32 = 0x5F0F_3CF5;
const CHECKSUM_MAGIC: u32 = 0xB1B0_AFBA;
const MAXP_VERSION_1_0: u32 = 0x0001_0000;
const MAXP_VERSION_0_5: u32 = 0x0000_5000;
/// Largest byte offset representable by a short `loca` entry.
const MAX_SHORT_LOCA_OFFSET: usize = 0x1FFFE;

/// Tables that are kept when present; rewritten tables are rebuilt instead.
const PRESERVED_TABLES: [&[u8; 4]; 15] = [
    b"head", b"hhea", b"maxp", b"hmtx", b"cmap", b"loca", b"glyf", b"vhea", b"vmtx", b"name",
    b"OS/2", b"cvt ", b"fpgm", b"prep", b"gasp",
];

/// Tables rebuilt from the subset glyph set.
const REWRITTEN_TABLES: [&[u8; 4]; 9] = [
    b"head", b"hhea", b"maxp", b"hmtx", b"cmap", b"loca", b"glyf", b"vhea", b"vmtx",
];

/// Build a TrueType subset containing `glyph_ids` and their composite closure.
///
/// Glyph 0 is always retained. Duplicate and unsorted IDs are accepted; a
/// requested ID outside the source font's glyph range returns
/// [`FontError::Invalid`]. Retained glyphs are renumbered from 0 in ascending
/// original-glyph order, and the rewritten `cmap`, `hmtx`, `loca`, `glyf`, and
/// optional `vhea`/`vmtx` tables refer to those new IDs.
pub fn subset_truetype(data: &[u8], glyph_ids: &[u16]) -> Result<Vec<u8>, FontError> {
    let source = SourceFont::parse(data)?;
    let mapping = source.retained_glyphs(glyph_ids)?;
    let tables = source.build_tables(&mapping)?;
    assemble_sfnt(source.flavor, tables)
}

fn invalid_subset(message: impl Into<String>) -> FontError {
    FontError::Invalid(format!("invalid TrueType subset: {}", message.into()))
}
