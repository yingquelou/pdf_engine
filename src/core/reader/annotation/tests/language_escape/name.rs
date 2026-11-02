use super::*;

#[test]
fn annotation_name_with_language_escape_returns_plain_text_and_declared_language() {
    // Table 164 L21283–L21284 的 `/NM` 是文本串，与 `/T`、`/Subj` 共用
    // 7.9.2.2 的转义口径：原串保留、语言视图按需解析。
    let name = format!("{}付箋-01", escape("ja"));
    let annotation = annotation_with_name(unicode_text(&name));

    assert_eq!(
        annotation.name,
        Some(unicode_text(&name)),
        "/NM 保留未去转义的原串"
    );
    assert!(annotation.has_name_language_escape());
    assert_eq!(
        annotation
            .name_semantic_text()
            .unwrap()
            .expect("escaped /NM")
            .escape_count(),
        1
    );
    assert_eq!(annotation.name_text().unwrap().as_deref(), Some("付箋-01"));
    let prevailing = identifier("en");
    assert_eq!(
        language_tag(
            annotation
                .name_language(Some(&prevailing))
                .unwrap()
                .as_ref()
        ),
        Some("ja"),
        "串内转义 shall 覆盖 prevailing Lang（14.9.2.1 raw L31940–L31941）"
    );
}

#[test]
fn annotation_name_without_escape_keeps_original_string_and_prevailing_lang() {
    let annotation = annotation_with_name(Value::String("annot-42".into()));
    assert!(!annotation.has_name_language_escape());
    assert_eq!(
        annotation.name_text().unwrap().as_deref(),
        Some("annot-42"),
        "无转义时自然文本即原串"
    );
    let text = annotation
        .name_semantic_text()
        .unwrap()
        .expect("plain /NM still yields a view");
    assert_eq!(text.escape_count(), 0);
    assert!(!text.has_language_escape());
    assert_eq!(text.segments().len(), 1);
    assert_eq!(segments_language_tag(&text, 0), None);
    assert_eq!(
        text.plain_text(),
        "annot-42",
        "去转义文本与 /NM 原串逐字相同"
    );

    let prevailing = identifier("en-US");
    assert_eq!(
        language_tag(
            annotation
                .name_language(Some(&prevailing))
                .unwrap()
                .as_ref()
        ),
        Some("en-US"),
        "注解自身没有 /Lang，prevailing 由调用方传入目录 /Lang"
    );
    assert_eq!(annotation.name_language(None).unwrap(), None);
}

#[test]
fn annotation_without_name_has_no_language_view() {
    let annotation = annotation_without_name();
    assert_eq!(annotation.name, None);
    assert!(!annotation.has_name_language_escape());
    assert_eq!(
        annotation.name_semantic_text().unwrap(),
        None,
        "缺 /NM 返回 Ok(None) 而非空视图"
    );
    assert_eq!(annotation.name_text().unwrap(), None);
    assert_eq!(annotation.name_language(None).unwrap(), None);
}

#[test]
fn annotation_name_segments_split_at_each_escape() {
    let name = format!("{}note {}NOTiz", escape("en"), escape("de"));
    let annotation = annotation_with_name(unicode_text(&name));
    let text = annotation
        .name_semantic_text()
        .unwrap()
        .expect("both escapes terminate");
    assert_eq!(text.escape_count(), 2);
    assert!(text.has_language_escape());
    let segments = text.segments();
    assert_eq!(segments.len(), 2);
    assert_eq!(segments[0].text(), "note ");
    // 串首即转义，故第一段已带 `en`；只有转义之前的文本才落到 `None`。
    assert_eq!(segments_language_tag(&text, 0), Some("en"));
    assert_eq!(segments[1].text(), "NOTiz");
    assert_eq!(segments_language_tag(&text, 1), Some("de"));
    assert_eq!(text.plain_text(), "note NOTiz");
    assert_eq!(
        language_tag(text.effective_language(Some(&identifier("fr"))).as_ref(),),
        Some("en"),
        "整串语言取首个转义"
    );
}

