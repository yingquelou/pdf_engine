//! Semantic access to simple Type 1 font dictionaries.

use std::{collections::BTreeMap, fmt};

use crate::pdf::types::{Array, Dict, Document, Value};

use super::ParseError;

type FontResult<T> = std::result::Result<T, FontError>;

/// Metrics parsed from a Type 1 font's `/FontDescriptor`.
#[derive(Debug, Clone, PartialEq)]
pub struct FontDescriptorMetrics {
    /// Descriptor `/FontName`, when present.
    pub font_name: Option<String>,
    /// Descriptor `/Flags`, when present.
    pub flags: Option<u32>,
    /// Descriptor `/FontBBox`, when present.
    pub font_bbox: Option<[f64; 4]>,
    /// Descriptor `/ItalicAngle`, when present.
    pub italic_angle: Option<f64>,
    /// Descriptor `/Ascent`, when present.
    pub ascent: Option<f64>,
    /// Descriptor `/Descent`, when present.
    pub descent: Option<f64>,
    /// Descriptor `/CapHeight`, when present.
    pub cap_height: Option<f64>,
    /// Descriptor `/StemV`, when present.
    pub stem_v: Option<f64>,
    /// Descriptor `/MissingWidth`, when present.
    pub missing_width: Option<f64>,
}

/// Resolved metrics from a simple Type 1 font dictionary.
#[derive(Debug, Clone, PartialEq)]
pub struct Type1Font {
    /// Font dictionary `/BaseFont`.
    pub base_font: String,
    /// First code covered by `/Widths`; absent for a standard 14 font without
    /// an explicit width array.
    pub first_char: Option<u8>,
    /// Widths in text-space units, indexed from `first_char`.
    pub widths: Vec<f64>,
    /// Parsed `/FontDescriptor` metrics, when the font dictionary provides one.
    pub descriptor: Option<FontDescriptorMetrics>,
}

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

/// Parsed `/Encoding` information for a Type 1 simple font.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimpleFontEncoding {
    base_encoding: PredefinedEncoding,
    differences: BTreeMap<u8, String>,
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
            } => standard_glyph_name(code),
            _ => None,
        }
    }

    /// Resolve a character code to Unicode when its encoding provides a known
    /// mapping. `/Differences` glyph names are resolved for the supported
    /// Adobe glyph-name subset.
    pub fn unicode_for(&self, code: u8) -> Option<char> {
        match self.mapping_for(code) {
            EncodingMapping::GlyphName(name) => unicode_for_glyph_name(name),
            EncodingMapping::Predefined {
                encoding: PredefinedEncoding::WinAnsi,
                code,
            } => win_ansi_unicode(code),
            EncodingMapping::Predefined {
                encoding: PredefinedEncoding::Standard,
                code,
            } => standard_glyph_name(code).and_then(|name| unicode_for_glyph_name(&name)),
            EncodingMapping::Predefined {
                encoding: PredefinedEncoding::MacRoman,
                code,
            } => mac_roman_unicode(code),
            EncodingMapping::Predefined { .. } => None,
        }
    }
}

impl Type1Font {
    /// Return the glyph width for a one-byte character code when available.
    pub fn width_for(&self, code: u8) -> Option<f64> {
        if let Some(first_char) = self.first_char
            && let Some(index) = usize::from(code).checked_sub(usize::from(first_char))
            && let Some(width) = self.widths.get(index)
        {
            return Some(*width);
        }
        self.descriptor
            .as_ref()
            .and_then(|descriptor| descriptor.missing_width)
            .or_else(|| standard14_width(&self.base_font, code))
    }
}

#[derive(Debug)]
pub enum FontError {
    /// An indirect object referenced by the font could not be parsed.
    Parse(ParseError),
    /// A font dictionary or metric does not satisfy the supported structure.
    Invalid(String),
}

impl fmt::Display for FontError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(error) => write!(f, "{error}"),
            Self::Invalid(message) => write!(f, "invalid Type 1 font: {message}"),
        }
    }
}

impl std::error::Error for FontError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Parse(error) => Some(error),
            Self::Invalid(_) => None,
        }
    }
}

impl From<ParseError> for FontError {
    fn from(error: ParseError) -> Self {
        Self::Parse(error)
    }
}

