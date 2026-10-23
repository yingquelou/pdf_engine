use std::collections::BTreeMap;

use crate::core::types::{Array, Dict, Document, Value};

use super::{
    FontDescriptorMetrics, FontError, FontResult, number, parse_descriptor, truetype::TrueTypeFont,
};

/// Parsed `/CIDSystemInfo` dictionary for a CID-keyed font.
///
/// Identifies the character collection: `/Registry` (e.g. "Adobe"),
/// `/Ordering` (e.g. "GB1"), and `/Supplement` (revision number).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CidSystemInfo {
    /// `/Registry` string (e.g. "Adobe").
    pub registry: String,
    /// `/Ordering` string (e.g. "GB1", "Japan1", "Korea1").
    pub ordering: String,
    /// `/Supplement` non-negative integer.
    pub supplement: i64,
}

#[derive(Debug, Clone, PartialEq)]
enum CidToGidMap {
    Identity,
    Explicit(Vec<u8>),
}

/// Parsed metrics and CID-to-glyph mapping for a PDF CIDFontType2.
#[derive(Debug, Clone, PartialEq)]
pub struct CidFontType2 {
    base_font: String,
    system_info: CidSystemInfo,
    default_width: f64,
    widths: BTreeMap<u16, f64>,
    descriptor: FontDescriptorMetrics,
    cid_to_gid: CidToGidMap,
    true_type_font: Option<TrueTypeFont>,
}

impl CidFontType2 {
    /// Return the descendant font's `/BaseFont` name.
    pub fn base_font(&self) -> &str {
        &self.base_font
    }

    /// Return the parsed `/CIDSystemInfo` (Registry, Ordering, Supplement).
    pub fn system_info(&self) -> &CidSystemInfo {
        &self.system_info
    }

    /// Return `/DW`, defaulting to 1000 when omitted.
    pub fn default_width(&self) -> f64 {
        self.default_width
    }

    /// Return the horizontal width for a CID using `/W` and `/DW`.
    pub fn width_for_cid(&self, cid: u16) -> f64 {
        self.widths.get(&cid).copied().unwrap_or(self.default_width)
    }

    /// Map a CID through `/CIDToGIDMap`, defaulting to identity.
    ///
    /// An explicit map that does not contain this CID returns `None`.
    pub fn glyph_id_for_cid(&self, cid: u16) -> Option<u16> {
        let glyph_id = match &self.cid_to_gid {
            CidToGidMap::Identity => Some(cid),
            CidToGidMap::Explicit(bytes) => {
                let offset = usize::from(cid).checked_mul(2)?;
                let pair = bytes.get(offset..offset.checked_add(2)?)?;
                Some(u16::from_be_bytes([pair[0], pair[1]]))
            }
        }?;
        self.true_type_font
            .as_ref()
            .is_none_or(|font| glyph_id < font.glyph_count())
            .then_some(glyph_id)
    }

    /// Return the parsed `/FontDescriptor` metrics.
    pub fn descriptor(&self) -> &FontDescriptorMetrics {
        &self.descriptor
    }

    /// Return the embedded TrueType program when `/FontFile2` is present.
    pub fn true_type_font(&self) -> Option<&TrueTypeFont> {
        self.true_type_font.as_ref()
    }
}

