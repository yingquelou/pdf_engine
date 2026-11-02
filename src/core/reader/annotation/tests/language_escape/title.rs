use super::*;

#[test]
fn annotation_title_with_language_escape_returns_plain_text_and_declared_language() {
    let title = format!("{}付箋", escape("ja"));
    let annotation = annotation_with_title(unicode_text(&title));

    // `title` 保留解码原串（不去转义），与书签 `/Title` 同一口径。
    assert_eq!(
        annotation.title,
        Some(unicode_text(&title)),
        "/T keeps the undecoded escaped string"
    );
    assert!(annotation.has_title_language_escape());
    assert_eq!(
        annotation
            .title_semantic_text()
            .unwrap()
            .expect("escaped /T")
            .escape_count(),
        1
    );
    assert_eq!(annotation.title_text().unwrap().as_deref(), Some("付箋"));
    let prevailing = identifier("en");
    assert_eq!(
        language_tag(
            annotation
                .title_language(Some(&prevailing))
                .unwrap()
                .as_ref()
        ),
        Some("ja"),
        "串内转义 shall 覆盖 prevailing Lang（14.9.2.1 raw L31940–L31941）"
    );
}

#[test]
fn annotation_title_without_escape_keeps_prevailing_lang_entry() {
    let annotation = annotation_with_title(Value::String("Reviewer".into()));
    assert!(!annotation.has_title_language_escape());
    assert_eq!(
        annotation.title_text().unwrap().as_deref(),
        Some("Reviewer"),
        "无转义时自然文本即原串"
    );
    let text = annotation
        .title_semantic_text()
        .unwrap()
        .expect("plain title still yields a view");
    assert_eq!(text.escape_count(), 0);
    assert!(!text.has_language_escape());
    assert_eq!(text.segments().len(), 1);
    assert_eq!(segments_language_tag(&text, 0), None);

    let prevailing = identifier("en-US");
    assert_eq!(
        language_tag(
            annotation
                .title_language(Some(&prevailing))
                .unwrap()
                .as_ref()
        ),
        Some("en-US"),
        "注解自身没有 /Lang，prevailing 由调用方传入目录 /Lang"
    );
    assert_eq!(
        annotation.title_language(None).unwrap(),
        None,
        "文档未声明 /Lang 时不臆造语言"
    );
}

#[test]
fn annotation_without_title_has_no_language_view() {
    let annotation = annotation_without_title();
    assert!(!annotation.has_title_language_escape());
    assert_eq!(annotation.title, None);
    assert_eq!(
        annotation.title_semantic_text().unwrap(),
        None,
        "缺 /T 返回 Ok(None) 而非空视图"
    );
    assert_eq!(annotation.title_text().unwrap(), None);
    assert_eq!(annotation.title_language(None).unwrap(), None);
}

#[test]
fn annotation_title_segments_split_at_each_escape() {
    let title = format!("{}Note {}Bemerkung", escape("en"), escape("fr"));
    let annotation = annotation_with_title(unicode_text(&title));

    let text = annotation
        .title_semantic_text()
        .unwrap()
        .expect("both escapes terminate");
    assert_eq!(text.escape_count(), 2);
    assert!(text.has_language_escape());
    let segments = text.segments();
    assert_eq!(segments.len(), 2);
    assert_eq!(segments[0].text(), "Note ");
    assert_eq!(segments[1].text(), "Bemerkung");
    assert_eq!(text.plain_text(), "Note Bemerkung");
    // 整串语言取首个转义（14.9.3 NOTE 3 的 shall）；逐段结论以 `segments` 为准。
    assert_eq!(
        language_tag(text.effective_language(Some(&identifier("fr"))).as_ref()),
        Some("en")
    );
}

