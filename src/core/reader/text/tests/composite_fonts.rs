//! Type 0 composite-font decoding through Identity-H and `/ToUnicode`.

use crate::core::types::{Array, Dict, Document, StreamData, Value};

use super::support::resources_with_font;

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
