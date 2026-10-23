use crate::pdf::types::{Document, Value};

use super::{
    CidFontType0, CidFontType2, FontError, FontResult,
    cid_cmap::{CidCMap, ToUnicodeMap, identity_cmap},
};

/// One character code decoded through a Type 0 font's encoding CMap.
#[derive(Debug, Clone, PartialEq)]
pub struct Type0DecodedCode {
    /// Source bytes consumed for this character code.
    pub code: Vec<u8>,
    /// CID selected by the encoding CMap.
    pub cid: u16,
    /// Glyph ID selected by the descendant CIDFont's `/CIDToGIDMap`.
    pub glyph_id: Option<u16>,
    /// Horizontal width selected by `/W` or `/DW`.
    pub width: f64,
    /// Unicode sequence selected by the font's `/ToUnicode` CMap, if present.
    pub unicode: Option<String>,
}

/// The supported descendant types behind a Type 0 parent font.
#[derive(Debug, Clone, PartialEq)]
pub enum CidDescendant {
    /// CIDFontType0 with CID-keyed Type 1/CFF outlines.
    Type0(CidFontType0),
    /// CIDFontType2 with TrueType outlines and `/CIDToGIDMap`.
    Type2(CidFontType2),
}

impl CidDescendant {
    /// Return the descendant font's `/BaseFont` name.
    pub fn base_font(&self) -> &str {
        match self {
            Self::Type0(font) => font.base_font(),
            Self::Type2(font) => font.base_font(),
        }
    }

    /// Return the horizontal width for a CID using `/W` and `/DW`.
    pub fn width_for_cid(&self, cid: u16) -> f64 {
        match self {
            Self::Type0(font) => font.width_for_cid(cid),
            Self::Type2(font) => font.width_for_cid(cid),
        }
    }

    /// Return the glyph ID selected by `/CIDToGIDMap`.
    ///
    /// CIDFontType0 selects glyphs by CID inside the font program, so this
    /// returns `None` for that descendant type.
    pub fn glyph_id_for_cid(&self, cid: u16) -> Option<u16> {
        match self {
            Self::Type0(_) => None,
            Self::Type2(font) => font.glyph_id_for_cid(cid),
        }
    }
}

/// Parsed Type 0 parent font backed by a CIDFontType2 or CIDFontType0 descendant.
#[derive(Debug, Clone, PartialEq)]
pub struct Type0Font {
    base_font: String,
    encoding: CidCMap,
    to_unicode: Option<ToUnicodeMap>,
    descendant: CidDescendant,
}

impl Type0Font {
    /// Return the Type 0 parent font's `/BaseFont` name.
    pub fn base_font(&self) -> &str {
        &self.base_font
    }

    /// Whether the encoding CMap selects vertical writing mode.
    pub fn is_vertical(&self) -> bool {
        self.encoding.writing_mode() == 1
    }

    /// Return the parsed descendant CID font.
    pub fn descendant(&self) -> &CidDescendant {
        &self.descendant
    }

    /// Decode a byte string into character codes, CIDs, glyph IDs, and widths.
    pub fn decode_codes(&self, input: &[u8]) -> FontResult<Vec<Type0DecodedCode>> {
        self.encoding
            .decode(input)?
            .into_iter()
            .map(|(code, cid)| {
                let unicode = self
                    .to_unicode
                    .as_ref()
                    .and_then(|map| map.unicode_for(&code).map(str::to_owned));
                Ok(Type0DecodedCode {
                    code,
                    cid,
                    glyph_id: self.descendant.glyph_id_for_cid(cid),
                    width: self.descendant.width_for_cid(cid),
                    unicode,
                })
            })
            .collect()
    }
}