impl Document {
    /// Resolve a Type 1 font dictionary and its metrics from direct or indirect values.
    ///
    /// Returns `Ok(None)` for non-Type-1 font dictionaries. Malformed font data,
    /// dangling references, and object-generation mismatches are reported.
    /// Standard 14 fonts may omit `/Widths`; built-in width tables and embedded
    /// font programs are not loaded by this API.
    pub fn type1_font(&self, value: &Value) -> FontResult<Option<Type1Font>> {
        let value = self.resolve_value(value)?;
        let dict = value
            .as_dict()
            .ok_or_else(|| FontError::Invalid("font value is not a dictionary".to_string()))?;
        if dict.get("Subtype").and_then(Value::as_name) != Some("Type1") {
            return Ok(None);
        }

        let base_font = dict
            .get("BaseFont")
            .and_then(Value::as_name)
            .ok_or_else(|| FontError::Invalid("missing or invalid /BaseFont".to_string()))?
            .to_string();
        let (first_char, widths) = self.read_widths(dict)?;
        let standard_font = is_standard_14(&base_font);
        if first_char.is_none() && !standard_font {
            return Err(FontError::Invalid(
                "non-standard Type 1 fonts require /FirstChar, /LastChar, and /Widths".to_string(),
            ));
        }
        let descriptor = match dict.get("FontDescriptor") {
            Some(value) => {
                let descriptor = self.resolve_value(value)?;
                let descriptor = descriptor.as_dict().ok_or_else(|| {
                    FontError::Invalid("/FontDescriptor is not a dictionary".to_string())
                })?;
                Some(parse_descriptor(descriptor)?)
            }
            None => None,
        };
        if descriptor.is_none() && !standard_font {
            return Err(FontError::Invalid(
                "non-standard Type 1 fonts require /FontDescriptor".to_string(),
            ));
        }

        Ok(Some(Type1Font {
            base_font,
            first_char,
            widths,
            descriptor,
        }))
    }

    /// Resolve a Type 1 font's predefined `/Encoding` and `/Differences`.
    ///
    /// Returns `Ok(None)` for non-Type-1 fonts. The existing [`Type1Font`]
    /// structure remains unchanged; callers can opt into encoding semantics
    /// through this separate method.
    pub fn type1_font_encoding(&self, value: &Value) -> FontResult<Option<SimpleFontEncoding>> {
        let value = self.resolve_value(value)?;
        let dict = value
            .as_dict()
            .ok_or_else(|| FontError::Invalid("font value is not a dictionary".to_string()))?;
        if dict.get("Subtype").and_then(Value::as_name) != Some("Type1") {
            return Ok(None);
        }
        let base_font = dict
            .get("BaseFont")
            .and_then(Value::as_name)
            .ok_or_else(|| FontError::Invalid("missing or invalid /BaseFont".to_string()))?;
        let default_encoding = default_encoding(base_font);
        let Some(encoding_value) = dict.get("Encoding") else {
            return Ok(Some(SimpleFontEncoding {
                base_encoding: default_encoding,
                differences: BTreeMap::new(),
            }));
        };
        let encoding = self.resolve_value(encoding_value)?;
        match encoding {
            Value::Name(name) => Ok(Some(SimpleFontEncoding {
                base_encoding: predefined_encoding(&name)?,
                differences: BTreeMap::new(),
            })),
            Value::Dict(encoding_dict) => {
                let base_encoding = match encoding_dict.get("BaseEncoding") {
                    Some(value) => {
                        let value = self.resolve_value(value)?;
                        let name = value.as_name().ok_or_else(|| {
                            FontError::Invalid("/Encoding /BaseEncoding is not a name".to_string())
                        })?;
                        predefined_encoding(name)?
                    }
                    None => default_encoding,
                };
                let differences = match encoding_dict.get("Differences") {
                    Some(value) => {
                        let value = self.resolve_value(value)?;
                        parse_differences(value.as_array().ok_or_else(|| {
                            FontError::Invalid("/Encoding /Differences is not an array".into())
                        })?)?
                    }
                    None => BTreeMap::new(),
                };
                Ok(Some(SimpleFontEncoding {
                    base_encoding,
                    differences,
                }))
            }
            _ => Err(FontError::Invalid(
                "/Encoding is neither a name nor a dictionary".to_string(),
            )),
        }
    }

