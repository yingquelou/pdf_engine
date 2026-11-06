use crate::core::types::{Document, Value};

use super::{
    CidFontType0, CidFontType2, FontError, FontResult,
    cid_cmap::{CidCMap, ToUnicodeMap, identity_cmap, parse_to_unicode_stream, resolve_cid_base},
};

/// Resolve a CMap base named by an in-stream `usecmap` operator or by a
/// stream dictionary's `/UseCMap` entry (Table 120) against the built-in
/// roster.
fn predefined_cid_base(name: &str) -> FontResult<CidCMap> {
    match name {
        "Identity-H" => Ok(identity_cmap(0)),
        "Identity-V" => Ok(identity_cmap(1)),
        // 名册内的预定义 CMap（如 Adobe-GB1、UniGB-UCS2-H、GB-EUC-H）按名称解析。
        _ => {
            super::predefined_cmaps::resolve_predefined_cmap(name).map(|cmap| cmap.into_cid_cmap())
        }
    }
}

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
                    .and_then(|map| map.resolve_unicode(&code));
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
    /// `/Encoding` may also reference a predefined CMap from the
    /// `predefined_cmaps` roster by name; embedded CMaps may inherit
    /// `/Identity-H`, `/Identity-V`, or a roster CMap through `usecmap`, and
    /// may inherit either a predefined name or another embedded CMap stream
    /// through the stream dictionary's `/UseCMap` entry (9.7.5.3, Table 120).
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
            // 名册内的预定义 CMap（如 Adobe-GB1、UniGB-UCS2-H、GB-EUC-H）允许按名称引用。
            Value::Name(name) => super::predefined_cmaps::resolve_predefined_cmap(&name)
                .map(|cmap| cmap.into_cid_cmap())
                .map_err(|_| {
                    FontError::Invalid(format!("unsupported predefined Type 0 encoding /{name}"))
                })?,
            Value::Stream(_, _) => {
                let bytes = self.decode_stream_bytes(&encoding_value).map_err(|error| {
                    FontError::Invalid(format!(
                        "failed to decode Type 0 /Encoding CMap stream: {error}"
                    ))
                })?;
                // Table 120：/Encoding CMap 流字典的 /UseCMap 可以是预定义名，
                // 也可以是另一个内嵌 CMap 流；基座不可解析时退回只用本地映射。
                let dict_base = encoding_value
                    .as_dict()
                    .and_then(|dict| resolve_cid_base(self, dict, predefined_cid_base));
                CidCMap::parse_with_base(&bytes, dict_base, predefined_cid_base)?
            }
            _ => {
                return Err(FontError::Invalid(
                    "Type 0 /Encoding is neither a name nor a CMap stream".into(),
                ));
            }
        };
        // `/ToUnicode` 是可选的文本映射；流损坏或引用不可解析时退化为按
        // 编码解码。`Adobe-Identity-UCS` 基座按内置恒等关系解析，流字典的
        // `/UseCMap`（Table 120）可指向预定义名或内嵌基座流；其余未知基座
        // 退化为仅使用本地映射，而不是让整个字体不可用。
        let to_unicode = dict
            .get("ToUnicode")
            .and_then(|value| self.resolve_value(value).ok())
            .and_then(|value| parse_to_unicode_stream(self, &value));

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
    use crate::core::types::{Array, Dict, Document, StreamData, Value};

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
        let cmap =
            b"/UniCNS-UTF16-HW usecmap 1 begincodespacerange <0000> <FFFF> endcodespacerange";
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
    fn ignores_invalid_tounicode_values_and_streams() {
        let document = Document::new();
        let non_stream = type0_font_with_to_unicode(
            Value::Name("Identity-H".into()),
            Some(Value::Name("Identity-H".into())),
        );
        let font = document.type0_font(&non_stream).unwrap().unwrap();
        assert_eq!(font.decode_codes(&[0, 0x41]).unwrap()[0].unicode, None);

        let malformed = type0_font_with_to_unicode(
            Value::Name("Identity-H".into()),
            Some(Value::Stream(
                Dict::new(),
                StreamData::Owned(b"1 beginbfchar <41> <D800> endbfchar".to_vec()),
            )),
        );
        let font = document.type0_font(&malformed).unwrap().unwrap();
        assert_eq!(
            font.decode_codes(&[0, 0x41]).unwrap()[0].unicode,
            None,
            "损坏的 ToUnicode 应被忽略而不是让字体不可用"
        );
    }

    #[test]
    fn resolves_predefined_encoding_names_by_reference() {
        let document = Document::new();
        // UniGB-UCS2-H：真实 Adobe CMap，UCS-2 码位 → GB1 CID（`<0020> <007e> 1`）。
        let font = document
            .type0_font(&type0_font(Value::Name("UniGB-UCS2-H".into())))
            .unwrap()
            .unwrap();
        assert!(!font.is_vertical());
        let decoded = font.decode_codes(&[0x4E, 0x2D]).unwrap();
        assert_eq!(decoded[0].code, [0x4E, 0x2D]);
        assert_eq!(decoded[0].cid, 4559);

        // Adobe-GB1：混合 1/2 字节 codespace。
        let font = document
            .type0_font(&type0_font(Value::Name("Adobe-GB1".into())))
            .unwrap()
            .unwrap();
        assert!(!font.is_vertical());
        assert_eq!(font.decode_codes(&[0xA1, 0xA1]).unwrap()[0].cid, 0x0100);
        assert_eq!(font.decode_codes(&[0x41]).unwrap()[0].cid, 0x41);
    }

    #[test]
    fn rejects_unsupported_type0_encodings_and_descendants() {
        let document = Document::new();
        assert!(
            document
                .type0_font(&type0_font(Value::Name("UniJIS-UTF16-HW".into())))
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

    #[test]
    fn resolves_vendored_predefined_encoding_by_name() {
        let document = Document::new();
        let font = document
            .type0_font(&type0_font(Value::Name("GB-EUC-H".into())))
            .unwrap()
            .unwrap();
        assert!(!font.is_vertical());
        let decoded = font.decode_codes(&[0xA1, 0xA1, 0xB0, 0xA1, 0x41]).unwrap();
        assert_eq!(decoded[0].code, [0xA1, 0xA1]);
        assert_eq!(decoded[0].cid, 96);
        assert_eq!(decoded[1].cid, 940);
        assert_eq!(decoded[2].code, [0x41]);
        assert_eq!(decoded[2].cid, 846);
    }

    #[test]
    fn vendored_vertical_encoding_sets_vertical_mode() {
        let document = Document::new();
        let font = document
            .type0_font(&type0_font(Value::Name("GB-V".into())))
            .unwrap()
            .unwrap();
        assert!(font.is_vertical());
        let decoded = font.decode_codes(&[0x21, 0x21, 0x21, 0x22]).unwrap();
        assert_eq!(decoded[0].cid, 96);
        assert_eq!(decoded[1].cid, 575);
    }

    #[test]
    fn rejects_predefined_encoding_outside_the_vendored_roster() {
        let document = Document::new();
        // UniCNS-UCS2-HW 不是 Adobe 发布的变体名；必须报错，而不是合成
        // 看似完整的映射。
        let error = document
            .type0_font(&type0_font(Value::Name("UniCNS-UCS2-HW".into())))
            .expect_err("名册外的变体名应报错");
        assert!(
            error
                .to_string()
                .contains("unsupported predefined Type 0 encoding")
        );
    }

    #[test]
    fn vendored_unicode_encoding_decodes_variable_length_codes() {
        let document = Document::new();
        let font = document
            .type0_font(&type0_font(Value::Name("UniJIS-UTF16-H".into())))
            .unwrap()
            .unwrap();
        assert!(!font.is_vertical());
        // 4 字节代理对（U+2000B）与 2 字节 BMP 码同流解码。
        let decoded = font
            .decode_codes(&[0xD8, 0x40, 0xDC, 0x0B, 0x4E, 0x2D])
            .unwrap();
        assert_eq!(decoded[0].cid, 13839);
        assert_eq!(decoded[1].cid, 2980);
    }

    #[test]
    fn to_unicode_without_codespace_maps_local_bfchar_codes() {
        let document = Document::new();
        let to_unicode = b"1 beginbfchar <0041> <0041> endbfchar";
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
        let decoded = font.decode_codes(&[0, 0x41, 0, 0x42]).unwrap();
        assert_eq!(decoded[0].unicode.as_deref(), Some("A"));
        assert_eq!(decoded[1].unicode, None);
    }
}
