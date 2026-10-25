//! Bare CFF1 font programs (`/CIDFontType0C` and `/Type1C`).
//!
//! PDF embeds Compact Font Format programs without an SFNT container in a
//! `/FontFile3` stream whose `/Subtype` is `/CIDFontType0C` (CID-keyed) or
//! `/Type1C` (name-keyed). `ttf-parser` only understands SFNT wrappers, so this
//! module parses the CFF1 structure itself:
//!
//! * header, Name INDEX, Top DICT INDEX, String INDEX and Global Subr INDEX;
//! * Top DICT operators `CharStrings` (17), `charset` (15), `Private` (18),
//!   `ROS` (12 30), `FDArray` (12 36), `FDSelect` (12 37), `CIDCount` (12 34),
//!   `FontMatrix` (12 7) and `FontBBox` (5);
//! * charset formats 0, 1 and 2 (the predefined ISOAdobe/Expert/ExpertSubset
//!   charsets are recognised but only ISOAdobe is meaningful and never used for
//!   CID-keyed fonts, where the specification forbids predefined charsets);
//! * FDSelect formats 0 and 3, the FDArray INDEX and each Font DICT's Private
//!   DICT (`defaultWidthX` 20 / `nominalWidthX` 21);
//! * the Type 2 width operand of every glyph charstring (the first stack
//!   clearing operator's operand-count parity decides whether the first operand
//!   is a width).
//!
//! Outlines are delegated to `ttf-parser` by wrapping the program in a minimal
//! synthetic face built with [`ttf_parser::Face::from_raw_tables`]: only
//! `head`, `hhea`, `maxp` and a zeroed `hmtx` are synthesized, and the original
//! CFF bytes are exposed as the `CFF ` table. No SFNT directory, checksums or
//! `cmap` are required. Widths come from the parsed CFF data, never from the
//! synthetic `hmtx`.
//!
//! [`RawCffBackend`] implements [`CffOutlineParser`]. For CID-keyed programs
//! the `parse_glyph` argument is a **CID** (CIDFontType0 semantics) and is
//! translated through the charset; for name-keyed programs it is a GID.
//!
//! # Submodules
//!
//! * `parse` - CFF INDEX, DICT, charset, FDSelect and Private DICT helpers;
//! * `charstring` - Type 2 width operands and path-drawing detection;
//! * `tables` - the synthetic `head`/`hhea`/`maxp`/`hmtx` tables.

mod charstring;
// 子集化模块（`cff::subset`）需要复用 INDEX/charset/FDSelect/DICT 解析器。
pub(crate) mod parse;
mod tables;
#[cfg(test)]
mod tests;

use ttf_parser::{Face, GlyphId, RawFaceTables};

use self::charstring::{charstring_draws_path, charstring_width_operand};
use self::parse::{
    WidthDefaults, parse_charset, parse_fd_select, parse_font_dict, parse_index,
    parse_private_widths, parse_top_dict, truncated,
};
use self::tables::{SyntheticTables, units_per_em_from_matrix};
use super::outline::{CffGlyphOutline, CffOutlineParser};
use super::ttf_parser::PathBuilder;
use crate::core::reader::font::{FontError, FontResult};

/// `FontMatrix` default: 1/1000 scaling, i.e. 1000 units per em.
const DEFAULT_FONT_MATRIX: [f64; 6] = [0.001, 0.0, 0.0, 0.001, 0.0, 0.0];