    fn read_widths(&self, dict: &Dict) -> FontResult<(Option<u8>, Vec<f64>)> {
        let first = dict.get("FirstChar");
        let last = dict.get("LastChar");
        let widths = dict.get("Widths");
        if first.is_none() && last.is_none() && widths.is_none() {
            return Ok((None, Vec::new()));
        }

        let first = first
            .and_then(Value::as_integer)
            .and_then(|value| u8::try_from(value).ok())
            .ok_or_else(|| FontError::Invalid("invalid or missing /FirstChar".to_string()))?;
        let last = last
            .and_then(Value::as_integer)
            .and_then(|value| u8::try_from(value).ok())
            .ok_or_else(|| FontError::Invalid("invalid or missing /LastChar".to_string()))?;
        if last < first {
            return Err(FontError::Invalid(
                "/LastChar precedes /FirstChar".to_string(),
            ));
        }
        let widths = self.resolve_value(
            widths.ok_or_else(|| FontError::Invalid("missing /Widths".to_string()))?,
        )?;
        let widths = widths
            .as_array()
            .ok_or_else(|| FontError::Invalid("/Widths is not an array".to_string()))?;
        let expected_len = usize::from(last - first) + 1;
        if widths.len() != expected_len {
            return Err(FontError::Invalid(format!(
                "/Widths contains {} entries; expected {expected_len}",
                widths.len()
            )));
        }
        let widths = widths
            .iter()
            .map(|value| {
                number(value).ok_or_else(|| {
                    FontError::Invalid(
                        "/Widths contains a non-finite or non-numeric value".to_string(),
                    )
                })
            })
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok((Some(first), widths))
    }

    fn resolve_value(&self, value: &Value) -> FontResult<Value> {
        match value {
            Value::Ref(num, generation) => {
                let object = self
                    .get_object(*num)?
                    .filter(|object| object.generation == *generation)
                    .ok_or_else(|| {
                        FontError::Invalid(format!(
                            "unresolved object reference {num} {generation} R"
                        ))
                    })?;
                Ok(object.value)
            }
            _ => Ok(value.clone()),
        }
    }
}

fn parse_differences(values: &Array) -> FontResult<BTreeMap<u8, String>> {
    let mut differences = BTreeMap::new();
    let mut next_code = None;
    let mut values = values.iter().peekable();
    while let Some(value) = values.next() {
        match value {
            Value::Integer(code) => {
                next_code = Some(u8::try_from(*code).map_err(|_| {
                    FontError::Invalid(
                        "/Encoding /Differences code is outside the byte range".to_string(),
                    )
                })?);
            }
            Value::Name(name) => {
                let code = next_code.ok_or_else(|| {
                    FontError::Invalid(
                        "/Encoding /Differences glyph name has no preceding code".to_string(),
                    )
                })?;
                differences.insert(code, name.clone());
                next_code = code.checked_add(1);
                if next_code.is_none() && matches!(values.peek(), Some(Value::Name(_))) {
                    return Err(FontError::Invalid(
                        "/Encoding /Differences assigns a glyph beyond code 255".to_string(),
                    ));
                }
            }
            _ => {
                return Err(FontError::Invalid(
                    "/Encoding /Differences contains a non-integer, non-name value".to_string(),
                ));
            }
        }
    }
    Ok(differences)
}

