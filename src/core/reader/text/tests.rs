use crate::core::types::{
    Array, Dict, Document, Object, StartXref, StreamData, Trailer, Value, Xref,
};

fn winansi_type1_font() -> Value {
    Value::Dict(
        Dict::new()
            .insert("Type", Value::Name("Font".into()))
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("Helvetica".into()))
            .insert("Encoding", Value::Name("WinAnsiEncoding".into())),
    )
}

fn truetype_font(encoding: Value) -> Value {
    Value::Dict(
        Dict::new()
            .insert("Type", Value::Name("Font".into()))
            .insert("Subtype", Value::Name("TrueType".into()))
            .insert("BaseFont", Value::Name("Arial".into()))
            .insert("Encoding", encoding),
    )
}

fn hello_to_unicode() -> Value {
    Value::Stream(
        Dict::new(),
        StreamData::Owned(
            b"1 begincodespacerange <0000> <FFFF> endcodespacerange \
              4 beginbfchar \
                <0048> <0048> \
                <0065> <0065> \
                <006C> <006C> \
                <006F> <006F> \
              endbfchar"
                .to_vec(),
        ),
    )
}

fn cid_system_info() -> Value {
    Value::Dict(
        Dict::new()
            .insert("Registry", Value::String("Adobe".into()))
            .insert("Ordering", Value::String("Identity".into()))
            .insert("Supplement", Value::Integer(0)),
    )
}

fn type0_with_descendant(descendant: Value) -> Value {
    Value::Dict(
        Dict::new()
            .insert("Type", Value::Name("Font".into()))
            .insert("Subtype", Value::Name("Type0".into()))
            .insert("BaseFont", Value::Name("ExampleComposite".into()))
            .insert("Encoding", Value::Name("Identity-H".into()))
            .insert("ToUnicode", hello_to_unicode())
            .insert(
                "DescendantFonts",
                Value::Array(Array::new().push(descendant)),
            ),
    )
}

fn type0_with_to_unicode() -> Value {
    let cid_descendant = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("CIDFontType2".into()))
            .insert("BaseFont", Value::Name("ExampleCID".into()))
            .insert("CIDSystemInfo", cid_system_info())
            .insert(
                "FontDescriptor",
                Value::Dict(Dict::new().insert("Type", Value::Name("FontDescriptor".into()))),
            ),
    );
    type0_with_descendant(cid_descendant)
}

fn type0_with_cid_type0_descendant() -> Value {
    let cid_descendant = Value::Dict(
        Dict::new()
            .insert("Subtype", Value::Name("CIDFontType0".into()))
            .insert("BaseFont", Value::Name("ExampleCIDType0".into()))
            .insert("CIDSystemInfo", cid_system_info())
            .insert(
                "FontDescriptor",
                Value::Dict(
                    Dict::new()
                        .insert("Type", Value::Name("FontDescriptor".into()))
                        .insert(
                            "FontFile3",
                            Value::Stream(
                                Dict::new().insert("Subtype", Value::Name("CIDFontType0C".into())),
                                StreamData::Owned(b"CFF ".to_vec()),
                            ),
                        ),
                ),
            ),
    );
    type0_with_descendant(cid_descendant)
}

fn resources_with_font(name: &str, font: Value) -> Dict {
    Dict::new().insert("Font", Value::Dict(Dict::new().insert(name, font)))
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
fn extracts_type0_with_to_unicode() {
    let document = Document::new();
    let resources = resources_with_font("F2", type0_with_to_unicode());
    let content = b"BT /F2 12 Tf <00480065006C006C006F> Tj ET";
    let text = document.extract_text(content, &resources);
    assert_eq!(text, "Hello");
}

#[test]
fn extracts_type0_with_cid_font_type0_descendant() {
    let document = Document::new();
    let resources = resources_with_font("F3", type0_with_cid_type0_descendant());
    let content = b"BT /F3 12 Tf <00480065006C006C006F> Tj ET";
    let text = document.extract_text(content, &resources);
    assert_eq!(text, "Hello");
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
fn tj_array_with_positioning() {
    let document = Document::new();
    let resources = resources_with_font("F1", winansi_type1_font());
    let content = b"BT /F1 12 Tf [(He) -200 (llo)] TJ ET";
    let text = document.extract_text(content, &resources);
    assert_eq!(text, "He llo");
}

#[test]
fn multiple_text_blocks() {
    let document = Document::new();
    let resources = resources_with_font("F1", winansi_type1_font());
    let content = b"BT /F1 12 Tf (First) Tj ET BT /F1 12 Tf (Second) Tj ET";
    let text = document.extract_text(content, &resources);
    assert_eq!(text, "FirstSecond");
}

#[test]
fn td_inserts_newline() {
    let document = Document::new();
    let resources = resources_with_font("F1", winansi_type1_font());
    let content = b"BT /F1 12 Tf (Line1) Tj 0 -14 Td (Line2) Tj ET";
    let text = document.extract_text(content, &resources);
    assert_eq!(text, "Line1\nLine2");
}

#[test]
fn hex_string_decoded() {
    let document = Document::new();
    let resources = resources_with_font("F1", winansi_type1_font());
    let content = b"BT /F1 12 Tf <48656C6C6F> Tj ET";
    let text = document.extract_text(content, &resources);
    assert_eq!(text, "Hello");
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
fn tick_operator_inserts_newline() {
    let document = Document::new();
    let resources = resources_with_font("F1", winansi_type1_font());
    let content = b"BT /F1 12 Tf (A) Tj (B) ' ET";
    let text = document.extract_text(content, &resources);
    assert_eq!(text, "A\nB");
}

#[test]
fn extract_page_text_from_constructed_document() {
    let font = winansi_type1_font();
    let resources = Value::Dict(resources_with_font("F1", font));
    let content = Value::Stream(
        Dict::new(),
        StreamData::Owned(b"BT /F1 12 Tf (Page1) Tj ET".to_vec()),
    );
    let page_dict = Value::Dict(
        Dict::new()
            .insert("Type", Value::Name("Page".into()))
            .insert("Contents", content)
            .insert("Resources", resources),
    );
    let pages_dict = Value::Dict(
        Dict::new()
            .insert("Type", Value::Name("Pages".into()))
            .insert("Kids", Value::Array(Array::new().push(Value::Ref(2, 0))))
            .insert("Count", Value::Integer(1)),
    );
    let catalog = Value::Dict(
        Dict::new()
            .insert("Type", Value::Name("Catalog".into()))
            .insert("Pages", pages_dict),
    );

    let mut trailer = Trailer::new();
    trailer.dict = Dict::new().insert("Root", Value::Ref(1, 0));

    let document = Document::from_objects(
        vec![Object::new(1, 0, catalog), Object::new(2, 0, page_dict)],
        Xref::new(),
        trailer,
        StartXref(0),
    );

    let text = document.extract_page_text(0).unwrap();
    assert_eq!(text, "Page1");
}

#[test]
fn tokenizer_handles_comments_and_whitespace() {
    let document = Document::new();
    let resources = resources_with_font("F1", winansi_type1_font());
    let content = b"% this is a comment\nBT /F1 12 Tf (OK) Tj ET";
    let text = document.extract_text(content, &resources);
    assert_eq!(text, "OK");
}

#[test]
fn empty_content_stream_returns_empty() {
    let document = Document::new();
    let resources = Dict::new();
    let text = document.extract_text(b"", &resources);
    assert_eq!(text, "");
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
    use super::super::{is_private_use, simple_byte_text};
    use super::{Document, Value, truetype_font};
    use crate::core::reader::font::SimpleFontEncoding;

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
