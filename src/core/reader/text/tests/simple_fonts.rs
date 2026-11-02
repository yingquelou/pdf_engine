//! Simple-font decoding: predefined encodings, `/Differences`, `/ToUnicode`
//! precedence, TrueType built-in fallbacks and the byte-text preference rules.

use crate::core::types::{Array, Dict, Document, StreamData, Value};

use super::support::{differences_type1_font, resources_with_font, winansi_type1_font};

fn truetype_font(encoding: Value) -> Value {
    Value::Dict(
        Dict::new()
            .insert("Type", Value::Name("Font".into()))
            .insert("Subtype", Value::Name("TrueType".into()))
            .insert("BaseFont", Value::Name("Arial".into()))
            .insert("Encoding", encoding),
    )
}

#[test]
fn extracts_type1_winansi_text() {
    let document = Document::new();
    let resources = resources_with_font("F1", winansi_type1_font());
    let content = b"BT /F1 12 Tf (Hello) Tj ET";
    let text = document.extract_text(content, &resources);
    assert_eq!(text, "Hello");
}

#[test]
fn simple_font_to_unicode_takes_precedence_over_encoding() {
    let document = Document::new();
    let font = match winansi_type1_font() {
        Value::Dict(dict) => Value::Dict(dict.insert(
            "ToUnicode",
            Value::Stream(
                Dict::new(),
                StreamData::Owned(b"1 beginbfchar <41> <4E2D> endbfchar".to_vec()),
            ),
        )),
        other => other,
    };
    let resources = resources_with_font("F1", font);
    let content = b"BT /F1 12 Tf (A) Tj ET";

    assert_eq!(
        document.extract_text(content, &resources),
        "中",
        "简单字体的 /ToUnicode 应优先于 /Encoding"
    );
}

#[test]
fn malformed_simple_font_to_unicode_falls_back_to_encoding() {
    let document = Document::new();
    let font = match winansi_type1_font() {
        Value::Dict(dict) => Value::Dict(dict.insert(
            "ToUnicode",
            Value::Stream(
                Dict::new(),
                StreamData::Owned(b"1 beginbfchar <41> <D800> endbfchar".to_vec()),
            ),
        )),
        other => other,
    };
    let resources = resources_with_font("F1", font);

    assert_eq!(
        document.extract_text(b"BT /F1 12 Tf (A) Tj ET", &resources),
        "A"
    );
}

#[test]
fn content_stream_name_escapes_select_font_resources() {
    let document = Document::new();
    let resources = resources_with_font("F1", differences_type1_font());

    assert_eq!(
        document.extract_text(b"BT /F#31 12 Tf (A) Tj ET", &resources),
        "©",
        "/F#31 应解码为资源名 F1"
    );
}

#[test]
fn extracts_truetype_winansi_text() {
    let document = Document::new();
    let resources = resources_with_font("F1", truetype_font(Value::Name("WinAnsiEncoding".into())));
    let content = b"BT /F1 12 Tf <93419442> Tj ET";
    let text = document.extract_text(content, &resources);
    assert_eq!(text, "“A”B");
}

#[test]
fn extracts_truetype_differences_text() {
    let differences = Array::new()
        .push(Value::Integer(65))
        .push(Value::Name("copyright".into()));
    let encoding = Value::Dict(Dict::new().insert("Differences", Value::Array(differences)));
    let document = Document::new();
    let resources = resources_with_font("F1", truetype_font(encoding));
    let content = b"BT /F1 12 Tf (A) Tj ET";
    let text = document.extract_text(content, &resources);
    assert_eq!(text, "©");
}

#[test]
fn missing_font_falls_back_to_latin1() {
    let document = Document::new();
    let resources = Dict::new();
    let content = b"BT /F1 12 Tf (ABC) Tj ET";
    let text = document.extract_text(content, &resources);
    assert_eq!(text, "ABC");
}

#[test]
fn truetype_without_encoding_falls_back_when_program_is_missing() {
    let font = Value::Dict(
        Dict::new()
            .insert("Type", Value::Name("Font".into()))
            .insert("Subtype", Value::Name("TrueType".into()))
            .insert("BaseFont", Value::Name("Arial".into())),
    );
    let document = Document::new();
    let resources = resources_with_font("F1", font);
    let content = b"BT /F1 12 Tf (A) Tj ET";
    let text = document.extract_text(content, &resources);
    assert_eq!(text, "A");
}

#[test]
fn truetype_without_encoding_falls_back_when_program_is_invalid() {
    let font = Value::Dict(
        Dict::new()
            .insert("Type", Value::Name("Font".into()))
            .insert("Subtype", Value::Name("TrueType".into()))
            .insert("BaseFont", Value::Name("Arial".into()))
            .insert(
                "FontDescriptor",
                Value::Dict(
                    Dict::new()
                        .insert("Type", Value::Name("FontDescriptor".into()))
                        .insert("FontName", Value::Name("Arial".into()))
                        .insert(
                            "FontFile2",
                            Value::Stream(Dict::new(), StreamData::Owned(b"not an sfnt".to_vec())),
                        ),
                ),
            ),
    );
    let document = Document::new();
    let resources = resources_with_font("F1", font);
    let content = b"BT /F1 12 Tf (A) Tj ET";
    let text = document.extract_text(content, &resources);
    assert_eq!(text, "A");
}

mod simple_byte_text_preference {
    use super::{Document, Value, truetype_font};
    use crate::core::reader::font::SimpleFontEncoding;
    use crate::core::reader::text::state::{is_private_use, simple_byte_text};

    fn encoding(name: &str) -> SimpleFontEncoding {
        let font = truetype_font(Value::Name(name.into()));
        Document::new()
            .simple_font_encoding(&font)
            .unwrap()
            .unwrap()
    }

    #[test]
    fn non_private_use_builtin_wins_over_encoding() {
        let win_ansi = encoding("WinAnsiEncoding");
        assert_eq!(
            simple_byte_text(0x41, Some('A'), Some(&win_ansi)),
            Some("A".to_string())
        );
    }

    #[test]
    fn private_use_builtin_loses_to_encoding() {
        // Symbol cmaps map code 0x41 to U+F041; SymbolEncoding resolves the
        // same byte to the Greek capital alpha, which is more useful text.
        let symbol = encoding("SymbolEncoding");
        assert_eq!(
            simple_byte_text(0x41, Some('\u{F041}'), Some(&symbol)),
            Some("\u{0391}".to_string())
        );
    }

    #[test]
    fn private_use_builtin_used_when_encoding_has_no_mapping() {
        assert_eq!(
            simple_byte_text(0x41, Some('\u{F041}'), None),
            Some("\u{F041}".to_string())
        );
        assert!(is_private_use('\u{F041}'));
        assert!(!is_private_use('\u{0391}'));
    }
}
