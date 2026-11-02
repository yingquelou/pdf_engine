//! Unicode 语言转义与注解 `/Contents` 来源优先级测试
//! （ISO 32000-1 7.9.2.2、14.9.3、14.9.4、14.9.5、12.5.6.2）。

use crate::core::reader::accessibility::{
    AlternateText, AnnotationDescriptionSource, LANGUAGE_ESCAPE, LanguageError, LanguageIdentifier,
    MultiLanguageTextArray, SemanticText, SemanticTextEntry, annotation_description_source,
};
use crate::core::reader::annotation::AnnotationSubtype;
use crate::core::reader::content::MarkedSection;
use crate::core::types::{Array, Dict, Value};

const ESCAPE: char = LANGUAGE_ESCAPE;

fn name(value: &str) -> Value {
    Value::Name(value.to_owned())
}

fn text(value: &str) -> Value {
    Value::String(value.to_owned())
}

fn arr(values: Vec<Value>) -> Value {
    Value::Array(Array(values))
}

/// UTF-16BE + BOM 的十六进制文本串：U+001B 在 PDFDocEncoding 里不可达，
/// 语言转义的规范载体就是这种编码。
fn unicode_text(value: &str) -> Value {
    let mut hex = String::from("FEFF");
    for unit in value.encode_utf16() {
        hex.push_str(&format!("{unit:04X}"));
    }
    Value::HexString(hex)
}

fn identifier(value: &str) -> LanguageIdentifier {
    LanguageIdentifier::parse(value).unwrap_or_else(|error| panic!("{value:?}: {error}"))
}

fn marked_section(tag: &str, properties: Option<Dict>) -> MarkedSection {
    MarkedSection {
        tag: tag.to_owned(),
        properties,
        visibility: None,
        start: 0,
        end: 0,
        point: false,
        parent: None,
    }
}

#[test]
fn semantic_text_parse_reads_two_byte_iso639_escape_and_strips_delimiters() {
    let parsed = SemanticText::parse(&format!("{ESCAPE}ja{ESCAPE}star")).expect("two-byte escape");
    assert_eq!(parsed.escape_count(), 1);
    assert!(parsed.has_language_escape());
    assert_eq!(parsed.plain_text(), "star");
    assert_eq!(parsed.segments().len(), 1);
    let segment = &parsed.segments()[0];
    assert_eq!(segment.text(), "star");
    assert_eq!(segment.language(), Some(&identifier("ja")));
    assert_eq!(parsed.escape_language(), Some(&identifier("ja")));
}

#[test]
fn semantic_text_parse_reads_four_byte_iso639_with_iso3166_escape() {
    let parsed =
        SemanticText::parse(&format!("six{ESCAPE}enus{ESCAPE}point")).expect("code with country");
    assert_eq!(parsed.plain_text(), "sixpoint");
    assert_eq!(parsed.segments().len(), 2);
    assert_eq!(parsed.segments()[0].language(), None);
    assert_eq!(parsed.segments()[0].text(), "six");
    assert_eq!(parsed.segments()[1].language(), Some(&identifier("en-us")));
    assert_eq!(parsed.segments()[1].text(), "point");
}

#[test]
fn semantic_text_parse_keeps_unescaped_prefix_segment_language_unknown() {
    let parsed = SemanticText::parse(&format!("plain{ESCAPE}de{ESCAPE}alt")).expect("late escape");
    assert_eq!(parsed.segments().len(), 2);
    assert_eq!(
        parsed.segments()[0].language(),
        None,
        "转义之前的段没有转义语言，沿用 prevailing Lang"
    );
    assert_eq!(parsed.segments()[0].text(), "plain");
    assert_eq!(parsed.segments()[1].language(), Some(&identifier("de")));
    assert_eq!(
        parsed.effective_language(Some(&identifier("en-US"))),
        Some(identifier("de")),
        "整串生效语言按 14.9.3 取首个转义声明的语言"
    );
}