impl Document {
    /// Parse a CIDFontType2 descendant font's CID metrics and glyph mapping.
    ///
    /// `/CIDToGIDMap` may be `/Identity` or a stream of big-endian 16-bit
    /// glyph IDs. `/W` and `/DW` provide widths in glyph-space units. An
    /// embedded `/FontFile2` is parsed as a TrueType font when present.
    ///
    /// Returns `Ok(None)` for values whose `/Subtype` is not `/CIDFontType2`.
    pub fn cid_font_type2(&self, value: &Value) -> FontResult<Option<CidFontType2>> {
        let value = self.resolve_value(value)?;
        let dict = value
            .as_dict()
            .ok_or_else(|| FontError::Invalid("CID font value is not a dictionary".into()))?;
        if dict.get("Subtype").and_then(Value::as_name) != Some("CIDFontType2") {
            return Ok(None);
        }

        let metrics = parse_cid_metrics(self, dict)?;

        let cid_to_gid = match dict.get("CIDToGIDMap") {
            None => CidToGidMap::Identity,
            Some(value) => {
                let value = self.resolve_value(value)?;
                match value {
                    Value::Name(name) if name == "Identity" => CidToGidMap::Identity,
                    Value::Stream(_, _) => {
                        let bytes = self.decode_stream_bytes(&value).map_err(|error| {
                            FontError::Invalid(format!(
                                "failed to decode CID font /CIDToGIDMap stream: {error}"
                            ))
                        })?;
                        if bytes.len() % 2 != 0 {
                            return Err(FontError::Invalid(
                                "CID font /CIDToGIDMap stream has an odd byte length".into(),
                            ));
                        }
                        if bytes.len() > (usize::from(u16::MAX) + 1) * 2 {
                            return Err(FontError::Invalid(
                                "CID font /CIDToGIDMap stream exceeds the CID range".into(),
                            ));
                        }
                        CidToGidMap::Explicit(bytes)
                    }
                    _ => {
                        return Err(FontError::Invalid(
                            "CID font /CIDToGIDMap is neither /Identity nor a stream".into(),
                        ));
                    }
                }
            }
        };

        let true_type_font = self
            .cid_font_type2_program(&value)?
            .map(|program| TrueTypeFont::parse(program.data))
            .transpose()?;
        Ok(Some(CidFontType2 {
            base_font: metrics.base_font,
            system_info: metrics.system_info,
            default_width: metrics.default_width,
            widths: metrics.widths,
            descriptor: metrics.descriptor,
            cid_to_gid,
            true_type_font,
        }))
    }
}

/// CID-keyed font metrics shared by CIDFontType0 and CIDFontType2.
pub(super) struct CidMetrics {
    pub(super) base_font: String,
    pub(super) system_info: CidSystemInfo,
    pub(super) default_width: f64,
    pub(super) widths: BTreeMap<u16, f64>,
    pub(super) descriptor: FontDescriptorMetrics,
}

/// Parse the metrics common to both CID-keyed descendant font types.
pub(super) fn parse_cid_metrics(document: &Document, dict: &Dict) -> FontResult<CidMetrics> {
    let base_font = dict
        .get("BaseFont")
        .and_then(Value::as_name)
        .ok_or_else(|| FontError::Invalid("missing or invalid CID font /BaseFont".into()))?
        .to_owned();
    let system_info = parse_cid_system_info(document, dict)?;
    let descriptor = dict
        .get("FontDescriptor")
        .ok_or_else(|| FontError::Invalid("CID font is missing /FontDescriptor".into()))?;
    let descriptor = document.resolve_value(descriptor)?;
    let descriptor = descriptor
        .as_dict()
        .ok_or_else(|| FontError::Invalid("CID font /FontDescriptor is not a dictionary".into()))?;
    if descriptor.get("Type").and_then(Value::as_name) != Some("FontDescriptor") {
        return Err(FontError::Invalid(
            "CID font /FontDescriptor has a missing or invalid /Type".into(),
        ));
    }
    let descriptor = parse_descriptor(document, descriptor)?;

    let default_width = match dict.get("DW") {
        Some(value) => {
            let value = document.resolve_value(value)?;
            number(&value)
                .ok_or_else(|| FontError::Invalid("CID font /DW is not a finite number".into()))?
        }
        None => 1000.0,
    };
    let widths = match dict.get("W") {
        Some(value) => {
            let value = document.resolve_value(value)?;
            let array = value
                .as_array()
                .ok_or_else(|| FontError::Invalid("CID font /W is not an array".into()))?;
            parse_widths(document, array)?
        }
        None => BTreeMap::new(),
    };
    Ok(CidMetrics {
        base_font,
        system_info,
        default_width,
        widths,
        descriptor,
    })
}