impl Document {
    /// Parse a Type 0 composite font with a CIDFontType2 or CIDFontType0
    /// descendant.
    ///
    /// The predefined `/Identity-H` and `/Identity-V` encodings and embedded
    /// CMaps with codespace, `cidchar`, and `cidrange` sections are supported.
    /// Embedded CMaps may inherit `/Identity-H` or `/Identity-V` through
    /// `usecmap`; other predefined base CMaps are not supported.
    ///
    /// Returns `Ok(None)` for values whose `/Subtype` is not `/Type0`.
    pub fn type0_font(&self, value: &Value) -> FontResult<Option<Type0Font>> {
        let value = self.resolve_value(value)?;
        let dict = value
            .as_dict()
            .ok_or_else(|| FontError::Invalid("font value is not a dictionary".into()))?;
        if dict.get("Subtype").and_then(Value::as_name) != Some("Type0") {
            return Ok(None);
        }
        let base_font = dict
            .get("BaseFont")
            .and_then(Value::as_name)
            .ok_or_else(|| FontError::Invalid("Type 0 font is missing /BaseFont".into()))?
            .to_owned();
        let encoding_value = dict
            .get("Encoding")
            .ok_or_else(|| FontError::Invalid("Type 0 font is missing /Encoding".into()))?;
        let encoding_value = self.resolve_value(encoding_value)?;
        let encoding = match encoding_value {
            Value::Name(name) if name == "Identity-H" => identity_cmap(0),
            Value::Name(name) if name == "Identity-V" => identity_cmap(1),
            Value::Name(name) => {
                return Err(FontError::Invalid(format!(
                    "unsupported predefined Type 0 encoding /{name}"
                )));
            }
            Value::Stream(_, _) => {
                let bytes = self.decode_stream_bytes(&encoding_value).map_err(|error| {
                    FontError::Invalid(format!(
                        "failed to decode Type 0 /Encoding CMap stream: {error}"
                    ))
                })?;
                CidCMap::parse_inheriting(&bytes, |name| match name {
                    "Identity-H" => Ok(identity_cmap(0)),
                    "Identity-V" => Ok(identity_cmap(1)),
                    _ => Err(FontError::Invalid(format!(
                        "unsupported predefined CMap /{name} in usecmap"
                    ))),
                })?
            }
            _ => {
                return Err(FontError::Invalid(
                    "Type 0 /Encoding is neither a name nor a CMap stream".into(),
                ));
            }
        };
        let to_unicode = match dict.get("ToUnicode") {
            Some(value) => {
                let value = self.resolve_value(value)?;
                if !matches!(value, Value::Stream(_, _)) {
                    return Err(FontError::Invalid(
                        "Type 0 /ToUnicode is not a CMap stream".into(),
                    ));
                }
                let bytes = self.decode_stream_bytes(&value).map_err(|error| {
                    FontError::Invalid(format!(
                        "failed to decode Type 0 /ToUnicode CMap stream: {error}"
                    ))
                })?;
                Some(ToUnicodeMap::parse(&bytes)?)
            }
            None => None,
        };

        let descendants = dict
            .get("DescendantFonts")
            .ok_or_else(|| FontError::Invalid("Type 0 font is missing /DescendantFonts".into()))?;
        let descendants = self.resolve_value(descendants)?;
        let descendants = descendants
            .as_array()
            .ok_or_else(|| FontError::Invalid("Type 0 /DescendantFonts is not an array".into()))?;
        if descendants.len() != 1 {
            return Err(FontError::Invalid(
                "Type 0 /DescendantFonts must contain exactly one font".into(),
            ));
        }
        let descendant_value = self.resolve_value(&descendants.0[0])?;
        let descendant = match self.cid_font_type2(&descendant_value)? {
            Some(font) => CidDescendant::Type2(font),
            None => match self.cid_font_type0(&descendant_value)? {
                Some(font) => CidDescendant::Type0(font),
                None => {
                    return Err(FontError::Invalid(
                        "Type 0 descendant font is neither /CIDFontType2 nor /CIDFontType0".into(),
                    ));
                }
            },
        };
        Ok(Some(Type0Font {
            base_font,
            encoding,
            to_unicode,
            descendant,
        }))
    }
}

#[cfg(test)]
mod tests {
    use crate::pdf::types::{Array, Dict, Document, StreamData, Value};

    use super::CidDescendant;

