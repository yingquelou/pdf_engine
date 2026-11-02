//! 多语言文本数组（ISO 32000-1 14.9.2.4）：配对与去重校验、匹配优先级、
//! `AlternateText` 的文本串/数组形态，以及 media clip `/Alt` 的解码。

use crate::core::reader::rendition::{MediaClip, MediaClipDataSource};
use crate::core::types::{Dict, Document, Value};

use super::super::*;
use super::support::{arr, assert_malformed, language, multi_language, name, text};

#[test]
fn multi_language_array_requires_pairs_and_unique_identifiers() {
    let parsed =
        MultiLanguageTextArray::parse(&multi_language(&[("", "default"), ("en", "English")]))
            .expect("pair array");
    assert_eq!(parsed.entries().len(), 2);
    assert_eq!(parsed.entries()[0].0.as_str(), "");
    assert_eq!(parsed.entries()[0].1, "default");
    assert_eq!(parsed.entries()[1].0.as_str(), "en");
    assert_eq!(parsed.default_text(), Some("default"));

    assert_malformed(
        MultiLanguageTextArray::parse(&arr(vec![text("en")])),
        "odd number",
    );
    assert_malformed(
        MultiLanguageTextArray::parse(&text("en-US")),
        "not an array",
    );
    assert_malformed(
        MultiLanguageTextArray::parse(&arr(vec![Value::Integer(1), text("x")])),
        "identifier 0",
    );
    assert_malformed(
        MultiLanguageTextArray::parse(&arr(vec![text("en"), Value::Integer(1)])),
        "text 0",
    );
    assert_malformed(
        MultiLanguageTextArray::parse(&multi_language(&[("en", "a"), ("EN", "b")])),
        "more than once",
    );
    assert_malformed(
        MultiLanguageTextArray::parse(&multi_language(&[("", "a"), ("", "b")])),
        "more than once",
    );

    // 4097 对：在解析标识符之前即触发条目上限。
    let mut values = Vec::with_capacity(4097 * 2);
    for _ in 0..=4096 {
        values.push(text(""));
        values.push(text("x"));
    }
    match MultiLanguageTextArray::parse(&arr(values)) {
        Err(LanguageError::LimitExceeded(_)) => {}
        other => panic!("expected entry limit, got {other:?}"),
    }
}

#[test]
fn multi_language_text_prefers_exact_then_prefix_then_default() {
    let array = MultiLanguageTextArray::parse(&multi_language(&[
        ("", "fallback"),
        ("en", "English"),
        ("en-US", "American"),
        ("en-GB", "British"),
        ("zh-Hant", "Traditional"),
    ]))
    .expect("multi-language array");

    assert_eq!(array.text_for(&language("en-US")), Some("American"));
    assert_eq!(array.text_for(&language("EN-us")), Some("American"));
    // 精确匹配优先于前缀匹配。
    assert_eq!(array.text_for(&language("en")), Some("English"));
    // 给定标识符更长时不反向匹配短条目。
    assert_eq!(array.text_for(&language("en-AU")), Some("fallback"));
    assert_eq!(array.text_for(&language("zh-Hant-TW")), Some("fallback"));
    // 前缀匹配：给定 zh 是数组标识符 zh-Hant 的前导且以 `-` 继续。
    assert_eq!(array.text_for(&language("zh")), Some("Traditional"));
    assert_eq!(
        array.text_for(&LanguageIdentifier::unknown()),
        Some("fallback")
    );
    assert_eq!(array.default_text(), Some("fallback"));

    let ordered = MultiLanguageTextArray::parse(&multi_language(&[
        ("en-US", "first"),
        ("en-Latn", "second"),
    ]))
    .expect("ordered array");
    assert_eq!(
        ordered.text_for(&language("en")),
        Some("first"),
        "前缀匹配按数组顺序"
    );
    assert_eq!(ordered.default_text(), None);

    let bare =
        MultiLanguageTextArray::parse(&multi_language(&[("en", "English")])).expect("single entry");
    assert_eq!(bare.text_for(&language("en-US")), None);
    assert_eq!(
        bare.text_for(&LanguageIdentifier::unknown()),
        None,
        "无默认条目时未知语言得到 None"
    );
}

#[test]
fn alternate_text_accepts_text_string_and_multi_language_array() {
    let plain = AlternateText::parse(&text("plain text")).expect("plain text");
    assert!(matches!(plain, AlternateText::Text(_)));
    assert_eq!(
        plain.text_for(&LanguageIdentifier::unknown()),
        Some("plain text"),
        "普通文本串与语言无关"
    );

    let multi = AlternateText::parse(&multi_language(&[
        ("", "default"),
        ("es-MX", "Spanish"),
        ("es-ES", "Castilian"),
    ]))
    .expect("multi-language alt");
    assert!(matches!(multi, AlternateText::MultiLanguage(_)));
    assert_eq!(multi.text_for(&language("es-MX")), Some("Spanish"));
    assert_eq!(multi.text_for(&language("es-ES")), Some("Castilian"));
    assert_eq!(multi.text_for(&language("fr")), Some("default"));

    let hex = AlternateText::parse(&Value::HexString("706C61696E".into())).expect("hex text");
    assert!(matches!(hex, AlternateText::Text(ref text) if text == "plain"));

    match AlternateText::parse(&Value::Integer(1)) {
        Err(LanguageError::Malformed(_)) => {}
        other => panic!("integer alt should be rejected, got {other:?}"),
    }
}

#[test]
fn media_clip_alt_decodes_plain_and_multi_language_text() {
    fn filespec() -> Dict {
        Dict::new()
            .insert("Type", name("Filespec"))
            .insert("FS", name("URL"))
            .insert("F", text("clip.mov"))
    }

    fn clip_data(alt: Value) -> Dict {
        Dict::new()
            .insert("Type", name("MediaClip"))
            .insert("S", name("MCD"))
            .insert("D", Value::Dict(filespec()))
            .insert("Alt", alt)
    }

    let doc = Document::new();
    let clip = doc
        .media_clip(&Value::Dict(clip_data(multi_language(&[
            ("", "default"),
            ("es-MX", "Spanish"),
        ]))))
        .expect("media clip data");
    let MediaClip::Data(data) = clip else {
        panic!("expected media clip data, got {clip:?}");
    };
    assert!(matches!(data.data, MediaClipDataSource::FileSpec(_)));
    let alt = data
        .alternate_text()
        .expect("decode /Alt")
        .expect("/Alt present");
    assert_eq!(alt.text_for(&language("es-MX")), Some("Spanish"));
    assert_eq!(alt.text_for(&language("fr")), Some("default"));

    let clip = doc
        .media_clip(&Value::Dict(clip_data(text("plain"))))
        .expect("media clip data");
    let MediaClip::Data(data) = clip else {
        panic!("expected media clip data");
    };
    assert!(matches!(
        data.alternate_text().expect("decode /Alt"),
        Some(AlternateText::Text(ref text)) if text == "plain"
    ));

    let section = Dict::new()
        .insert("Type", name("MediaClip"))
        .insert("S", name("MCS"))
        .insert("D", Value::Dict(clip_data(text("nested"))))
        .insert("Alt", multi_language(&[("en-GB", "British")]));
    let clip = doc.media_clip(&Value::Dict(section)).expect("clip section");
    let MediaClip::Section(section) = clip else {
        panic!("expected media clip section, got {clip:?}");
    };
    let alt = section
        .alternate_text()
        .expect("decode /Alt")
        .expect("/Alt present");
    assert_eq!(alt.text_for(&language("en-GB")), Some("British"));
}