#[test]
fn annotation_title_segment_language_follows_escape_and_prevailing_before_it() {
    // 转义之前的段沿用 prevailing（language 为 None），其后的段取转义语言。
    let title = format!("Vorwort {}Ende", escape("de"));
    let annotation = annotation_with_title(unicode_text(&title));
    let text = annotation
        .title_semantic_text()
        .unwrap()
        .expect("escape terminates");
    assert_eq!(text.segments().len(), 2);
    assert_eq!(segments_language_tag(&text, 0), None);
    assert_eq!(text.segments()[0].text(), "Vorwort ");
    assert_eq!(segments_language_tag(&text, 1), Some("de"));
    assert_eq!(text.segments()[1].text(), "Ende");
    // 逐段语言不受整串结论影响。
    assert_eq!(
        language_tag(
            annotation
                .title_language(Some(&identifier("en")))
                .unwrap()
                .as_ref()
        ),
        Some("de")
    );
}

#[test]
fn annotation_title_with_iso_3166_country_code_escape_resolves_regional_tag() {
    // 7.9.2.2 c)（raw L4246）的可选 2 字节 ISO 3166 码拼为 `de-AT`。
    let title = format!("{}Einleitung", escape("deAT"));
    let annotation = annotation_with_title(unicode_text(&title));
    assert_eq!(
        annotation.title_text().unwrap().as_deref(),
        Some("Einleitung")
    );
    assert_eq!(
        language_tag(annotation.title_language(None).unwrap().as_ref()),
        Some("de-AT")
    );
}

#[test]
fn annotation_title_unterminated_escape_is_typed_malformed_error() {
    // 残缺转义不毁掉注解本身的读取，但索取语言视图时必须报错（库策略：不
    // 静默把残缺转义当普通文本），错误沿用既有 LanguageError::Malformed。
    let title = format!("broken {}", unterminated_escape("en"));
    let annotation = annotation_with_title(unicode_text(&title));
    assert!(
        annotation.has_title_language_escape(),
        "残缺转义仍含 U+001B，存在性判断不校验合法性"
    );
    assert_eq!(
        annotation.title,
        Some(unicode_text(&title)),
        "原串仍按解析结果保留"
    );

    for result in [
        annotation.title_semantic_text().map(|_| ()),
        annotation.title_text().map(|_| ()),
        annotation.title_language(None).map(|_| ()),
    ] {
        let error = result.expect_err("unterminated escape shall be rejected");
        assert!(
            matches!(error, LanguageError::Malformed(_)),
            "残缺转义应是类型化 Malformed，got {error:?}"
        );
        assert!(error.to_string().contains("annotation /T"), "{error}");
        assert!(error.to_string().contains("not terminated"), "{error}");
    }
}

#[test]
fn annotation_title_non_ascii_letter_escape_code_is_typed_malformed_error() {
    let title = format!("{}x", escape("e1"));
    let annotation = annotation_with_title(unicode_text(&title));
    let error = annotation
        .title_semantic_text()
        .expect_err("digits are no ISO 639 code");
    assert!(matches!(error, LanguageError::Malformed(_)));
    assert!(error.to_string().contains("ASCII letters"), "{error}");

    // 码长非 2/4 同样报错，且带上条目上下文。
    let title = format!("{}x", escape("eng"));
    let error = annotation_with_title(unicode_text(&title))
        .title_text()
        .expect_err("three-byte codes are not a valid escape");
    assert!(error.to_string().contains("annotation /T"), "{error}");
    assert!(error.to_string().contains("2-byte ISO 639"), "{error}");
}

#[test]
fn annotation_title_literal_pdfdocencoding_escape_byte_is_not_a_language_escape() {
    // 字面串里的 0x1B 按 PDFDocEncoding（Annex D）解为 U+02D9，不是 U+001B，
    // 故不构成转义：prevailing Lang 不被覆盖、也无转义可剥离。
    let raw = format!("{}Chapter", escape("ja"));
    let annotation = annotation_with_title(Value::String(raw));
    assert!(
        !annotation.has_title_language_escape(),
        "PDFDocEncoding 字面串解不出 U+001B，故无转义"
    );
    assert_eq!(
        annotation.title_text().unwrap().as_deref(),
        Some("\u{02D9}ja\u{02D9}Chapter"),
        "原样返回解码后的文本"
    );
    let prevailing = identifier("en");
    assert_eq!(
        language_tag(
            annotation
                .title_language(Some(&prevailing))
                .unwrap()
                .as_ref()
        ),
        Some("en"),
        "无转义时 prevailing 保持生效"
    );
}
