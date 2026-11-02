use super::*;

use super::super::super::PredefinedEncoding;
use super::super::Type3Font;

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
        PredefinedEncoding::Standard
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
