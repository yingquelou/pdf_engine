//! Span 属性表读端测试（ISO 32000-1 14.9.3–14.9.5）：`/Alt`、`/ActualText`、
//! `/E` 与 `/Lang` 的类型化解析，以及仅 `Span` 标签生效与文本串校验。

use crate::core::types::{Dict, Value};

use super::super::*;
use super::support::{assert_malformed, marked_section, name, text};

#[test]
fn span_properties_parse_text_entries_and_language() {
    let properties = Dict::new()
        .insert("Alt", text("plain alt"))
        .insert("ActualText", Value::HexString("FEFF4E2D".into()))
        .insert("E", text("expanded"))
        .insert("Lang", text(""));
    let parsed = SpanProperties::parse("Span", Some(&properties))
        .expect("typed span")
        .expect("span properties");
    assert_eq!(parsed.alternate_text.as_deref(), Some("plain alt"));
    assert_eq!(
        parsed.actual_text.as_deref(),
        Some("中"),
        "UTF-16BE 十六进制串"
    );
    assert_eq!(parsed.expansion.as_deref(), Some("expanded"));
    let language = parsed.language.as_ref().expect("/Lang declared");
    assert!(language.is_unknown());
    assert_eq!(language.as_str(), "");

    let section = marked_section("Span", Some(properties), None);
    assert_eq!(
        section.span_properties().expect("section span"),
        Some(parsed)
    );
}

#[test]
fn span_properties_require_span_tag_and_text_strings() {
    let properties = Dict::new().insert("Alt", text("alt"));
    assert_eq!(
        SpanProperties::parse("P", Some(&properties)).expect("non-Span tag"),
        None
    );
    assert_eq!(
        SpanProperties::parse("span", Some(&properties)).expect("tag is case-sensitive"),
        None
    );
    assert_eq!(
        SpanProperties::parse("Span", None).expect("missing properties"),
        None
    );
    assert_eq!(
        SpanProperties::parse("Span", Some(&Dict::new())).expect("empty properties"),
        Some(SpanProperties {
            alternate_text: None,
            actual_text: None,
            expansion: None,
            language: None,
        })
    );

    let invalid_alt = Dict::new().insert("Alt", Value::Integer(1));
    match SpanProperties::parse("Span", Some(&invalid_alt)) {
        Err(LanguageError::Malformed(message)) => {
            assert!(message.contains("Alt"), "{message}");
            assert!(message.contains("integer"), "{message}");
        }
        other => panic!("expected malformed /Alt, got {other:?}"),
    }
    let invalid_lang = Dict::new().insert("Lang", name("en"));
    assert_malformed(SpanProperties::parse("Span", Some(&invalid_lang)), "Lang");
}
