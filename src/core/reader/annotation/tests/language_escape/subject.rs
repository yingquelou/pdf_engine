use super::*;

#[test]
fn annotation_subject_with_language_escape_returns_plain_text_and_declared_language() {
    // Table 170 L21706–L21707 的 `/Subj` 是文本串，故与 `/T` 共用 7.9.2.2
    // 的转义口径：原串保留、语言视图按需解析。
    let subject = format!("{}Rechnung", escape("de"));
    let annotation = annotation_with_subject(unicode_text(&subject));

    assert_eq!(
        annotation.subject,
        Some(unicode_text(&subject)),
        "/Subj 保留未去转义的原串"
    );
    assert!(annotation.has_subject_language_escape());
    assert_eq!(
        annotation
            .subject_semantic_text()
            .unwrap()
            .expect("escaped /Subj")
            .escape_count(),
        1
    );
    assert_eq!(
        annotation.subject_text().unwrap().as_deref(),
        Some("Rechnung")
    );
    let prevailing = identifier("en");
    assert_eq!(
        language_tag(
            annotation
                .subject_language(Some(&prevailing))
                .unwrap()
                .as_ref()
        ),
        Some("de"),
        "串内转义 shall 覆盖 prevailing Lang（14.9.2.1 raw L31940–L31941）"
    );
}

#[test]
fn annotation_subject_without_escape_keeps_prevailing_lang_entry() {
    let annotation = annotation_with_subject(Value::String("Invoice".into()));
    assert!(!annotation.has_subject_language_escape());
    assert_eq!(
        annotation.subject_text().unwrap().as_deref(),
        Some("Invoice"),
        "无转义时自然文本即原串"
    );
    let prevailing = identifier("en-US");
    assert_eq!(
        language_tag(
            annotation
                .subject_language(Some(&prevailing))
                .unwrap()
                .as_ref()
        ),
        Some("en-US")
    );
    assert_eq!(annotation.subject_language(None).unwrap(), None);
}

#[test]
fn annotation_without_subject_has_no_language_view() {
    let annotation = annotation_without_title();
    assert_eq!(annotation.subject, None);
    assert!(!annotation.has_subject_language_escape());
    assert_eq!(annotation.subject_semantic_text().unwrap(), None);
    assert_eq!(annotation.subject_text().unwrap(), None);
    assert_eq!(annotation.subject_language(None).unwrap(), None);
}

#[test]
fn annotation_subject_unterminated_escape_is_typed_malformed_error() {
    // 残缺转义不影响注解读取与 `/T` 视图，只在索取 `/Subj` 语言视图时报错，
    // 且上下文是 `/Subj`（与 `/T` 的错误串可区分）。
    let subject = format!("broken {}", unterminated_escape("en"));
    let annotation = annotation_with_subject(unicode_text(&subject));
    assert!(annotation.has_subject_language_escape());
    assert_eq!(
        annotation.subject,
        Some(unicode_text(&subject)),
        "原串仍按解析结果保留"
    );
    assert_eq!(annotation.title_text().unwrap(), None, "无 /T 不受影响");

    for result in [
        annotation.subject_semantic_text().map(|_| ()),
        annotation.subject_text().map(|_| ()),
        annotation.subject_language(None).map(|_| ()),
    ] {
        let error = result.expect_err("unterminated escape shall be rejected");
        assert!(
            matches!(error, LanguageError::Malformed(_)),
            "残缺转义应是类型化 Malformed，got {error:?}"
        );
        assert!(error.to_string().contains("annotation /Subj"), "{error}");
        assert!(error.to_string().contains("not terminated"), "{error}");
    }
}