#[test]
fn semantic_text_parse_handles_two_escapes_in_one_string() {
    let parsed = SemanticText::parse(&format!("{ESCAPE}en{ESCAPE}first {ESCAPE}fr{ESCAPE}second"))
        .expect("two escapes");
    assert_eq!(parsed.escape_count(), 2);
    assert_eq!(parsed.plain_text(), "first second");
    let languages: Vec<String> = parsed
        .segments()
        .iter()
        .map(|segment| {
            segment
                .language()
                .map(|language| language.as_str().to_owned())
                .unwrap_or_default()
        })
        .collect();
    assert_eq!(languages, vec!["en".to_owned(), "fr".to_owned()]);
    assert_eq!(parsed.effective_language(None), Some(identifier("en")));
}

#[test]
fn semantic_text_parse_rejects_unterminated_escape() {
    match SemanticText::parse(&format!("lead{ESCAPE}enus")) {
        Err(LanguageError::Malformed(message)) => {
            assert!(message.contains("not terminated"), "{message}");
            assert!(message.contains("character offset 4"), "{message}");
        }
        other => panic!("expected malformed escape, got {other:?}"),
    }
}

#[test]
fn semantic_text_parse_rejects_escape_with_wrong_code_length() {
    for body in ["", "e", "abc", "abcde"] {
        let value = format!("{ESCAPE}{body}{ESCAPE}tail");
        match SemanticText::parse(&value) {
            Err(LanguageError::Malformed(message)) => {
                assert!(message.contains("2-byte ISO 639"), "{body:?}: {message}");
            }
            other => panic!("{body:?}: expected malformed escape, got {other:?}"),
        }
    }
}

#[test]
fn semantic_text_parse_rejects_non_alphabetic_language_code() {
    for body in ["e1", "1n", "e-", "e中"] {
        let value = format!("{ESCAPE}{body}{ESCAPE}tail");
        match SemanticText::parse(&value) {
            Err(LanguageError::Malformed(message)) => {
                assert!(message.contains("ASCII letters"), "{body:?}: {message}");
            }
            other => panic!("{body:?}: expected malformed escape, got {other:?}"),
        }
    }
}

#[test]
fn semantic_text_without_escape_falls_back_to_prevailing_language() {
    let parsed = SemanticText::parse("six-point star").expect("no escape");
    assert_eq!(parsed.escape_count(), 0);
    assert!(!parsed.has_language_escape());
    assert_eq!(parsed.segments().len(), 1);
    assert_eq!(parsed.segments()[0].language(), None);
    assert_eq!(parsed.plain_text(), "six-point star");
    assert_eq!(parsed.escape_language(), None);
    assert_eq!(
        parsed.effective_language(Some(&identifier("en-US"))),
        Some(identifier("en-US")),
        "无转义时沿用 prevailing Lang"
    );
    assert_eq!(parsed.effective_language(None), None);
}

#[test]
fn semantic_text_escape_overrides_prevailing_lang_entry() {
    let parsed = SemanticText::parse(&format!("{ESCAPE}ja{ESCAPE}star")).expect("escape");
    assert_eq!(
        parsed.effective_language(Some(&identifier("en-US"))),
        Some(identifier("ja")),
        "14.9.3 NOTE 3：转义 shall 覆盖 prevailing Lang"
    );
    assert_eq!(parsed.effective_language(None), Some(identifier("ja")));
}

#[test]
fn semantic_text_from_value_requires_text_string_types() {
    let cases = [
        name("en"),
        Value::Integer(1),
        arr(vec![text("en"), text("alt")]),
        Value::Dict(Dict::new()),
    ];
    for value in cases {
        match SemanticText::from_value(&value) {
            Err(LanguageError::Malformed(message)) => {
                assert!(message.contains("text string"), "{message}");
                assert!(message.contains(value.type_name()), "{message}");
            }
            other => panic!("expected malformed entry, got {other:?}"),
        }
    }
}