fn parse_cid_system_info(document: &Document, dict: &Dict) -> FontResult<CidSystemInfo> {
    let value = dict
        .get("CIDSystemInfo")
        .ok_or_else(|| FontError::Invalid("CID font is missing /CIDSystemInfo".into()))?;
    let value = document.resolve_value(value)?;
    let info = value
        .as_dict()
        .ok_or_else(|| FontError::Invalid("CID font /CIDSystemInfo is not a dictionary".into()))?;

    let registry = info
        .get("Registry")
        .ok_or_else(|| FontError::Invalid("CIDSystemInfo is missing /Registry".into()))?;
    let registry = document.resolve_value(registry)?;
    let registry = match registry {
        Value::String(s) | Value::HexString(s) => s.clone(),
        _ => {
            return Err(FontError::Invalid(
                "CIDSystemInfo /Registry is not a string".into(),
            ));
        }
    };

    let ordering = info
        .get("Ordering")
        .ok_or_else(|| FontError::Invalid("CIDSystemInfo is missing /Ordering".into()))?;
    let ordering = document.resolve_value(ordering)?;
    let ordering = match ordering {
        Value::String(s) | Value::HexString(s) => s.clone(),
        _ => {
            return Err(FontError::Invalid(
                "CIDSystemInfo /Ordering is not a string".into(),
            ));
        }
    };

    let supplement = info
        .get("Supplement")
        .ok_or_else(|| FontError::Invalid("CIDSystemInfo is missing /Supplement".into()))?;
    let supplement = document.resolve_value(supplement)?;
    let supplement = match supplement {
        Value::Integer(value) if value >= 0 => value,
        _ => {
            return Err(FontError::Invalid(
                "CIDSystemInfo /Supplement is not a nonnegative integer".into(),
            ));
        }
    };

    Ok(CidSystemInfo {
        registry,
        ordering,
        supplement,
    })
}

fn parse_widths(document: &Document, values: &Array) -> FontResult<BTreeMap<u16, f64>> {
    let mut widths = BTreeMap::new();
    let mut index = 0;
    while index < values.len() {
        let first = resolve_cid(document, &values.0[index])?;
        let next = values
            .0
            .get(index + 1)
            .ok_or_else(|| FontError::Invalid("CID font /W has a truncated entry".into()))?;
        let next = document.resolve_value(next)?;
        if let Some(width_array) = next.as_array() {
            if width_array.is_empty() {
                return Err(FontError::Invalid(
                    "CID font /W contains an empty width array".into(),
                ));
            }
            for (offset, value) in width_array.iter().enumerate() {
                let cid = first
                    .checked_add(u16::try_from(offset).map_err(|_| {
                        FontError::Invalid("CID font /W width array is too long".into())
                    })?)
                    .ok_or_else(|| FontError::Invalid("CID font /W CID overflows".into()))?;
                insert_width(document, &mut widths, cid, value)?;
            }
            index += 2;
        } else {
            let last = integer_cid(&next)?;
            if last < first {
                return Err(FontError::Invalid(
                    "CID font /W range ends before it starts".into(),
                ));
            }
            let width_value = values
                .0
                .get(index + 2)
                .ok_or_else(|| FontError::Invalid("CID font /W has a truncated range".into()))?;
            let width_value = document.resolve_value(width_value)?;
            let width = number(&width_value)
                .ok_or_else(|| FontError::Invalid("CID font /W width is not finite".into()))?;
            for cid in first..=last {
                if widths.insert(cid, width).is_some() {
                    return Err(FontError::Invalid(
                        "CID font /W assigns a CID more than once".into(),
                    ));
                }
            }
            index += 3;
        }
    }
    Ok(widths)
}

fn resolve_cid(document: &Document, value: &Value) -> FontResult<u16> {
    integer_cid(&document.resolve_value(value)?)
}

fn integer_cid(value: &Value) -> FontResult<u16> {
    match value {
        Value::Integer(cid) => u16::try_from(*cid)
            .map_err(|_| FontError::Invalid("CID font /W CID is outside 0..=65535".into())),
        _ => Err(FontError::Invalid(
            "CID font /W CID is not an integer".into(),
        )),
    }
}