#[test]
fn annotation_name_with_iso_3166_country_code_escape_resolves_regional_tag() {
    // 7.9.2.2 c)（raw L4246）的可选 2 字节 ISO 3166 码拼为 `de-AT`。
    let name = format!("{}Anmerkung", escape("deAT"));
    let annotation = annotation_with_name(unicode_text(&name));
    assert_eq!(
        annotation.name_text().unwrap().as_deref(),
        Some("Anmerkung")
    );
    assert_eq!(
        language_tag(annotation.name_language(None).unwrap().as_ref()),
        Some("de-AT")
    );
}

#[test]
fn annotation_name_unterminated_escape_is_typed_malformed_error() {
    // 残缺转义不毁掉注解本身的读取，只在索取 `/NM` 语言视图时报错，
    // 且上下文是 `/NM`（与 `/T`、`/Subj` 的错误串可区分）。
    let name = format!("broken {}", unterminated_escape("en"));
    let annotation = annotation_with_name(unicode_text(&name));
    assert!(
        annotation.has_name_language_escape(),
        "残缺转义仍含 U+001B，存在性判断不校验合法性"
    );
    assert_eq!(
        annotation.name,
        Some(unicode_text(&name)),
        "原串仍按解析结果保留"
    );
    assert_eq!(annotation.title_text().unwrap(), None, "无 /T 不受影响");

    for result in [
        annotation.name_semantic_text().map(|_| ()),
        annotation.name_text().map(|_| ()),
        annotation.name_language(None).map(|_| ()),
    ] {
        let error = result.expect_err("unterminated escape shall be rejected");
        assert!(
            matches!(error, LanguageError::Malformed(_)),
            "残缺转义应是类型化 Malformed，got {error:?}"
        );
        assert!(error.to_string().contains("annotation /NM"), "{error}");
        assert!(error.to_string().contains("not terminated"), "{error}");
    }
}

#[test]
fn annotation_name_non_ascii_letter_escape_code_is_typed_malformed_error() {
    // 非法转义序列一律返回类型化错误而非 panic（库策略：报错不静默当文本）。
    let name = format!("{}x", escape("e1"));
    let error = annotation_with_name(unicode_text(&name))
        .name_semantic_text()
        .expect_err("digits are no ISO 639 code");
    assert!(matches!(error, LanguageError::Malformed(_)));
    assert!(error.to_string().contains("annotation /NM"), "{error}");
    assert!(error.to_string().contains("ASCII letters"), "{error}");

    let name = format!("{}x", escape("eng"));
    let error = annotation_with_name(unicode_text(&name))
        .name_text()
        .expect_err("three-byte codes are not a valid escape");
    assert!(error.to_string().contains("2-byte ISO 639"), "{error}");
}

#[test]
fn annotation_name_literal_pdfdocencoding_escape_byte_is_not_a_language_escape() {
    // 字面串里的 0x1B 按附录 D.3 的 PDFDocEncoding 表（raw L34054）解为
    // U+02D9，不是 U+001B，故不构成转义：prevailing Lang 不被覆盖。
    let raw = format!("{}draft", escape("ja"));
    let annotation = annotation_with_name(Value::String(raw));
    assert!(
        !annotation.has_name_language_escape(),
        "PDFDocEncoding 字面串解不出 U+001B，故无转义"
    );
    assert_eq!(
        annotation.name_text().unwrap().as_deref(),
        Some("\u{02D9}ja\u{02D9}draft"),
        "原样返回解码后的文本"
    );
    let prevailing = identifier("en");
    assert_eq!(
        language_tag(
            annotation
                .name_language(Some(&prevailing))
                .unwrap()
                .as_ref()
        ),
        Some("en"),
        "无转义时 prevailing 保持生效"
    );
}