/// A parsed bare CFF1 font program.
///
/// The program may be CID-keyed (`ROS` present) or name-keyed. Parsing is eager
/// and validates that the bytes can be wrapped into the synthetic face used for
/// outlines, so construction fails early for truncated or unsupported data.
#[derive(Debug, Clone)]
pub struct RawCffFont {
    /// Program bytes followed by one zero padding byte (see [`Self::parse_data`]).
    data: Vec<u8>,
    /// Length of the original program, excluding the trailing padding byte.
    data_len: usize,
    /// Whether the Top DICT contains `ROS` (CID-keyed program).
    is_cid_keyed: bool,
    /// Number of entries in the CharStrings INDEX.
    glyph_count: u16,
    /// Derived from `FontMatrix[0]` (`1 / FontMatrix[0]`), default 1000.
    units_per_em: u16,
    /// Top DICT `FontMatrix`, defaulting to `[0.001 0 0 0.001 0 0]`.
    font_matrix: [f64; 6],
    /// Top DICT `FontBBox`, when present.
    font_bbox: Option<[f64; 4]>,
    /// Top DICT `CIDCount`, when present.
    cid_count: Option<u16>,
    /// `ROS` operands (Registry SID, Ordering SID, Supplement), when present.
    ros: Option<[u16; 3]>,
    /// GID -> CID for CID-keyed programs, GID -> SID for name-keyed programs
    /// with an explicit charset. Index 0 is always 0.
    charset: Option<Vec<u16>>,
    /// Resolved advance width per GID, in font units.
    widths: Vec<f64>,
    /// Whether the charstring contains a path drawing operator. Glyphs without
    /// one (`endchar`-only charstrings) yield an empty outline.
    draws_path: Vec<bool>,
    /// Synthetic tables handed to `ttf-parser`.
    tables: SyntheticTables,
}

impl RawCffFont {
    /// Parse a bare CFF1 program.
    ///
    /// The bytes are not modified: the single padding byte required by
    /// `ttf-parser`'s INDEX reader is appended to an internal copy of the data.
    pub fn parse(data: Vec<u8>) -> FontResult<Self> {
        if data.len() < 4 {
            return Err(truncated("CFF header", 0));
        }
        let major = data[0];
        if major != 1 {
            return Err(FontError::Invalid(format!(
                "unsupported CFF major version {major}; only CFF 1.x is supported"
            )));
        }
        let header_size = usize::from(data[2]);
        if header_size < 4 || header_size > data.len() {
            return Err(FontError::Invalid(format!(
                "invalid CFF header size {header_size} for {} bytes",
                data.len()
            )));
        }

        let name_index = parse_index(&data, header_size)?;
        let top_dict_index = parse_index(&data, name_index.end)?;
        let top_dict_range = top_dict_index
            .entries
            .first()
            .ok_or_else(|| FontError::Invalid("CFF Top DICT INDEX is empty".into()))?;
        let top_dict = parse_top_dict(&data[top_dict_range.clone()])?;
        let string_index = parse_index(&data, top_dict_index.end)?;
        let _global_subrs = parse_index(&data, string_index.end)?;

        let char_strings_offset = top_dict.char_strings.ok_or_else(|| {
            FontError::Invalid("CFF Top DICT is missing the CharStrings offset".into())
        })?;
        let char_strings = parse_index(&data, char_strings_offset)?;
        let glyph_count = u16::try_from(char_strings.entries.len()).map_err(|_| {
            FontError::Invalid(format!(
                "CFF CharStrings INDEX has {} glyphs, more than 65535",
                char_strings.entries.len()
            ))
        })?;
        if glyph_count == 0 {
            return Err(FontError::Invalid(
                "CFF CharStrings INDEX contains no glyphs".into(),
            ));
        }

        let is_cid_keyed = top_dict.ros.is_some();
        let charset = match top_dict.charset {
            Some(charset_id @ (0..=2)) if is_cid_keyed => {
                return Err(FontError::Invalid(format!(
                    "CID-keyed CFF uses predefined charset {charset_id}; the specification forbids predefined charsets for CID fonts"
                )));
            }
            // Predefined charsets. ISOAdobe maps GID to SID directly; Expert and
            // ExpertSubset are accepted for name-keyed fonts but not expanded
            // because no SID lookup is exposed.
            Some(0..=2) => None,
            Some(offset) => Some(parse_charset(&data, offset, glyph_count)?),
            None if is_cid_keyed => {
                return Err(FontError::Invalid(
                    "CID-keyed CFF is missing the required charset offset".into(),
                ));
            }
            None => None,
        };

        let (fd_select, font_dict_widths) = if is_cid_keyed {
            let fd_array_offset = top_dict.fd_array.ok_or_else(|| {
                FontError::Invalid("CID-keyed CFF is missing the FDArray offset".into())
            })?;
            let fd_select_offset = top_dict.fd_select.ok_or_else(|| {
                FontError::Invalid("CID-keyed CFF is missing the FDSelect offset".into())
            })?;
            let fd_array = parse_index(&data, fd_array_offset)?;
            if fd_array.entries.is_empty() {
                return Err(FontError::Invalid("CFF FDArray INDEX is empty".into()));
            }
            let mut widths = Vec::with_capacity(fd_array.entries.len());
            for entry in &fd_array.entries {
                let font_dict = parse_font_dict(&data[entry.clone()])?;
                widths.push(match font_dict.private {
                    Some((size, offset)) => parse_private_widths(&data, offset, size)?,
                    None => WidthDefaults::default(),
                });
            }
            let fd_select = parse_fd_select(&data, fd_select_offset, glyph_count)?;
            if let Some(invalid) = fd_select
                .iter()
                .find(|&&fd| usize::from(fd) >= widths.len())
            {
                return Err(FontError::Invalid(format!(
                    "CFF FDSelect references Font DICT {invalid} but FDArray has {} entries",
                    widths.len()
                )));
            }
            (fd_select, widths)
        } else {
            let widths = match top_dict.private {
                Some((size, offset)) => parse_private_widths(&data, offset, size)?,
                None => WidthDefaults::default(),
            };
            (Vec::new(), vec![widths])
        };

        let mut widths = Vec::with_capacity(usize::from(glyph_count));
        let mut draws_path = Vec::with_capacity(usize::from(glyph_count));
        for (gid, range) in char_strings.entries.iter().enumerate() {
            let charstring = &data[range.clone()];
            let defaults = if is_cid_keyed {
                font_dict_widths[usize::from(fd_select[gid])]
            } else {
                font_dict_widths[0]
            };
            let width = match charstring_width_operand(charstring)? {
                Some(operand) => defaults.nominal_width + operand,
                None => defaults.default_width,
            };
            widths.push(width);
            draws_path.push(charstring_draws_path(charstring));
        }

        let font_matrix = top_dict.font_matrix.unwrap_or(DEFAULT_FONT_MATRIX);
        let units_per_em = units_per_em_from_matrix(&font_matrix);
        let font_bbox = top_dict.font_bbox;
        let tables = SyntheticTables::new(units_per_em, glyph_count, font_bbox);

        let data_len = data.len();
        let mut data = data;
        // `ttf-parser`'s INDEX reader consumes one byte more than the last
        // INDEX holds (`last_offset` instead of `last_offset - 1`). Bare CFF
        // programs typically end with the CharStrings INDEX, so a single
        // trailing byte makes the data acceptable to every CFF reader.
        data.push(0);

        let font = Self {
            data,
            data_len,
            is_cid_keyed,
            glyph_count,
            units_per_em,
            font_matrix,
            font_bbox,
            cid_count: top_dict.cid_count,
            ros: top_dict.ros,
            charset,
            widths,
            draws_path,
            tables,
        };
        // Reject programs that our parser accepts but `ttf-parser` cannot read
        // while the caller still has access to the error.
        font.face()?;
        Ok(font)
    }

