//! Type 3 font dictionary parsing.
//!
//! A Type 3 font describes its glyphs as content streams (the `/CharProcs`
//! procedures) instead of an embedded font program. This module resolves the
//! dictionary metadata needed by readers and text extraction: font matrix,
//! bounding box, character widths, `/Encoding`, and the set of glyph
//! procedure names. Rendering the `d0`/`d1` operators and executing the glyph
//! procedures is out of scope here.

use crate::core::types::{Dict, Document, Value};

use super::{FontError, FontResult, SimpleFontEncoding, number};

/// Resolved metadata from a Type 3 font dictionary (PDF 32000-1 §9.6.5).
#[derive(Debug, Clone, PartialEq)]
pub struct Type3Font {
    /// Font matrix mapping glyph space to text space (`/FontMatrix`).
    pub font_matrix: [f64; 6],
    /// Glyph bounding box in glyph space (`/FontBBox`).
    pub font_bbox: [f64; 4],
    /// First character code covered by `/Widths`.
    pub first_char: u8,
    /// Raw glyph-space widths from `/Widths`, indexed from `first_char`.
    ///
    /// These are glyph-space values: apply [`Self::font_matrix`] to convert
    /// them to text space. Rendered width also depends on the `d0`/`d1`
    /// operators inside each character procedure, which are not interpreted.
    pub widths: Vec<f64>,
    /// Parsed `/Encoding` (base encoding plus `/Differences`).
    pub encoding: SimpleFontEncoding,
    /// Glyph procedure names from `/CharProcs`, sorted ascending.
    pub char_procs: Vec<String>,
}

impl Type3Font {
    /// Return the raw glyph-space width for a one-byte character code.
    pub fn width_for(&self, code: u8) -> Option<f64> {
        let index = usize::from(code).checked_sub(usize::from(self.first_char))?;
        self.widths.get(index).copied()
    }
}

impl Document {
    /// Resolve a Type 3 font dictionary from direct or indirect values.
    ///
    /// Returns `Ok(None)` for non-Type-3 font dictionaries. Type 3 fonts have
    /// no `/BaseFont`; `/FontBBox`, `/FontMatrix`, `/CharProcs`, `/Encoding`,
    /// `/FirstChar`, `/LastChar`, and `/Widths` are required by this method,
    /// and `/Widths` must contain exactly `LastChar - FirstChar + 1` entries.
    ///
    /// `/Encoding` must be present (PDF 32000-1 §9.6.5.2). A name selects
    /// that predefined encoding; a dictionary selects `/BaseEncoding` when
    /// present and otherwise falls back to StandardEncoding, because Type 3
    /// has no `/BaseFont` from which to derive a default. This fallback is an
    /// approximation for encoding dictionaries that omit `/BaseEncoding`.
    ///
    /// The character procedures themselves, including their `d0`/`d1`
    /// operators, are not executed or interpreted.
    pub fn type3_font(&self, value: &Value) -> FontResult<Option<Type3Font>> {
        let value = self.resolve_value(value)?;
        let dict = value
            .as_dict()
            .ok_or_else(|| FontError::Invalid("font value is not a dictionary".to_string()))?;
        if dict.get("Subtype").and_then(Value::as_name) != Some("Type3") {
            return Ok(None);
        }

        let font_bbox = self.read_type3_numbers::<4>(dict, "FontBBox")?;
        let font_matrix = self.read_type3_numbers::<6>(dict, "FontMatrix")?;

        let (first_char, widths) = self.read_widths(dict)?;
        let first_char = first_char.ok_or_else(|| {
            FontError::Invalid(
                "Type 3 fonts require /FirstChar, /LastChar, and /Widths".to_string(),
            )
        })?;

        let char_procs_value = dict
            .get("CharProcs")
            .ok_or_else(|| FontError::Invalid("missing /CharProcs in Type 3 font".to_string()))?;
        let char_procs_value = self.resolve_value(char_procs_value)?;
        let char_procs = char_procs_value
            .as_dict()
            .ok_or_else(|| FontError::Invalid("/CharProcs is not a dictionary".to_string()))?;
        let char_procs = char_procs.0.keys().cloned().collect();

        let encoding = self
            .simple_font_encoding(&value)?
            .ok_or_else(|| FontError::Invalid("Type 3 fonts require /Encoding".to_string()))?;

        Ok(Some(Type3Font {
            font_matrix,
            font_bbox,
            first_char,
            widths,
            encoding,
            char_procs,
        }))
    }

