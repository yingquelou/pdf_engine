//! Simple-font encodings shared by Type 1 and TrueType fonts.

use std::collections::BTreeMap;

use super::{FontError, FontResult, encoding_data, encoding_unicode};

/// A predefined encoding used by a simple font.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PredefinedEncoding {
    Standard,
    WinAnsi,
    MacRoman,
    Symbol,
    ZapfDingbats,
}

/// How a character code is interpreted by a simple font encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodingMapping<'a> {
    /// The code is interpreted by the selected predefined encoding.
    Predefined {
        encoding: PredefinedEncoding,
        code: u8,
    },
    /// The font's `/Differences` array assigns this glyph name to the code.
    GlyphName(&'a str),
}

/// Parsed `/Encoding` information for a simple font.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimpleFontEncoding {
    pub(super) base_encoding: PredefinedEncoding,
    pub(super) differences: BTreeMap<u8, String>,
}

impl SimpleFontEncoding {
    /// Return the encoding used for codes not overridden by `/Differences`.
    pub fn base_encoding(&self) -> PredefinedEncoding {
        self.base_encoding
    }

    /// Return the encoding interpretation for a one-byte character code.
    pub fn mapping_for(&self, code: u8) -> EncodingMapping<'_> {
        match self.differences.get(&code) {
            Some(name) => EncodingMapping::GlyphName(name),
            None => EncodingMapping::Predefined {
                encoding: self.base_encoding,
                code,
            },
        }
    }

    /// Resolve a character code to a glyph name when this encoding knows one.
    ///
    /// StandardEncoding covers its defined ASCII and high-byte glyph slots;
    /// other predefined tables are added independently as their data is
    /// validated.
    pub fn glyph_name_for(&self, code: u8) -> Option<String> {
        match self.mapping_for(code) {
            EncodingMapping::GlyphName(name) => Some(name.to_owned()),
            EncodingMapping::Predefined {
                encoding: PredefinedEncoding::Standard,
                code,
            } => encoding_data::standard_glyph_name(code),
            EncodingMapping::Predefined {
                encoding: PredefinedEncoding::Symbol,
                code,
            } => encoding_data::symbol_name(code).map(str::to_owned),
            EncodingMapping::Predefined {
                encoding: PredefinedEncoding::ZapfDingbats,
                code,
            } => encoding_data::zapf_dingbats_name(code).map(str::to_owned),
            _ => None,
        }
    }

    /// Resolve a character code to Unicode when its encoding provides a known
    /// mapping. `/Differences` glyph names are resolved for the supported
    /// Adobe glyph names with a single Unicode scalar.
    pub fn unicode_for(&self, code: u8) -> Option<char> {
        match self.mapping_for(code) {
            EncodingMapping::GlyphName(name) => encoding_unicode::unicode_for_glyph_name(name),
            EncodingMapping::Predefined { encoding, code } => base_unicode(encoding, code),
        }
    }

    /// Resolve a character code to its Unicode sequence when its encoding
    /// provides a known mapping.
    ///
    /// Adobe Glyph List mappings with multiple Unicode scalars are available
    /// here, while [`Self::unicode_for`] returns only single-scalar mappings.
    pub fn unicode_sequence_for(&self, code: u8) -> Option<String> {
        match self.mapping_for(code) {
            EncodingMapping::GlyphName(name) => {
                encoding_unicode::unicode_sequence_for_glyph_name(name)
            }
            EncodingMapping::Predefined {
                encoding: PredefinedEncoding::WinAnsi,
                code,
            } => encoding_data::win_ansi_unicode(code).map(String::from),
            EncodingMapping::Predefined {
                encoding: PredefinedEncoding::Standard,
                code,
            } => encoding_data::standard_glyph_name(code)
                .and_then(|name| encoding_unicode::unicode_sequence_for_glyph_name(&name)),
            EncodingMapping::Predefined {
                encoding: PredefinedEncoding::MacRoman,
                code,
            } => encoding_data::mac_roman_unicode(code).map(String::from),
            EncodingMapping::Predefined {
                encoding: PredefinedEncoding::Symbol,
                code,
            } => encoding_data::symbol_name(code)
                .and_then(encoding_unicode::unicode_sequence_for_glyph_name),
            EncodingMapping::Predefined {
                encoding: PredefinedEncoding::ZapfDingbats,
                code,
            } => encoding_unicode::zapf_dingbats_unicode(code).map(String::from),
        }
    }
}

/// Resolve a predefined encoding name for reading and write-side encoding.
pub(crate) fn predefined_encoding(name: &str) -> FontResult<PredefinedEncoding> {
    match name {
        "StandardEncoding" => Ok(PredefinedEncoding::Standard),
        "WinAnsiEncoding" => Ok(PredefinedEncoding::WinAnsi),
        "MacRomanEncoding" => Ok(PredefinedEncoding::MacRoman),
        "SymbolEncoding" => Ok(PredefinedEncoding::Symbol),
        "ZapfDingbatsEncoding" => Ok(PredefinedEncoding::ZapfDingbats),
        _ => Err(FontError::Invalid(format!(
            "unsupported predefined encoding /{name}"
        ))),
    }
}

/// Encoding a font uses when `/Encoding` is omitted (PDF 32000-1 §9.6.6.1).
pub(crate) fn default_encoding(base_font: &str) -> PredefinedEncoding {
    match base_font {
        "Symbol" => PredefinedEncoding::Symbol,
        "ZapfDingbats" => PredefinedEncoding::ZapfDingbats,
        _ => PredefinedEncoding::Standard,
    }
}

/// Unicode scalar for a code in a predefined encoding.
fn base_unicode(encoding: PredefinedEncoding, code: u8) -> Option<char> {
    match encoding {
        PredefinedEncoding::WinAnsi => encoding_data::win_ansi_unicode(code),
        PredefinedEncoding::Standard => encoding_data::standard_glyph_name(code)
            .and_then(|name| encoding_unicode::unicode_for_glyph_name(&name)),
        PredefinedEncoding::MacRoman => encoding_data::mac_roman_unicode(code),
        PredefinedEncoding::Symbol => {
            encoding_data::symbol_name(code).and_then(encoding_unicode::unicode_for_glyph_name)
        }
        PredefinedEncoding::ZapfDingbats => encoding_unicode::zapf_dingbats_unicode(code),
    }
}

/// Reverse of [`base_unicode`] for write-side text encoding.
///
/// `/Differences` assignments are searched first (lowest code wins). The base
/// encoding is then scanned with overridden codes excluded, so a glyph name
/// reassigned by `/Differences` no longer resolves to its original base code.
pub(crate) fn code_for_char(
    base: PredefinedEncoding,
    differences: &BTreeMap<u8, String>,
    ch: char,
) -> Option<u8> {
    if let Some((&code, _)) = differences
        .iter()
        .find(|(_, name)| encoding_unicode::unicode_for_glyph_name(name) == Some(ch))
    {
        return Some(code);
    }
    (0..=u8::MAX)
        .find(|&code| !differences.contains_key(&code) && base_unicode(base, code) == Some(ch))
}