fn predefined_encoding(name: &str) -> FontResult<PredefinedEncoding> {
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

fn default_encoding(base_font: &str) -> PredefinedEncoding {
    match base_font {
        "Symbol" => PredefinedEncoding::Symbol,
        "ZapfDingbats" => PredefinedEncoding::ZapfDingbats,
        _ => PredefinedEncoding::Standard,
    }
}

fn win_ansi_unicode(code: u8) -> Option<char> {
    let codepoint = match code {
        0x80 => 0x20AC,
        0x82 => 0x201A,
        0x83 => 0x0192,
        0x84 => 0x201E,
        0x85 => 0x2026,
        0x86 => 0x2020,
        0x87 => 0x2021,
        0x88 => 0x02C6,
        0x89 => 0x2030,
        0x8A => 0x0160,
        0x8B => 0x2039,
        0x8C => 0x0152,
        0x8E => 0x017D,
        0x91 => 0x2018,
        0x92 => 0x2019,
        0x93 => 0x201C,
        0x94 => 0x201D,
        0x95 => 0x2022,
        0x96 => 0x2013,
        0x97 => 0x2014,
        0x98 => 0x02DC,
        0x99 => 0x2122,
        0x9A => 0x0161,
        0x9B => 0x203A,
        0x9C => 0x0153,
        0x9E => 0x017E,
        0x9F => 0x0178,
        0x00..=0x1F | 0x7F | 0x81 | 0x8D | 0x8F | 0x90 | 0x9D => return None,
        _ => u32::from(code),
    };
    char::from_u32(codepoint)
}

fn mac_roman_unicode(code: u8) -> Option<char> {
    let codepoint = match code {
        0x00..=0x1F | 0x7F => return None,
        0x20..=0x7E => u32::from(code),
        0x80..=0xFF => u32::from(MAC_ROMAN_HIGH[usize::from(code - 0x80)]),
    };
    char::from_u32(codepoint)
}

const MAC_ROMAN_HIGH: [u16; 128] = [
    0x00C4, 0x00C5, 0x00C7, 0x00C9, 0x00D1, 0x00D6, 0x00DC, 0x00E1, 0x00E0, 0x00E2, 0x00E4, 0x00E3,
    0x00E5, 0x00E7, 0x00E9, 0x00E8, 0x00EA, 0x00EB, 0x00ED, 0x00EC, 0x00EE, 0x00EF, 0x00F1, 0x00F3,
    0x00F2, 0x00F4, 0x00F6, 0x00F5, 0x00FA, 0x00F9, 0x00FB, 0x00FC, 0x2020, 0x00B0, 0x00A2, 0x00A3,
    0x00A7, 0x2022, 0x00B6, 0x00DF, 0x00AE, 0x00A9, 0x2122, 0x00B4, 0x00A8, 0x2260, 0x00C6, 0x00D8,
    0x221E, 0x00B1, 0x2264, 0x2265, 0x00A5, 0x00B5, 0x2202, 0x2211, 0x220F, 0x03C0, 0x222B, 0x00AA,
    0x00BA, 0x03A9, 0x00E6, 0x00F8, 0x00BF, 0x00A1, 0x00AC, 0x221A, 0x0192, 0x2248, 0x2206, 0x00AB,
    0x00BB, 0x2026, 0x00A0, 0x00C0, 0x00C3, 0x00D5, 0x0152, 0x0153, 0x2013, 0x2014, 0x201C, 0x201D,
    0x2018, 0x2019, 0x00F7, 0x25CA, 0x00FF, 0x0178, 0x2044, 0x20AC, 0x2039, 0x203A, 0xFB01, 0xFB02,
    0x2021, 0x00B7, 0x201A, 0x201E, 0x2030, 0x00C2, 0x00CA, 0x00C1, 0x00CB, 0x00C8, 0x00CD, 0x00CE,
    0x00CF, 0x00CC, 0x00D3, 0x00D4, 0xF8FF, 0x00D2, 0x00DA, 0x00DB, 0x00D9, 0x0131, 0x02C6, 0x02DC,
    0x00AF, 0x02D8, 0x02D9, 0x02DA, 0x00B8, 0x02DD, 0x02DB, 0x02C7,
];

fn standard_glyph_name(code: u8) -> Option<String> {
    if code.is_ascii_alphabetic() {
        return Some(char::from(code).to_string());
    }
    if code.is_ascii_digit() {
        return Some(
            [
                "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine",
            ][usize::from(code - b'0')]
            .to_owned(),
        );
    }
    let name = match code {
        0x20 => "space",
        0x21 => "exclam",
        0x22 => "quotedbl",
        0x23 => "numbersign",
        0x24 => "dollar",
        0x25 => "percent",
        0x26 => "ampersand",
        0x27 => "quoteright",
        0x28 => "parenleft",
        0x29 => "parenright",
        0x2A => "asterisk",
        0x2B => "plus",
        0x2C => "comma",
        0x2D => "hyphen",
        0x2E => "period",
        0x2F => "slash",
        0x3A => "colon",
        0x3B => "semicolon",
        0x3C => "less",
        0x3D => "equal",
        0x3E => "greater",
        0x3F => "question",
        0x40 => "at",
        0x5B => "bracketleft",
        0x5C => "backslash",
        0x5D => "bracketright",
        0x5E => "asciicircum",
        0x5F => "underscore",
        0x60 => "quoteleft",
        0x7B => "braceleft",
        0x7C => "bar",
        0x7D => "braceright",
        0x7E => "asciitilde",
        0xA1 => "exclamdown",
        0xA2 => "cent",
        0xA3 => "sterling",
        0xA4 => "fraction",
        0xA5 => "yen",
        0xA6 => "florin",
        0xA7 => "section",
        0xA8 => "currency",
        0xA9 => "quotesingle",
        0xAA => "quotedblleft",
        0xAB => "guillemotleft",
        0xAC => "guilsinglleft",
        0xAD => "guilsinglright",
        0xAE => "fi",
        0xAF => "fl",
        0xB1 => "endash",
        0xB2 => "dagger",
        0xB3 => "daggerdbl",
        0xB4 => "periodcentered",
        0xB6 => "paragraph",
        0xB7 => "bullet",
        0xB8 => "quotesinglbase",
        0xB9 => "quotedblbase",
        0xBA => "quotedblright",
        0xBB => "guillemotright",
        0xBC => "ellipsis",
        0xBD => "perthousand",
        0xBF => "questiondown",
        0xC1 => "grave",
        0xC2 => "acute",
        0xC3 => "circumflex",
        0xC4 => "tilde",
        0xC5 => "macron",
        0xC6 => "breve",
        0xC7 => "dotaccent",
        0xC8 => "dieresis",
        0xCA => "ring",
        0xCB => "cedilla",
        0xCD => "hungarumlaut",
        0xCE => "ogonek",
        0xCF => "caron",
        0xD0 => "emdash",
        0xDF => "AE",
        0xE1 => "ordfeminine",
        0xE7 => "Lslash",
        0xE8 => "Oslash",
        0xE9 => "OE",
        0xEA => "ordmasculine",
        0xF1 => "ae",
        0xF5 => "dotlessi",
        0xF8 => "lslash",
        0xF9 => "oslash",
        0xFA => "oe",
        0xFB => "germandbls",
        _ => return None,
    };
    Some(name.to_owned())
}

fn unicode_for_glyph_name(name: &str) -> Option<char> {
    let scalar = match name {
        "space" => 0x0020,
        "exclam" => 0x0021,
        "quotedbl" => 0x0022,
        "numbersign" => 0x0023,
        "dollar" => 0x0024,
        "percent" => 0x0025,
        "ampersand" => 0x0026,
        "quotesingle" => 0x0027,
        "quoteright" => 0x2019,
        "parenleft" => 0x0028,
        "parenright" => 0x0029,
        "asterisk" => 0x002A,
        "plus" => 0x002B,
        "comma" => 0x002C,
        "hyphen" => 0x002D,
        "period" => 0x002E,
        "slash" => 0x002F,
        "colon" => 0x003A,
        "semicolon" => 0x003B,
        "less" => 0x003C,
        "equal" => 0x003D,
        "greater" => 0x003E,
        "question" => 0x003F,
        "at" => 0x0040,
        "exclamdown" => 0x00A1,
        "cent" => 0x00A2,
        "sterling" => 0x00A3,
        "fraction" => 0x2044,
        "yen" => 0x00A5,
        "florin" => 0x0192,
        "section" => 0x00A7,
        "currency" => 0x00A4,
        "quotedblleft" => 0x201C,
        "guillemotleft" => 0x00AB,
        "guilsinglleft" => 0x2039,
        "guilsinglright" => 0x203A,
        "fi" => 0xFB01,
        "fl" => 0xFB02,
        "endash" => 0x2013,
        "dagger" => 0x2020,
        "daggerdbl" => 0x2021,
        "periodcentered" => 0x00B7,
        "paragraph" => 0x00B6,
        "bullet" => 0x2022,
        "quotesinglbase" => 0x201A,
        "quotedblbase" => 0x201E,
        "quotedblright" => 0x201D,
        "guillemotright" => 0x00BB,
        "ellipsis" => 0x2026,
        "perthousand" => 0x2030,
        "questiondown" => 0x00BF,
        "grave" => 0x0060,
        "acute" => 0x00B4,
        "circumflex" => 0x005E,
        "tilde" => 0x007E,
        "macron" => 0x00AF,
        "breve" => 0x02D8,
        "dotaccent" => 0x02D9,
        "dieresis" => 0x00A8,
        "ring" => 0x02DA,
        "cedilla" => 0x00B8,
        "hungarumlaut" => 0x02DD,
        "ogonek" => 0x02DB,
        "caron" => 0x02C7,
        "dotlessi" => 0x0131,
        "bracketleft" => 0x005B,
        "backslash" => 0x005C,
        "bracketright" => 0x005D,
        "asciicircum" => 0x005E,
        "underscore" => 0x005F,
        "quoteleft" => 0x2018,
        "braceleft" => 0x007B,
        "bar" => 0x007C,
        "braceright" => 0x007D,
        "asciitilde" => 0x007E,
        "emdash" => 0x2014,
        "Euro" => 0x20AC,
        "zero" => 0x0030,
        "one" => 0x0031,
        "two" => 0x0032,
        "three" => 0x0033,
        "four" => 0x0034,
        "five" => 0x0035,
        "six" => 0x0036,
        "seven" => 0x0037,
        "eight" => 0x0038,
        "nine" => 0x0039,
        "AE" => 0x00C6,
        "ae" => 0x00E6,
        "OE" => 0x0152,
        "oe" => 0x0153,
        "Lslash" => 0x0141,
        "lslash" => 0x0142,
        "Oslash" => 0x00D8,
        "oslash" => 0x00F8,
        "ordfeminine" => 0x00AA,
        "ordmasculine" => 0x00BA,
        "germandbls" => 0x00DF,
        _ if name.len() == 1 => u32::from(name.as_bytes()[0]),
        _ => return None,
    };
    char::from_u32(scalar)
}

fn standard14_width(base_font: &str, _code: u8) -> Option<f64> {
    match base_font {
        "Courier" | "Courier-Bold" | "Courier-Oblique" | "Courier-BoldOblique" => Some(600.0),
        _ => None,
    }
}

fn parse_descriptor(dict: &Dict) -> FontResult<FontDescriptorMetrics> {
    if dict.get("Type").and_then(Value::as_name) != Some("FontDescriptor") {
        return Err(FontError::Invalid(
            "/FontDescriptor has a missing or invalid /Type".to_string(),
        ));
    }
    let font_name = optional_name(dict, "FontName")?;
    let flags = match dict.get("Flags") {
        Some(value) => {
            let flags = value
                .as_integer()
                .and_then(|value| u32::try_from(value).ok())
                .ok_or_else(|| FontError::Invalid("invalid /FontDescriptor /Flags".to_string()))?;
            Some(flags)
        }
        None => None,
    };
    Ok(FontDescriptorMetrics {
        font_name,
        flags,
        font_bbox: optional_bbox(dict, "FontBBox")?,
        italic_angle: optional_number(dict, "ItalicAngle")?,
        ascent: optional_number(dict, "Ascent")?,
        descent: optional_number(dict, "Descent")?,
        cap_height: optional_number(dict, "CapHeight")?,
        stem_v: optional_number(dict, "StemV")?,
        missing_width: optional_number(dict, "MissingWidth")?,
    })
}

fn optional_name(dict: &Dict, key: &str) -> FontResult<Option<String>> {
    match dict.get(key) {
        Some(Value::Name(value)) => Ok(Some(value.clone())),
        Some(_) => Err(FontError::Invalid(format!(
            "invalid /FontDescriptor /{key}"
        ))),
        None => Ok(None),
    }
}

fn optional_number(dict: &Dict, key: &str) -> FontResult<Option<f64>> {
    match dict.get(key) {
        Some(value) => number(value)
            .map(Some)
            .ok_or_else(|| FontError::Invalid(format!("invalid /FontDescriptor /{key}"))),
        None => Ok(None),
    }
}

fn optional_bbox(dict: &Dict, key: &str) -> FontResult<Option<[f64; 4]>> {
    let Some(value) = dict.get(key) else {
        return Ok(None);
    };
    let values = value
        .as_array()
        .ok_or_else(|| FontError::Invalid(format!("invalid /FontDescriptor /{key}")))?;
    if values.len() != 4 {
        return Err(FontError::Invalid(format!(
            "/FontDescriptor /{key} must contain four numbers"
        )));
    }
    let mut bbox = [0.0; 4];
    for (index, value) in values.iter().enumerate() {
        bbox[index] = number(value)
            .ok_or_else(|| FontError::Invalid(format!("invalid /FontDescriptor /{key}")))?;
    }
    Ok(Some(bbox))
}

fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Integer(value) => Some(*value as f64),
        Value::Float(value) if value.is_finite() => Some(*value),
        _ => None,
    }
}

fn is_standard_14(name: &str) -> bool {
    matches!(
        name,
        "Courier"
            | "Courier-Bold"
            | "Courier-Oblique"
            | "Courier-BoldOblique"
            | "Helvetica"
            | "Helvetica-Bold"
            | "Helvetica-Oblique"
            | "Helvetica-BoldOblique"
            | "Times-Roman"
            | "Times-Bold"
            | "Times-Italic"
            | "Times-BoldItalic"
            | "Symbol"
            | "ZapfDingbats"
    )
}
