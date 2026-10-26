use crate::core::types::{
    Array, Dict, Document, Object, StartXref, StreamData, Trailer, Value, Xref,
};

use super::{MAX_CONTENT_BYTES, MAX_CONTENT_STREAMS, MAX_PAGE_TREE_DEPTH, TextExtractionError};

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

fn differences_type1_font() -> Value {
    let differences = Array::new()
        .push(Value::Integer(65))
        .push(Value::Name("copyright".into()));
    Value::Dict(
        Dict::new()
            .insert("Type", Value::Name("Font".into()))
            .insert("Subtype", Value::Name("Type1".into()))
            .insert("BaseFont", Value::Name("Helvetica".into()))
            .insert(
                "Encoding",
                Value::Dict(Dict::new().insert("Differences", Value::Array(differences))),
            ),
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
fn extracts_type0_with_to_unicode() {
    let document = Document::new();
    let resources = resources_with_font("F2", type0_with_to_unicode());
    let content = b"BT /F2 12 Tf <00480065006C006C006F> Tj ET";
    let text = document.extract_text(content, &resources);
    assert_eq!(text, "Hello");
}

#[test]
fn type0_to_unicode_with_unknown_usecmap_keeps_local_mappings() {
    let document = Document::new();
    let font = match type0_with_to_unicode() {
        Value::Dict(dict) => Value::Dict(
            dict.insert(
                "ToUnicode",
                Value::Stream(
                    Dict::new(),
                    StreamData::Owned(
                        b"/Unknown usecmap \
                      4 beginbfchar <0048> <0048> <0065> <0065> \
                      <006C> <006C> <006F> <006F> endbfchar"
                            .to_vec(),
                    ),
                ),
            ),
        ),
        other => other,
    };
    let resources = resources_with_font("F2", font);
    let content = b"BT /F2 12 Tf <00480065006C006C006F> Tj ET";

    assert_eq!(
        document.extract_text(content, &resources),
        "Hello",
        "无法解析的 usecmap 基名不应丢弃本地映射"
    );
}

#[test]
fn malformed_type0_to_unicode_does_not_disable_the_font() {
    let document = Document::new();
    let font = match type0_with_to_unicode() {
        Value::Dict(dict) => Value::Dict(dict.insert(
            "ToUnicode",
            Value::Stream(
                Dict::new(),
                StreamData::Owned(b"1 beginbfchar <0041> <D800> endbfchar".to_vec()),
            ),
        )),
        other => other,
    };
    let resources = resources_with_font("F2", font);

    assert_eq!(
        document.extract_text(b"BT /F2 12 Tf <0041> Tj ET", &resources),
        "\0A",
        "ToUnicode 损坏时应回落为字符码字节而不是丢弃字体"
    );
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

// ── Page tree / inheritance hardening fixtures ───────────

fn document_from_objects(objects: Vec<Object>) -> Document {
    let mut trailer = Trailer::new();
    trailer.dict = Dict::new().insert("Root", Value::Ref(1, 0));
    Document::from_objects(objects, Xref::new(), trailer, StartXref(0))
}

fn catalog_with_pages(pages: Value) -> Value {
    Value::Dict(
        Dict::new()
            .insert("Type", Value::Name("Catalog".into()))
            .insert("Pages", pages),
    )
}

fn pages_node(kids: Array) -> Value {
    let count = kids.len() as i64;
    Value::Dict(
        Dict::new()
            .insert("Type", Value::Name("Pages".into()))
            .insert("Kids", Value::Array(kids))
            .insert("Count", Value::Integer(count)),
    )
}

fn page_with(contents: Value, resources: Value) -> Value {
    Value::Dict(
        Dict::new()
            .insert("Type", Value::Name("Page".into()))
            .insert("Parent", Value::Ref(2, 0))
            .insert("Contents", contents)
            .insert("Resources", resources),
    )
}

fn contents_stream(content: &[u8]) -> Value {
    Value::Stream(Dict::new(), StreamData::Owned(content.to_vec()))
}

/// Catalog object 1 → `/Pages` object 2 → single page object 3, plus any
/// additional objects supplied by the test.
fn single_page_document(contents: Value, resources: Value, extra: Vec<Object>) -> Document {
    let mut objects = vec![
        Object::new(1, 0, catalog_with_pages(Value::Ref(2, 0))),
        Object::new(2, 0, pages_node(Array::new().push(Value::Ref(3, 0)))),
        Object::new(3, 0, page_with(contents, resources)),
    ];
    objects.extend(extra);
    document_from_objects(objects)
}

fn resources_with_font_and_xobjects(font: Value, xobjects: &[(&str, i64)]) -> Dict {
    let mut map = Dict::new();
    for (name, num) in xobjects {
        map = map.insert(*name, Value::Ref(*num, 0));
    }
    resources_with_font("F1", font).insert("XObject", Value::Dict(map))
}

fn form_xobject(content: &[u8], resources: Option<Dict>) -> Value {
    let mut dict = Dict::new()
        .insert("Type", Value::Name("XObject".into()))
        .insert("Subtype", Value::Name("Form".into()))
        .insert("FormType", Value::Integer(1))
        .insert(
            "BBox",
            Value::Array(
                Array::new()
                    .push(Value::Integer(0))
                    .push(Value::Integer(0))
                    .push(Value::Integer(100))
                    .push(Value::Integer(100)),
            ),
        );
    if let Some(resources) = resources {
        dict = dict.insert("Resources", Value::Dict(resources));
    }
    Value::Stream(dict, StreamData::Owned(content.to_vec()))
}

fn image_xobject() -> Value {
    Value::Stream(
        Dict::new()
            .insert("Type", Value::Name("XObject".into()))
            .insert("Subtype", Value::Name("Image".into()))
            .insert("Width", Value::Integer(1))
            .insert("Height", Value::Integer(1))
            .insert("ColorSpace", Value::Name("DeviceGray".into()))
            .insert("BitsPerComponent", Value::Integer(8)),
        StreamData::Owned(vec![0]),
    )
}

#[test]
fn page_tree_cycle_does_not_hang_and_finds_reachable_page() {
    let objects = vec![
        Object::new(1, 0, catalog_with_pages(Value::Ref(2, 0))),
        Object::new(
            2,
            0,
            pages_node(Array::new().push(Value::Ref(2, 0)).push(Value::Ref(3, 0))),
        ),
        Object::new(
            3,
            0,
            page_with(
                contents_stream(b"BT /F1 12 Tf (Reachable) Tj ET"),
                Value::Dict(resources_with_font("F1", winansi_type1_font())),
            ),
        ),
    ];
    let document = document_from_objects(objects);

    assert_eq!(document.extract_page_text(0).as_deref(), Some("Reachable"));
    assert!(document.extract_page_text(1).is_none());
    assert_eq!(
        document.try_extract_page_text(1),
        Err(TextExtractionError::PageNotFound)
    );
}

#[test]
fn page_tree_deeper_than_limit_returns_not_found() {
    let layers = MAX_PAGE_TREE_DEPTH + 2;
    let mut objects = vec![Object::new(1, 0, catalog_with_pages(Value::Ref(2, 0)))];
    for layer in 0..layers {
        let num = 2 + layer as i64;
        objects.push(Object::new(
            num,
            0,
            pages_node(Array::new().push(Value::Ref(num + 1, 0))),
        ));
    }
    let page_num = 2 + layers as i64;
    objects.push(Object::new(
        page_num,
        0,
        page_with(
            contents_stream(b"BT /F1 12 Tf (TooDeep) Tj ET"),
            Value::Dict(resources_with_font("F1", winansi_type1_font())),
        ),
    ));
    let document = document_from_objects(objects);

    assert!(document.extract_page_text(0).is_none());
    assert_eq!(
        document.try_extract_page_text(0),
        Err(TextExtractionError::PageNotFound)
    );
}

#[test]
fn parent_chain_cycle_terminates_and_own_resources_still_apply() {
    let font_resources = Value::Dict(resources_with_font("F1", winansi_type1_font()));
    let own_page = Value::Dict(
        Dict::new()
            .insert("Type", Value::Name("Page".into()))
            .insert("Parent", Value::Ref(5, 0))
            .insert("Contents", contents_stream(b"BT /F1 12 Tf (Own) Tj ET"))
            .insert("Resources", font_resources),
    );
    // Page 4 has no /Resources; its /Parent points at object 5 whose /Parent
    // points back at page 4, forming a cycle.
    let page_without_resources = Value::Dict(
        Dict::new()
            .insert("Type", Value::Name("Page".into()))
            .insert("Parent", Value::Ref(5, 0))
            .insert("Contents", contents_stream(b"BT /F1 12 Tf (Missing) Tj ET")),
    );
    let cyclic_parent = Value::Dict(
        Dict::new()
            .insert("Type", Value::Name("Pages".into()))
            .insert("Parent", Value::Ref(4, 0)),
    );
    let objects = vec![
        Object::new(1, 0, catalog_with_pages(Value::Ref(2, 0))),
        Object::new(
            2,
            0,
            pages_node(Array::new().push(Value::Ref(3, 0)).push(Value::Ref(4, 0))),
        ),
        Object::new(3, 0, own_page),
        Object::new(4, 0, page_without_resources),
        Object::new(5, 0, cyclic_parent),
    ];
    let document = document_from_objects(objects);

    assert_eq!(document.extract_page_text(0).as_deref(), Some("Own"));
    assert!(document.extract_page_text(1).is_none());
    assert_eq!(
        document.try_extract_page_text(1),
        Err(TextExtractionError::MalformedContent)
    );
}

#[test]
fn page_contents_stream_count_over_limit_returns_content_too_large() {
    let mut contents = Array::new();
    for _ in 0..=MAX_CONTENT_STREAMS {
        contents = contents.push(contents_stream(b""));
    }
    let document =
        single_page_document(Value::Array(contents), Value::Dict(Dict::new()), Vec::new());

    assert_eq!(
        document.try_extract_page_text(0),
        Err(TextExtractionError::ContentTooLarge)
    );
    assert!(document.extract_page_text(0).is_none());
}

#[test]
fn page_contents_byte_size_over_limit_returns_content_too_large() {
    let oversized = Value::Stream(
        Dict::new(),
        StreamData::Owned(vec![b' '; MAX_CONTENT_BYTES + 1]),
    );
    let document = single_page_document(oversized, Value::Dict(Dict::new()), Vec::new());

    assert_eq!(
        document.try_extract_page_text(0),
        Err(TextExtractionError::ContentTooLarge)
    );
    assert!(document.extract_page_text(0).is_none());
}

#[test]
fn form_stream_budget_is_shared_with_page_content_streams() {
    let mut contents = Array::new();
    for _ in 0..MAX_CONTENT_STREAMS - 1 {
        contents = contents.push(contents_stream(b""));
    }
    contents = contents.push(contents_stream(b"/Fm Do"));
    let resources = resources_with_font_and_xobjects(winansi_type1_font(), &[("Fm", 10)]);
    let form = form_xobject(b"BT /F1 12 Tf (Form) Tj ET", None);
    let document = single_page_document(
        Value::Array(contents),
        Value::Dict(resources),
        vec![Object::new(10, 0, form)],
    );

    assert_eq!(
        document.try_extract_page_text(0),
        Err(TextExtractionError::ContentTooLarge)
    );
    assert!(document.extract_page_text(0).is_none());
}

#[test]
fn form_xobject_with_own_resources_is_extracted() {
    let resources = resources_with_font_and_xobjects(winansi_type1_font(), &[("Fm", 10)]);
    let form = form_xobject(
        b"BT /F2 12 Tf (Form) Tj ET",
        Some(resources_with_font("F2", winansi_type1_font())),
    );
    let document = single_page_document(
        contents_stream(b"BT /F1 12 Tf (Page) Tj ET /Fm Do"),
        Value::Dict(resources),
        vec![Object::new(10, 0, form)],
    );

    assert_eq!(document.extract_page_text(0).as_deref(), Some("PageForm"));
}

#[test]
fn form_without_resources_inherits_caller_resources() {
    let resources = resources_with_font_and_xobjects(winansi_type1_font(), &[("Fm", 10)]);
    let form = form_xobject(b"BT /F1 12 Tf (Inherited) Tj ET", None);
    let document = single_page_document(
        contents_stream(b"/Fm Do"),
        Value::Dict(resources),
        vec![Object::new(10, 0, form)],
    );

    assert_eq!(document.extract_page_text(0).as_deref(), Some("Inherited"));
}

#[test]
fn nested_form_xobject_text_is_extracted_in_order() {
    let resources = resources_with_font_and_xobjects(winansi_type1_font(), &[("A", 10), ("B", 11)]);
    let form_a = form_xobject(b"BT /F1 12 Tf (A) Tj ET /B Do BT /F1 12 Tf (C) Tj ET", None);
    let form_b = form_xobject(b"BT /F1 12 Tf (B) Tj ET", None);
    let document = single_page_document(
        contents_stream(b"/A Do"),
        Value::Dict(resources),
        vec![Object::new(10, 0, form_a), Object::new(11, 0, form_b)],
    );

    assert_eq!(document.extract_page_text(0).as_deref(), Some("ABC"));
}

#[test]
fn cyclic_form_xobject_references_terminate() {
    let resources = resources_with_font_and_xobjects(winansi_type1_font(), &[("A", 10), ("B", 11)]);
    let form_a = form_xobject(b"BT /F1 12 Tf (A) Tj ET /B Do", None);
    let form_b = form_xobject(b"BT /F1 12 Tf (B) Tj ET /A Do", None);
    let document = single_page_document(
        contents_stream(b"/A Do"),
        Value::Dict(resources),
        vec![Object::new(10, 0, form_a), Object::new(11, 0, form_b)],
    );

    assert_eq!(document.extract_page_text(0).as_deref(), Some("AB"));
}

#[test]
fn repeated_form_xobject_is_extracted_each_time() {
    let resources = resources_with_font_and_xobjects(winansi_type1_font(), &[("Fm", 10)]);
    let form = form_xobject(b"BT /F1 12 Tf (X) Tj ET", None);
    let document = single_page_document(
        contents_stream(b"/Fm Do /Fm Do"),
        Value::Dict(resources),
        vec![Object::new(10, 0, form)],
    );

    assert_eq!(document.extract_page_text(0).as_deref(), Some("XX"));
}

#[test]
fn form_font_selection_does_not_leak_into_caller() {
    let page_resources = resources_with_font_and_xobjects(winansi_type1_font(), &[("Fm", 10)]);
    let form = form_xobject(
        b"BT /F1 12 Tf (A) Tj ET",
        Some(resources_with_font("F1", differences_type1_font())),
    );
    let document = single_page_document(
        contents_stream(b"BT /F1 12 Tf (A) Tj ET /Fm Do BT (A) Tj ET"),
        Value::Dict(page_resources),
        vec![Object::new(10, 0, form)],
    );

    assert_eq!(document.extract_page_text(0).as_deref(), Some("A©A"));
}

#[test]
fn q_and_q_restore_selected_font() {
    let resources = Dict::new().insert(
        "Font",
        Value::Dict(
            Dict::new()
                .insert("F1", winansi_type1_font())
                .insert("F2", differences_type1_font()),
        ),
    );
    let document = single_page_document(
        contents_stream(b"BT /F1 12 Tf (A) Tj ET q BT /F2 12 Tf (A) Tj ET Q BT (A) Tj ET"),
        Value::Dict(resources),
        Vec::new(),
    );

    assert_eq!(document.extract_page_text(0).as_deref(), Some("A©A"));
}

#[test]
fn image_xobject_is_skipped_without_losing_text() {
    let resources = resources_with_font_and_xobjects(winansi_type1_font(), &[("Im1", 10)]);
    let document = single_page_document(
        contents_stream(b"BT /F1 12 Tf (A) Tj ET /Im1 Do /Missing Do BT /F1 12 Tf (B) Tj ET"),
        Value::Dict(resources),
        vec![Object::new(10, 0, image_xobject())],
    );

    assert_eq!(document.extract_page_text(0).as_deref(), Some("AB"));
}

#[test]
fn inline_image_data_is_skipped_without_losing_following_text() {
    let document = Document::new();
    let resources = resources_with_font("F1", winansi_type1_font());
    // The image data contains an unmatched "(" that would otherwise consume
    // the rest of the stream as a literal string.
    let content = b"BT /F1 12 Tf (A) Tj ET \
                    BI /W 1 /H 1 /BPC 8 /CS /G ID ( EI \
                    BT /F1 12 Tf (B) Tj ET";

    assert_eq!(document.extract_text(content, &resources), "AB");
}