    fn cid_descendant() -> Value {
        Value::Dict(
            Dict::new()
                .insert("Subtype", Value::Name("CIDFontType2".into()))
                .insert("BaseFont", Value::Name("ExampleCIDFont".into()))
                .insert(
                    "CIDSystemInfo",
                    Value::Dict(
                        Dict::new()
                            .insert("Registry", Value::String("Adobe".into()))
                            .insert("Ordering", Value::String("Identity".into()))
                            .insert("Supplement", Value::Integer(0)),
                    ),
                )
                .insert(
                    "FontDescriptor",
                    Value::Dict(Dict::new().insert("Type", Value::Name("FontDescriptor".into()))),
                )
                .insert(
                    "W",
                    Value::Array(
                        Array::new()
                            .push(Value::Integer(65))
                            .push(Value::Array(Array::new().push(Value::Integer(500)))),
                    ),
                ),
        )
    }

    fn type0_font(encoding: Value) -> Value {
        type0_font_with_to_unicode(encoding, None)
    }

    fn type0_font_with_to_unicode(encoding: Value, to_unicode: Option<Value>) -> Value {
        Value::Dict({
            let mut dict = Dict::new()
                .insert("Subtype", Value::Name("Type0".into()))
                .insert("BaseFont", Value::Name("ExampleComposite".into()))
                .insert("Encoding", encoding)
                .insert(
                    "DescendantFonts",
                    Value::Array(Array::new().push(cid_descendant())),
                );
            if let Some(to_unicode) = to_unicode {
                dict = dict.insert("ToUnicode", to_unicode);
            }
            dict
        })
    }

    #[test]
    fn maps_identity_h_codes_through_cid_font_metrics() {
        let document = Document::new();
        let font = document
            .type0_font(&type0_font(Value::Name("Identity-H".into())))
            .unwrap()
            .unwrap();
        assert!(!font.is_vertical());
        assert_eq!(font.base_font(), "ExampleComposite");
        let decoded = font.decode_codes(&[0, 65, 0, 66]).unwrap();
        assert_eq!(decoded.len(), 2);
        assert_eq!(decoded[0].code, [0, 65]);
        assert_eq!(decoded[0].cid, 65);
        assert_eq!(decoded[0].glyph_id, Some(65));
        assert_eq!(decoded[0].width, 500.0);
        assert_eq!(decoded[0].unicode, None);
        assert_eq!(decoded[1].cid, 66);
        assert_eq!(decoded[1].width, 1000.0);
    }

    #[test]
    fn decodes_embedded_cmap_and_vertical_mode() {
        let document = Document::new();
        let cmap = b"
            /WMode 1 def
            1 begincodespacerange <20> <7E> endcodespacerange
            1 begincidchar <41> 65 endcidchar";
        let font = document
            .type0_font(&type0_font(Value::Stream(
                Dict::new(),
                StreamData::Owned(cmap.to_vec()),
            )))
            .unwrap()
            .unwrap();
        assert!(font.is_vertical());
        assert_eq!(font.decode_codes(b"A").unwrap()[0].cid, 65);
    }

    #[test]
    fn embedded_cmap_inherits_identity_usecmap() {
        let document = Document::new();
        let cmap = b"
            /Identity-H usecmap
            1 begincodespacerange <0000> <FFFF> endcodespacerange
            1 begincidchar <0041> 7 endcidchar";
        let font = document
            .type0_font(&type0_font(Value::Stream(
                Dict::new(),
                StreamData::Owned(cmap.to_vec()),
            )))
            .unwrap()
            .unwrap();
        let decoded = font.decode_codes(&[0, 0x41, 0, 0x42]).unwrap();
        assert_eq!(decoded[0].cid, 7);
        assert_eq!(decoded[1].cid, 0x42);
    }

    #[test]
    fn rejects_unsupported_usecmap_bases() {
        let document = Document::new();
        let cmap = b"/UniJIS-UCS2-H usecmap 1 begincodespacerange <0000> <FFFF> endcodespacerange";
        assert!(
            document
                .type0_font(&type0_font(Value::Stream(
                    Dict::new(),
                    StreamData::Owned(cmap.to_vec()),
                )))
                .is_err()
        );
    }