    fn read_type3_numbers<const N: usize>(&self, dict: &Dict, key: &str) -> FontResult<[f64; N]> {
        let value = dict
            .get(key)
            .ok_or_else(|| FontError::Invalid(format!("missing /{key} in Type 3 font")))?;
        let value = self.resolve_value(value)?;
        let array = value
            .as_array()
            .ok_or_else(|| FontError::Invalid(format!("/{key} is not an array")))?;
        if array.len() != N {
            return Err(FontError::Invalid(format!(
                "/{key} contains {} entries; expected {N}",
                array.len()
            )));
        }
        let mut numbers = [0.0; N];
        for (slot, value) in numbers.iter_mut().zip(array.iter()) {
            *slot = number(value).ok_or_else(|| {
                FontError::Invalid(format!("/{key} contains a non-finite or non-numeric value"))
            })?;
        }
        Ok(numbers)
    }
}

#[cfg(test)]
mod tests {
    use crate::core::types::{Array, Dict, Document, Object, StreamData, Value};

    use super::Type3Font;

    fn char_proc() -> Value {
        Value::Stream(Dict::new(), StreamData::Owned(b"0 0 d0".to_vec()))
    }

    fn differences_encoding() -> Value {
        Value::Dict(
            Dict::new().insert(
                "Differences",
                Value::Array(
                    Array::new()
                        .push(Value::Integer(65))
                        .push(Value::Name("A".into()))
                        .push(Value::Integer(66))
                        .push(Value::Name("B".into())),
                ),
            ),
        )
    }

    fn type3_dict(encoding: Value) -> Dict {
        Dict::new()
            .insert("Type", Value::Name("Font".into()))
            .insert("Subtype", Value::Name("Type3".into()))
            .insert(
                "FontBBox",
                Value::Array(
                    Array::new()
                        .push(Value::Integer(0))
                        .push(Value::Integer(0))
                        .push(Value::Integer(1000))
                        .push(Value::Integer(1000)),
                ),
            )
            .insert(
                "FontMatrix",
                Value::Array(
                    Array::new()
                        .push(Value::Float(0.001))
                        .push(Value::Integer(0))
                        .push(Value::Integer(0))
                        .push(Value::Float(0.001))
                        .push(Value::Integer(0))
                        .push(Value::Integer(0)),
                ),
            )
            .insert(
                "CharProcs",
                Value::Dict(
                    Dict::new()
                        .insert("B", char_proc())
                        .insert("A", char_proc()),
                ),
            )
            .insert("FirstChar", Value::Integer(65))
            .insert("LastChar", Value::Integer(66))
            .insert(
                "Widths",
                Value::Array(
                    Array::new()
                        .push(Value::Integer(500))
                        .push(Value::Integer(600)),
                ),
            )
            .insert("Encoding", encoding)
    }

    fn type3_font(encoding: Value) -> Value {
        Value::Dict(type3_dict(encoding))
    }

    fn resources_with_font(font: Value) -> Dict {
        Dict::new().insert("Font", Value::Dict(Dict::new().insert("F1", font)))
    }

    fn assert_contains(error: super::FontError, needle: &str) {
        let message = error.to_string();
        assert!(
            message.contains(needle),
            "expected error containing {needle:?}, got {message:?}"
        );
    }

