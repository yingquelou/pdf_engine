//! RFC 3066 语言标识符的接受、拒绝与大小写无关比较（ISO 32000-1 14.9.2.2）。

use std::collections::HashSet;

use super::super::*;
use super::support::language;

#[test]
fn language_identifier_accepts_rfc3066_and_unknown() {
    for value in [
        "",
        "en",
        "en-US",
        "zh-Hant",
        "x-default",
        "EN-us",
        "en-abcdefgh",
        "i-klingon",
        "abc-1",
        "english",
        "abcdefgh-Latn",
    ] {
        let parsed = LanguageIdentifier::parse(value)
            .unwrap_or_else(|error| panic!("{value:?} should be accepted: {error}"));
        assert_eq!(parsed.as_str(), value, "原始拼写应保留");
        assert_eq!(parsed.to_string(), value);
    }

    let unknown = LanguageIdentifier::unknown();
    assert!(unknown.is_unknown());
    assert_eq!(unknown.as_str(), "");
    assert_eq!(LanguageIdentifier::parse("").expect("empty tag"), unknown);
    assert!(!language("en").is_unknown());
}

#[test]
fn language_identifier_rejects_malformed_tags() {
    for value in [
        "en_US",
        "-en",
        "en-",
        "123",
        "abcdefghi",
        "en-abcdefghi",
        "en--US",
        " en",
        "en ",
        "en-US-",
        "é",
        "en-é",
    ] {
        match LanguageIdentifier::parse(value) {
            Err(LanguageError::Malformed(_)) => {}
            other => panic!("{value:?} should be rejected, got {other:?}"),
        }
    }
}

#[test]
fn language_identifiers_compare_case_insensitively() {
    let mixed = language("EN-us");
    assert_eq!(mixed, language("en-US"));
    assert_eq!(mixed, language("En-Us"));
    assert_ne!(language("en"), language("en-US"));
    assert_eq!(mixed.to_string(), "EN-us", "Display 应保留原串");

    let mut set = HashSet::new();
    set.insert(language("en-US"));
    set.insert(language("EN-us"));
    assert_eq!(set.len(), 1, "Hash 必须与 Eq 一致");
    assert!(set.contains(&language("eN-uS")));
    assert!(!set.contains(&LanguageIdentifier::unknown()));

    let mut unknown_set = HashSet::new();
    unknown_set.insert(LanguageIdentifier::unknown());
    assert!(
        unknown_set.contains(&language("")),
        "空串的 Hash 与 Eq 一致"
    );
}