#[test]
fn semantic_text_from_value_decodes_unicode_and_pdfdocencoded_strings() {
    // UTF-16BE + BOM（7.9.2.2 的规范形态：`00 1B` 即 U+001B）。
    let unicode = Value::HexString("FEFF001B006A0061001B0053".to_owned());
    let parsed = SemanticText::from_value(&unicode).expect("UTF-16BE escape");
    assert_eq!(parsed.plain_text(), "S");
    assert_eq!(parsed.escape_language(), Some(&identifier("ja")));

    // 14.9.3 的编码 shall 只允许 PDFDocEncoding 或 Unicode；PDFDocEncoding
    // 的第 27 字节按 Annex D Table D.1（raw L34054）映射为 U+02D9，不是 U+001B，
    // 因此这种编码里根本不会出现语言转义（与 7.9.2.2 的原文一致）。
    let pdfdoc = text(&format!("{ESCAPE}de{ESCAPE}alt"));
    let parsed = SemanticText::from_value(&pdfdoc).expect("PDFDocEncoding string");
    assert_eq!(parsed.escape_count(), 0, "PDFDocEncoding 不携带 U+001B");
    assert!(
        !parsed.plain_text().contains(ESCAPE),
        "{}",
        parsed.plain_text()
    );

    let country = unicode_text(&format!("{ESCAPE}zhCN{ESCAPE}\u{661f}"));
    let parsed = SemanticText::from_value(&country).expect("UTF-16BE with country");
    assert_eq!(parsed.escape_language(), Some(&identifier("zh-cn")));
    assert_eq!(parsed.plain_text(), "\u{661f}");
}

#[test]
fn alternate_text_semantic_text_selects_array_entry_before_applying_escape() {
    // 数组条目本身也必须是 UTF-16BE 文本串才可能带转义。
    let array_value = arr(vec![
        text("en"),
        text("Hello"),
        text("fr"),
        unicode_text(&format!("{ESCAPE}ja{ESCAPE}bonjour")),
        text(""),
        text("default"),
    ]);
    let array = AlternateText::parse(&array_value).expect("multi-language array");
    // 数组条目选择用 prevailing Lang；转义只在选中的条目内部生效。
    let parsed = array
        .semantic_text_for(Some(&identifier("fr")))
        .expect("fr entry")
        .expect("entry present");
    assert_eq!(parsed.plain_text(), "bonjour");
    assert_eq!(
        parsed.effective_language(Some(&identifier("fr"))),
        Some(identifier("ja")),
        "选中的是 fr 条目，但条目内转义把生效语言改成 ja"
    );

    let english = array
        .semantic_text_for(Some(&identifier("en")))
        .expect("en entry")
        .expect("entry present");
    assert!(!english.has_language_escape());
    assert_eq!(english.plain_text(), "Hello");
    assert_eq!(
        english.effective_language(Some(&identifier("en"))),
        Some(identifier("en"))
    );

    // 前缀/默认匹配仍在转义之前完成：de-DE 没有条目，退到默认文本。
    let fallback = array
        .semantic_text_for(Some(&identifier("de-DE")))
        .expect("default entry")
        .expect("default text present");
    assert_eq!(fallback.plain_text(), "default");
}

#[test]
fn alternate_text_semantic_text_returns_none_when_array_has_no_match() {
    let value = arr(vec![
        text("it"),
        unicode_text(&format!("{ESCAPE}ja{ESCAPE}ciao")),
    ]);
    let array = AlternateText::MultiLanguage(MultiLanguageTextArray::parse(&value).expect("array"));
    let parsed = array
        .semantic_text_for(Some(&identifier("de")))
        .expect("lookup");
    assert!(parsed.is_none(), "没有默认文本且无匹配时不返回任何条目");
}