    /// Return the original program bytes (the padding byte is not exposed).
    pub fn data(&self) -> &[u8] {
        &self.data[..self.data_len]
    }

    /// Return whether the program is CID-keyed (`ROS` present).
    pub fn is_cid_keyed(&self) -> bool {
        self.is_cid_keyed
    }

    /// Return the number of glyphs (entries in the CharStrings INDEX).
    pub fn glyph_count(&self) -> u16 {
        self.glyph_count
    }

    /// Return the resolution of one glyph-space unit, derived from
    /// `FontMatrix[0]` (default 1000).
    pub fn units_per_em(&self) -> u16 {
        self.units_per_em
    }

    /// Return the Top DICT `FontMatrix`, defaulting to
    /// `[0.001 0 0 0.001 0 0]`.
    pub fn font_matrix(&self) -> [f64; 6] {
        self.font_matrix
    }

    /// Return the Top DICT `FontBBox`, when present.
    pub fn font_bbox(&self) -> Option<[f64; 4]> {
        self.font_bbox
    }

    /// Return the Top DICT `CIDCount`, when present.
    pub fn cid_count(&self) -> Option<u16> {
        self.cid_count
    }

    /// Return the `ROS` operands (`[Registry SID, Ordering SID, Supplement]`),
    /// or `None` for name-keyed programs.
    pub fn ros(&self) -> Option<[u16; 3]> {
        self.ros
    }