fn insert_width(
    document: &Document,
    widths: &mut BTreeMap<u16, f64>,
    cid: u16,
    value: &Value,
) -> FontResult<()> {
    let value = document.resolve_value(value)?;
    let width = number(&value)
        .ok_or_else(|| FontError::Invalid("CID font /W width is not finite".into()))?;
    if widths.insert(cid, width).is_some() {
        return Err(FontError::Invalid(
            "CID font /W assigns a CID more than once".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::core::types::{Array, Dict, Document, Object, StreamData, Value};

    fn cid_system_info() -> Value {
        Value::Dict(
            Dict::new()
                .insert("Registry", Value::String("Adobe".into()))
                .insert("Ordering", Value::String("Identity".into()))
                .insert("Supplement", Value::Integer(0)),
        )
    }

    fn cid_font(entries: Vec<(&str, Value)>) -> Value {
        cid_font_with_descriptor(entries, Vec::new())
    }

    fn cid_font_with_descriptor(
        entries: Vec<(&str, Value)>,
        descriptor_entries: Vec<(&str, Value)>,
    ) -> Value {
        let mut descriptor = Dict::new().insert("Type", Value::Name("FontDescriptor".into()));
        for (key, value) in descriptor_entries {
            descriptor = descriptor.insert(key, value);
        }
        let mut dict = Dict::new()
            .insert("Subtype", Value::Name("CIDFontType2".into()))
            .insert("BaseFont", Value::Name("ExampleCIDFont".into()))
            .insert("CIDSystemInfo", cid_system_info())
            .insert("FontDescriptor", Value::Dict(descriptor));
        for (key, value) in entries {
            dict = dict.insert(key, value);
        }
        Value::Dict(dict)
    }

    #[test]
    fn parses_cid_to_gid_map_and_both_width_forms() {
        let document = Document::new();
        let font = cid_font(vec![
            (
                "CIDToGIDMap",
                Value::Stream(Dict::new(), StreamData::Owned(vec![0, 4, 0, 9, 0, 10])),
            ),
            (
                "W",
                Value::Array(
                    Array::new()
                        .push(Value::Integer(1))
                        .push(Value::Array(
                            Array::new()
                                .push(Value::Integer(500))
                                .push(Value::Float(510.5)),
                        ))
                        .push(Value::Integer(5))
                        .push(Value::Integer(6))
                        .push(Value::Integer(700)),
                ),
            ),
            ("DW", Value::Integer(950)),
        ]);

        let font = document.cid_font_type2(&font).unwrap().unwrap();
        assert_eq!(font.base_font(), "ExampleCIDFont");
        assert_eq!(font.default_width(), 950.0);
        assert_eq!(font.glyph_id_for_cid(0), Some(4));
        assert_eq!(font.glyph_id_for_cid(1), Some(9));
        assert_eq!(font.glyph_id_for_cid(2), Some(10));
        assert_eq!(font.glyph_id_for_cid(3), None);
        assert_eq!(font.width_for_cid(0), 950.0);
        assert_eq!(font.width_for_cid(1), 500.0);
        assert_eq!(font.width_for_cid(2), 510.5);
        assert_eq!(font.width_for_cid(5), 700.0);
        assert_eq!(font.width_for_cid(6), 700.0);
        assert!(font.true_type_font().is_none());
    }

    #[test]
    fn defaults_to_identity_cid_mapping_and_width() {
        let document = Document::new();
        let font = document
            .cid_font_type2(&cid_font(Vec::new()))
            .unwrap()
            .unwrap();
        assert_eq!(font.glyph_id_for_cid(0), Some(0));
        assert_eq!(font.glyph_id_for_cid(u16::MAX), Some(u16::MAX));
        assert_eq!(font.width_for_cid(123), 1000.0);
    }

    #[test]
    fn resolves_indirect_widths_and_cid_to_gid_streams() {
        let document = Document::new();
        document.set_object(Object::new(
            10,
            0,
            Value::Array(Array::new().push(Value::Integer(650))),
        ));
        document.set_object(Object::new(
            11,
            0,
            Value::Array(
                Array::new()
                    .push(Value::Integer(12))
                    .push(Value::Ref(10, 0)),
            ),
        ));
        document.set_object(Object::new(
            12,
            0,
            Value::Stream(Dict::new(), StreamData::Owned(vec![0, 22, 0, 23])),
        ));
        let font = cid_font(vec![
            ("W", Value::Ref(11, 0)),
            ("CIDToGIDMap", Value::Ref(12, 0)),
        ]);

        let font = document.cid_font_type2(&font).unwrap().unwrap();
        assert_eq!(font.width_for_cid(12), 650.0);
        assert_eq!(font.glyph_id_for_cid(0), Some(22));
        assert_eq!(font.glyph_id_for_cid(1), Some(23));
    }

    #[test]
    fn rejects_invalid_cid_font_tables() {
        let document = Document::new();
        let odd_map = cid_font(vec![(
            "CIDToGIDMap",
            Value::Stream(Dict::new(), StreamData::Owned(vec![0])),
        )]);
        assert!(document.cid_font_type2(&odd_map).is_err());

        let oversized_map = cid_font(vec![(
            "CIDToGIDMap",
            Value::Stream(
                Dict::new(),
                StreamData::Owned(vec![0; (usize::from(u16::MAX) + 2) * 2]),
            ),
        )]);
        assert!(document.cid_font_type2(&oversized_map).is_err());

        let invalid_map = cid_font(vec![("CIDToGIDMap", Value::Name("Custom".into()))]);
        assert!(document.cid_font_type2(&invalid_map).is_err());

        let truncated_widths = cid_font(vec![(
            "W",
            Value::Array(Array::new().push(Value::Integer(4))),
        )]);
        assert!(document.cid_font_type2(&truncated_widths).is_err());

        let duplicate_widths = cid_font(vec![(
            "W",
            Value::Array(
                Array::new()
                    .push(Value::Integer(4))
                    .push(Value::Array(
                        Array::new()
                            .push(Value::Integer(500))
                            .push(Value::Integer(501)),
                    ))
                    .push(Value::Integer(5))
                    .push(Value::Integer(6))
                    .push(Value::Integer(600)),
            ),
        )]);
        assert!(document.cid_font_type2(&duplicate_widths).is_err());

        let missing_info = Value::Dict(
            Dict::new()
                .insert("Subtype", Value::Name("CIDFontType2".into()))
                .insert("BaseFont", Value::Name("MissingInfo".into()))
                .insert(
                    "FontDescriptor",
                    Value::Dict(Dict::new().insert("Type", Value::Name("FontDescriptor".into()))),
                ),
        );
        assert!(document.cid_font_type2(&missing_info).is_err());

        let missing_descriptor = Value::Dict(
            Dict::new()
                .insert("Subtype", Value::Name("CIDFontType2".into()))
                .insert("BaseFont", Value::Name("MissingDescriptor".into()))
                .insert("CIDSystemInfo", cid_system_info()),
        );
        assert!(document.cid_font_type2(&missing_descriptor).is_err());
    }

    #[test]
    fn exposes_descriptor_metrics_and_cid_set_bits() {
        let document = Document::new();
        let font = cid_font_with_descriptor(
            Vec::new(),
            vec![
                ("FontName", Value::Name("CIDFont".into())),
                ("FontFamily", Value::String("Example Family".into())),
                ("FontStretch", Value::Name("Condensed".into())),
                ("FontWeight", Value::Integer(700)),
                ("Flags", Value::Integer(32)),
                (
                    "FontBBox",
                    Value::Array(
                        Array::new()
                            .push(Value::Integer(-10))
                            .push(Value::Integer(-200))
                            .push(Value::Integer(1000))
                            .push(Value::Integer(900)),
                    ),
                ),
                ("ItalicAngle", Value::Integer(0)),
                ("Ascent", Value::Integer(800)),
                ("Descent", Value::Integer(-200)),
                ("Leading", Value::Integer(100)),
                ("CapHeight", Value::Integer(700)),
                ("XHeight", Value::Integer(500)),
                ("StemV", Value::Integer(80)),
                ("StemH", Value::Integer(60)),
                ("AvgWidth", Value::Integer(600)),
                ("MaxWidth", Value::Integer(1000)),
                ("MissingWidth", Value::Integer(450)),
                ("CharSet", Value::String("/A/B".into())),
                (
                    "CIDSet",
                    Value::Stream(
                        Dict::new().insert("Filter", Value::Name("ASCIIHexDecode".into())),
                        StreamData::Owned(b"AF01>".to_vec()),
                    ),
                ),
            ],
        );

        let font = document.cid_font_type2(&font).unwrap().unwrap();
        let descriptor = font.descriptor();
        assert_eq!(descriptor.font_name.as_deref(), Some("CIDFont"));
        assert_eq!(descriptor.font_family.as_deref(), Some("Example Family"));
        assert_eq!(descriptor.font_stretch.as_deref(), Some("Condensed"));
        assert_eq!(descriptor.font_weight, Some(700.0));
        assert_eq!(descriptor.font_bbox, Some([-10.0, -200.0, 1000.0, 900.0]));
        assert_eq!(descriptor.italic_angle, Some(0.0));
        assert_eq!(descriptor.ascent, Some(800.0));
        assert_eq!(descriptor.descent, Some(-200.0));
        assert_eq!(descriptor.leading, Some(100.0));
        assert_eq!(descriptor.cap_height, Some(700.0));
        assert_eq!(descriptor.x_height, Some(500.0));
        assert_eq!(descriptor.stem_v, Some(80.0));
        assert_eq!(descriptor.stem_h, Some(60.0));
        assert_eq!(descriptor.avg_width, Some(600.0));
        assert_eq!(descriptor.max_width, Some(1000.0));
        assert_eq!(descriptor.missing_width, Some(450.0));
        assert_eq!(descriptor.char_set.as_deref(), Some("/A/B"));
        assert!(descriptor.is_nonsymbolic());
        assert!(!descriptor.is_serif());
        assert_eq!(descriptor.cid_is_present(0), Some(true));
        assert_eq!(descriptor.cid_is_present(1), Some(false));
        assert_eq!(descriptor.cid_is_present(2), Some(true));
        assert_eq!(descriptor.cid_is_present(15), Some(true));
        assert_eq!(descriptor.cid_is_present(16), None);
    }

    #[test]
    fn rejects_non_stream_cid_set() {
        let document = Document::new();
        let font =
            cid_font_with_descriptor(Vec::new(), vec![("CIDSet", Value::Name("Invalid".into()))]);
        assert!(document.cid_font_type2(&font).is_err());
    }

    #[test]
    fn ignores_other_font_subtypes() {
        let document = Document::new();
        let value = Value::Dict(Dict::new().insert("Subtype", Value::Name("TrueType".into())));
        assert!(document.cid_font_type2(&value).unwrap().is_none());
    }

    #[test]
    fn parses_cid_system_info() {
        let document = Document::new();
        let font = cid_font(vec![(
            "CIDSystemInfo",
            Value::Dict(
                Dict::new()
                    .insert("Registry", Value::String("Adobe".into()))
                    .insert("Ordering", Value::String("GB1".into()))
                    .insert("Supplement", Value::Integer(5)),
            ),
        )]);

        let font = document.cid_font_type2(&font).unwrap().unwrap();
        let info = font.system_info();
        assert_eq!(info.registry, "Adobe");
        assert_eq!(info.ordering, "GB1");
        assert_eq!(info.supplement, 5);
    }

    #[test]
    fn rejects_missing_cid_system_info() {
        let document = Document::new();
        let dict = Dict::new()
            .insert("Subtype", Value::Name("CIDFontType2".into()))
            .insert("BaseFont", Value::Name("ExampleCIDFont".into()))
            .insert(
                "FontDescriptor",
                Value::Dict(Dict::new().insert("Type", Value::Name("FontDescriptor".into()))),
            );
        // Missing CIDSystemInfo
        let font = Value::Dict(dict);
        assert!(document.cid_font_type2(&font).is_err());
    }

    #[test]
    fn rejects_invalid_cid_system_info_fields() {
        let document = Document::new();

        // Missing Registry
        let font = cid_font(vec![(
            "CIDSystemInfo",
            Value::Dict(
                Dict::new()
                    .insert("Ordering", Value::String("GB1".into()))
                    .insert("Supplement", Value::Integer(0)),
            ),
        )]);
        assert!(document.cid_font_type2(&font).is_err());

        // Missing Ordering
        let font = cid_font(vec![(
            "CIDSystemInfo",
            Value::Dict(
                Dict::new()
                    .insert("Registry", Value::String("Adobe".into()))
                    .insert("Supplement", Value::Integer(0)),
            ),
        )]);
        assert!(document.cid_font_type2(&font).is_err());

        // Missing Supplement
        let font = cid_font(vec![(
            "CIDSystemInfo",
            Value::Dict(
                Dict::new()
                    .insert("Registry", Value::String("Adobe".into()))
                    .insert("Ordering", Value::String("GB1".into())),
            ),
        )]);
        assert!(document.cid_font_type2(&font).is_err());

        // Negative Supplement
        let font = cid_font(vec![(
            "CIDSystemInfo",
            Value::Dict(
                Dict::new()
                    .insert("Registry", Value::String("Adobe".into()))
                    .insert("Ordering", Value::String("GB1".into()))
                    .insert("Supplement", Value::Integer(-1)),
            ),
        )]);
        assert!(document.cid_font_type2(&font).is_err());

        // Non-string Registry
        let font = cid_font(vec![(
            "CIDSystemInfo",
            Value::Dict(
                Dict::new()
                    .insert("Registry", Value::Integer(123))
                    .insert("Ordering", Value::String("GB1".into()))
                    .insert("Supplement", Value::Integer(0)),
            ),
        )]);
        assert!(document.cid_font_type2(&font).is_err());
    }
}