#[test]
fn marked_section_span_semantic_text_parses_property_list_entries() {
    let alt = unicode_text(&format!("{ESCAPE}enus{ESCAPE}six-point star"));
    let properties = Dict::new().insert("Alt", alt.clone());
    let section = marked_section("Span", Some(properties.clone()));
    let parsed = section
        .span_semantic_text(SemanticTextEntry::AlternateText)
        .expect("span /Alt")
        .expect("entry present");
    assert_eq!(parsed.plain_text(), "six-point star");
    assert_eq!(parsed.escape_language(), Some(&identifier("en-us")));

    assert_eq!(
        section
            .span_semantic_text(SemanticTextEntry::Expansion)
            .expect("missing /E"),
        None
    );

    let non_span = marked_section("P", Some(properties));
    assert_eq!(
        non_span
            .span_semantic_text(SemanticTextEntry::AlternateText)
            .expect("non-Span tag"),
        None,
        "只有精确 Span 标签才建模这四个条目"
    );
    assert_eq!(
        marked_section("Span", None)
            .span_semantic_text(SemanticTextEntry::ActualText)
            .expect("no property list"),
        None
    );

    let invalid = marked_section(
        "Span",
        Some(Dict::new().insert("ActualText", Value::Integer(7))),
    );
    match invalid.span_semantic_text(SemanticTextEntry::ActualText) {
        Err(LanguageError::Malformed(message)) => {
            assert!(message.contains("text string"), "{message}");
        }
        other => panic!("expected malformed /ActualText, got {other:?}"),
    }
}

#[test]
fn semantic_text_entry_exposes_dict_keys_and_rejects_malformed_escape() {
    assert_eq!(SemanticTextEntry::AlternateText.dict_key(), "Alt");
    assert_eq!(SemanticTextEntry::ActualText.dict_key(), "ActualText");
    assert_eq!(SemanticTextEntry::Expansion.dict_key(), "E");
    let bad = unicode_text(&format!("{ESCAPE}too short{ESCAPE}"));
    let dict = Dict::new().insert("E", bad);
    match SemanticText::from_dict(&dict, SemanticTextEntry::Expansion) {
        Err(LanguageError::Malformed(message)) => {
            assert!(message.contains("2-byte ISO 639"), "{message}");
        }
        other => panic!("expected malformed escape, got {other:?}"),
    }
    assert_eq!(
        SemanticText::from_dict(&Dict::new(), SemanticTextEntry::Expansion).expect("missing key"),
        None
    );
}

#[test]
fn annotation_description_source_requires_contents_for_markup_displaying_types() {
    // 12.5.6.2：正常显示文本的注解类型 → /Contents 是 shall 的来源。
    for subtype in [
        AnnotationSubtype::Text,
        AnnotationSubtype::FreeText,
        AnnotationSubtype::Line,
        AnnotationSubtype::Square,
        AnnotationSubtype::Circle,
        AnnotationSubtype::Polygon,
        AnnotationSubtype::PolyLine,
        AnnotationSubtype::Highlight,
        AnnotationSubtype::Underline,
        AnnotationSubtype::Squiggly,
        AnnotationSubtype::StrikeOut,
        AnnotationSubtype::Caret,
        AnnotationSubtype::Ink,
        AnnotationSubtype::Stamp,
        AnnotationSubtype::FileAttachment,
        AnnotationSubtype::Redact,
    ] {
        let source = annotation_description_source(&subtype);
        assert_eq!(
            source,
            AnnotationDescriptionSource::ContentsRequired,
            "{subtype:?} 属于 12.5.6.2 的 markup 组"
        );
        assert!(source.is_required());
    }
}

#[test]
fn annotation_description_source_marks_sound_and_non_displaying_types() {
    assert_eq!(
        annotation_description_source(&AnnotationSubtype::Sound),
        AnnotationDescriptionSource::SoundNote,
        "14.9.3：Sound 不需要用于朗读的替代描述"
    );
    assert!(!AnnotationDescriptionSource::SoundNote.is_required());

    for subtype in [
        AnnotationSubtype::Link,
        AnnotationSubtype::Movie,
        AnnotationSubtype::Widget,
        AnnotationSubtype::Screen,
        AnnotationSubtype::PrinterMark,
        AnnotationSubtype::TrapNet,
        AnnotationSubtype::Watermark,
        AnnotationSubtype::ThreeD,
        AnnotationSubtype::Popup,
        AnnotationSubtype::Unknown("Foo".to_owned()),
    ] {
        let source = annotation_description_source(&subtype);
        assert_eq!(
            source,
            AnnotationDescriptionSource::ContentsOptional,
            "{subtype:?} 不显示文本，/Contents 只是可选描述"
        );
        assert!(!source.is_required());
    }
}