    /// Return the glyph ID whose charset entry equals `cid`.
    ///
    /// Returns `None` for name-keyed programs and for CIDs missing from the
    /// charset.
    pub fn glyph_for_cid(&self, cid: u16) -> Option<u16> {
        if !self.is_cid_keyed {
            return None;
        }
        self.charset
            .as_ref()?
            .iter()
            .position(|&value| value == cid)
            .and_then(|gid| u16::try_from(gid).ok())
    }

    /// Return the CID of a glyph, or `None` for name-keyed programs.
    pub fn cid_for_glyph(&self, gid: u16) -> Option<u16> {
        if !self.is_cid_keyed {
            return None;
        }
        self.charset.as_ref()?.get(usize::from(gid)).copied()
    }

    /// Return the advance width of a glyph in font units.
    pub fn glyph_width(&self, gid: u16) -> Option<f64> {
        self.widths.get(usize::from(gid)).copied()
    }

    /// Outline a glyph by glyph ID.
    ///
    /// Glyphs whose charstring only contains `endchar` produce an empty
    /// segment list rather than an error.
    pub fn outline(&self, gid: u16) -> FontResult<CffGlyphOutline> {
        let index = usize::from(gid);
        if index >= usize::from(self.glyph_count) {
            return Err(FontError::Invalid(format!(
                "CFF glyph ID {gid} is out of range (glyph count {})",
                self.glyph_count
            )));
        }
        let width = self.widths[index];
        if !self.draws_path[index] {
            return Ok(CffGlyphOutline {
                width,
                segments: Vec::new(),
            });
        }

        let face = self.face()?;
        let mut builder = PathBuilder::new();
        face.outline_glyph(GlyphId(gid), &mut builder)
            .ok_or_else(|| FontError::Invalid(format!("CFF glyph {gid} has no outline")))?;
        Ok(CffGlyphOutline {
            width,
            segments: builder.segments,
        })
    }

    /// Build the synthetic `ttf-parser` face that borrows this program.
    fn face(&self) -> FontResult<Face<'_>> {
        let tables = RawFaceTables {
            head: &self.tables.head,
            hhea: &self.tables.hhea,
            maxp: &self.tables.maxp,
            hmtx: Some(&self.tables.hmtx),
            cff: Some(&self.data),
            ..RawFaceTables::default()
        };
        Face::from_raw_tables(tables).map_err(|error| {
            FontError::Invalid(format!(
                "bare CFF program cannot be wrapped into an OpenType face: {error}"
            ))
        })
    }
}

/// A [`CffOutlineParser`] for bare CFF1 programs.
///
/// The backend owns the parsed program and rebuilds the synthetic `ttf-parser`
/// face per call, so no self-referential borrow has to be stored.
#[derive(Debug, Clone)]
pub struct RawCffBackend {
    font: RawCffFont,
}

impl RawCffBackend {
    /// Parse a bare CFF1 program.
    ///
    /// Returns an error for truncated data, unsupported CFF versions and
    /// CID-keyed programs that lack their mandatory charset/FDArray/FDSelect.
    pub fn parse(data: Vec<u8>) -> FontResult<Self> {
        Ok(Self {
            font: RawCffFont::parse(data)?,
        })
    }

    /// Return the parsed program.
    pub fn font(&self) -> &RawCffFont {
        &self.font
    }

    /// Return the glyph ID whose charset entry equals `cid`.
    pub fn glyph_for_cid(&self, cid: u16) -> Option<u16> {
        self.font.glyph_for_cid(cid)
    }
}

impl CffOutlineParser for RawCffBackend {
    fn parse_glyph(&self, gid_or_cid: u16) -> FontResult<CffGlyphOutline> {
        let gid = if self.font.is_cid_keyed {
            self.font.glyph_for_cid(gid_or_cid).ok_or_else(|| {
                FontError::Invalid(format!(
                    "CID {gid_or_cid} is not present in the CFF charset"
                ))
            })?
        } else {
            gid_or_cid
        };
        self.font.outline(gid)
    }

    fn glyph_count(&self) -> u16 {
        self.font.glyph_count
    }

    fn is_cid_keyed(&self) -> bool {
        self.font.is_cid_keyed
    }

    fn glyph_for_cid(&self, cid: u16) -> Option<u16> {
        self.font.glyph_for_cid(cid)
    }
}