    #[test]
    fn type3_font_parses_font_matrix_bbox_widths_and_char_procs() {
        let document = Document::new();
        let font = type3_font(differences_encoding());
        let parsed = document.type3_font(&font).unwrap().unwrap();

        assert_eq!(parsed.font_matrix, [0.001, 0.0, 0.0, 0.001, 0.0, 0.0]);
        assert_eq!(parsed.font_bbox, [0.0, 0.0, 1000.0, 1000.0]);
        assert_eq!(parsed.first_char, 65);
        assert_eq!(parsed.widths, vec![500.0, 600.0]);
        assert_eq!(parsed.char_procs, vec!["A".to_string(), "B".to_string()]);
        assert_eq!(parsed.width_for(65), Some(500.0));
        assert_eq!(parsed.width_for(66), Some(600.0));
        assert_eq!(parsed.width_for(64), None);
        assert_eq!(parsed.width_for(67), None);

        // The same entries resolved through indirect objects.
        let document = Document::new();
        document.set_object(Object::new(
            10,
            0,
            Value::Array(
                Array::new()
                    .push(Value::Float(0.002))
                    .push(Value::Integer(0))
                    .push(Value::Integer(0))
                    .push(Value::Float(0.002))
                    .push(Value::Integer(0))
                    .push(Value::Integer(0)),
            ),
        ));
        document.set_object(Object::new(
            11,
            0,
            Value::Array(
                Array::new()
                    .push(Value::Integer(0))
                    .push(Value::Integer(0))
                    .push(Value::Integer(2000))
                    .push(Value::Integer(2000)),
            ),
        ));
        document.set_object(Object::new(
            12,
            0,
            Value::Dict(Dict::new().insert("A", char_proc())),
        ));
        document.set_object(Object::new(
            13,
            0,
            Value::Array(Array::new().push(Value::Integer(750))),
        ));
        let indirect = Value::Dict(
            Dict::new()
                .insert("Subtype", Value::Name("Type3".into()))
                .insert("FontBBox", Value::Ref(11, 0))
                .insert("FontMatrix", Value::Ref(10, 0))
                .insert("CharProcs", Value::Ref(12, 0))
                .insert("FirstChar", Value::Integer(65))
                .insert("LastChar", Value::Integer(65))
                .insert("Widths", Value::Ref(13, 0))
                .insert("Encoding", Value::Name("WinAnsiEncoding".into())),
        );
        let parsed = document.type3_font(&indirect).unwrap().unwrap();
        assert_eq!(parsed.font_matrix, [0.002, 0.0, 0.0, 0.002, 0.0, 0.0]);
        assert_eq!(parsed.font_bbox, [0.0, 0.0, 2000.0, 2000.0]);
        assert_eq!(parsed.widths, vec![750.0]);
        assert_eq!(parsed.char_procs, vec!["A".to_string()]);
        assert_eq!(parsed.width_for(65), Some(750.0));
    }

    #[test]
    fn type3_font_requires_char_procs() {
        let document = Document::new();

        let mut dict = type3_dict(differences_encoding());
        dict.0.remove("CharProcs");
        assert_contains(
            document.type3_font(&Value::Dict(dict)).unwrap_err(),
            "/CharProcs",
        );

        let mut dict = type3_dict(differences_encoding());
        dict.0.remove("FontMatrix");
        assert_contains(
            document.type3_font(&Value::Dict(dict)).unwrap_err(),
            "/FontMatrix",
        );

        let mut dict = type3_dict(differences_encoding());
        dict.0.remove("FontBBox");
        assert_contains(
            document.type3_font(&Value::Dict(dict)).unwrap_err(),
            "/FontBBox",
        );

        let mut dict = type3_dict(differences_encoding());
        dict.0.remove("Encoding");
        assert_contains(
            document.type3_font(&Value::Dict(dict)).unwrap_err(),
            "/Encoding",
        );

        // Non-Type-3 dictionaries are not Type 3 fonts.
        let other = Value::Dict(
            Dict::new()
                .insert("Subtype", Value::Name("Type1".into()))
                .insert("BaseFont", Value::Name("Helvetica".into())),
        );
        assert!(document.type3_font(&other).unwrap().is_none());
    }

    #[test]
    fn extracts_type3_differences_text() {
        let document = Document::new();
        let resources = resources_with_font(type3_font(differences_encoding()));
        let content = b"BT /F1 12 Tf (AB) Tj ET";
        assert_eq!(document.extract_text(content, &resources), "AB");
    }

    #[test]
    fn extracts_type3_predefined_encoding_text() {
        let document = Document::new();
        let resources = resources_with_font(type3_font(Value::Name("WinAnsiEncoding".into())));
        let content = b"BT /F1 12 Tf <E9> Tj ET";
        assert_eq!(document.extract_text(content, &resources), "é");
    }

    #[test]
    fn type3_encoding_without_base_encoding_uses_differences() {
        let document = Document::new();
        let encoding = Value::Dict(
            Dict::new().insert(
                "Differences",
                Value::Array(
                    Array::new()
                        .push(Value::Integer(65))
                        .push(Value::Name("eacute".into())),
                ),
            ),
        );
        let font = type3_font(encoding);
        let parsed: Type3Font = document.type3_font(&font).unwrap().unwrap();

        assert_eq!(
            parsed.encoding.base_encoding(),
            super::super::PredefinedEncoding::Standard
        );
        assert_eq!(
            parsed.encoding.unicode_sequence_for(65).as_deref(),
            Some("é")
        );
        // Codes not overridden by /Differences fall back to the StandardEncoding
        // approximation documented on `Document::type3_font`.
        assert_eq!(
            parsed.encoding.unicode_sequence_for(66).as_deref(),
            Some("B")
        );
    }
}