    #[test]
    fn maps_decoded_codes_to_unicode_sequences() {
        let document = Document::new();
        let to_unicode = b"
            1 begincodespacerange <0000> <FFFF> endcodespacerange
            2 beginbfchar
              <0041> <0041>
              <0042> <D83DDE00>
            endbfchar
            1 beginbfrange
              <0050> <0052> <0061>
            endbfrange";
        let font = document
            .type0_font(&type0_font_with_to_unicode(
                Value::Name("Identity-H".into()),
                Some(Value::Stream(
                    Dict::new(),
                    StreamData::Owned(to_unicode.to_vec()),
                )),
            ))
            .unwrap()
            .unwrap();

        let decoded = font.decode_codes(&[0, 0x41, 0, 0x42, 0, 0x50]).unwrap();
        assert_eq!(decoded[0].unicode.as_deref(), Some("A"));
        assert_eq!(decoded[1].unicode.as_deref(), Some("😀"));
        assert_eq!(decoded[2].unicode.as_deref(), Some("a"));
    }

    #[test]
    fn rejects_invalid_tounicode_values_and_streams() {
        let document = Document::new();
        let non_stream = type0_font_with_to_unicode(
            Value::Name("Identity-H".into()),
            Some(Value::Name("Identity-H".into())),
        );
        assert!(document.type0_font(&non_stream).is_err());

        let malformed = type0_font_with_to_unicode(
            Value::Name("Identity-H".into()),
            Some(Value::Stream(
                Dict::new(),
                StreamData::Owned(b"1 beginbfchar <41> <D800> endbfchar".to_vec()),
            )),
        );
        assert!(document.type0_font(&malformed).is_err());
    }

    #[test]
    fn rejects_unsupported_type0_encodings_and_descendants() {
        let document = Document::new();
        assert!(
            document
                .type0_font(&type0_font(Value::Name("UniJIS-UTF16-H".into())))
                .is_err()
        );
        assert!(
            document
                .type0_font(&type0_font(Value::Name("Identity-H".into())))
                .is_ok()
        );
        assert!(
            document
                .type0_font(&type0_font(Value::Array(Array::new())))
                .is_err()
        );
        assert!(
            document
                .type0_font(&Value::Dict(
                    Dict::new().insert("Subtype", Value::Name("Type1".into()))
                ))
                .unwrap()
                .is_none()
        );
    }

    fn cid_type0_descendant() -> Value {
        Value::Dict(
            Dict::new()
                .insert("Subtype", Value::Name("CIDFontType0".into()))
                .insert("BaseFont", Value::Name("ExampleCIDType0".into()))
                .insert(
                    "CIDSystemInfo",
                    Value::Dict(
                        Dict::new()
                            .insert("Registry", Value::String("Adobe".into()))
                            .insert("Ordering", Value::String("Identity".into()))
                            .insert("Supplement", Value::Integer(0)),
                    ),
                )
                .insert(
                    "FontDescriptor",
                    Value::Dict(
                        Dict::new()
                            .insert("Type", Value::Name("FontDescriptor".into()))
                            .insert(
                                "FontFile3",
                                Value::Stream(
                                    Dict::new()
                                        .insert("Subtype", Value::Name("CIDFontType0C".into())),
                                    StreamData::Owned(b"CFF ".to_vec()),
                                ),
                            ),
                    ),
                )
                .insert(
                    "W",
                    Value::Array(
                        Array::new()
                            .push(Value::Integer(65))
                            .push(Value::Array(Array::new().push(Value::Integer(500)))),
                    ),
                ),
        )
    }

    #[test]
    fn accepts_cid_font_type0_descendant_without_glyph_mapping() {
        let document = Document::new();
        let font = Value::Dict(
            Dict::new()
                .insert("Subtype", Value::Name("Type0".into()))
                .insert("BaseFont", Value::Name("ExampleComposite".into()))
                .insert("Encoding", Value::Name("Identity-H".into()))
                .insert(
                    "DescendantFonts",
                    Value::Array(Array::new().push(cid_type0_descendant())),
                ),
        );
        let font = document.type0_font(&font).unwrap().unwrap();
        let decoded = font.decode_codes(&[0, 65, 0, 66]).unwrap();
        assert_eq!(decoded[0].cid, 65);
        assert_eq!(decoded[0].glyph_id, None);
        assert_eq!(decoded[0].width, 500.0);
        assert_eq!(decoded[1].width, 1000.0);
        assert!(matches!(font.descendant(), CidDescendant::Type0(_)));
        assert_eq!(font.descendant().base_font(), "ExampleCIDType0");
    }
}
