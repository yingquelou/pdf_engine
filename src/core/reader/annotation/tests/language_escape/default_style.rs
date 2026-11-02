use super::*;

#[test]
fn free_text_default_style_text_string_with_escape_returns_plain_text_and_declared_language() {
    // Table 174 L21909 的 `/DS` 是文本串：原串（已解码、未去转义）保留在
    // `default_style`，语言视图按需解析。
    let style = format!("{}font: ArialMT 12pt;", escape("ja"));
    let annotation = free_text_with_default_style(unicode_text(&style));

    assert_eq!(
        annotation.default_style.as_deref(),
        Some(style.as_str()),
        "/DS 保留未去转义的解码原串"
    );
    assert!(annotation.has_default_style_language_escape());
    assert_eq!(
        annotation
            .default_style_semantic_text()
            .unwrap()
            .expect("escaped text-string /DS")
            .escape_count(),
        1
    );
    assert_eq!(
        annotation
            .default_style_text()
            .unwrap()
            .as_deref()
            .expect("/DS 视图"),
        "font: ArialMT 12pt;"
    );
    let prevailing = identifier("en");
    assert_eq!(
        language_tag(
            annotation
                .default_style_language(Some(&prevailing))
                .unwrap()
                .as_ref()
        ),
        Some("ja"),
        "串内转义 shall 覆盖 prevailing Lang（14.9.2.1 raw L31940–L31941）"
    );
    // 其余条目不受 `/DS` 残缺与否影响。
    assert_eq!(
        annotation.default_appearance.as_deref(),
        Some("/F1 12 Tf 0 g")
    );
}

#[test]
fn free_text_default_style_without_escape_keeps_original_string_and_prevailing_lang() {
    let annotation = free_text_with_default_style(Value::String("font: 18pt Arial;".into()));
    assert!(!annotation.has_default_style_language_escape());
    let text = annotation
        .default_style_semantic_text()
        .unwrap()
        .expect("plain /DS still yields a view");
    assert_eq!(text.escape_count(), 0);
    assert_eq!(text.segments().len(), 1);
    assert_eq!(segments_language_tag(&text, 0), None);
    assert_eq!(
        annotation
            .default_style_text()
            .unwrap()
            .as_deref()
            .expect("/DS 视图"),
        "font: 18pt Arial;",
        "无转义时自然文本即原串"
    );
    assert_eq!(
        annotation
            .default_style_language(Some(&identifier("en-US")))
            .unwrap(),
        Some(identifier("en-US"))
    );
    assert_eq!(annotation.default_style_language(None).unwrap(), None);
}

#[test]
fn free_text_without_default_style_has_no_language_view() {
    let document = doc(vec![catalog_object()]);
    let annotation = parse_annotation(
        &document,
        0,
        None,
        &base_annotation("FreeText").insert("DA", Value::String("/F1 12 Tf 0 g".into())),
    )
    .expect("free text without /DS parses");
    assert_eq!(annotation.default_style, None);
    assert!(!annotation.has_default_style_language_escape());
    assert_eq!(
        annotation.default_style_semantic_text().unwrap(),
        None,
        "缺 /DS 返回 Ok(None)"
    );
    assert_eq!(annotation.default_style_text().unwrap(), None);
    assert_eq!(
        annotation
            .default_style_language(Some(&identifier("en")))
            .unwrap(),
        None,
        "没有 /DS 就无所谓语言"
    );
}

#[test]
fn free_text_default_style_as_name_has_no_language_escape_and_no_error() {
    // 解析器按库策略用 parse_name_or_text 宽容接受名字形态的 `/DS`；
    // 7.9.2.2 只把转义定义在文本串里，故名字形态一律判为无转义、视图为
    // Ok(None)，而不是报错。
    let annotation = free_text_with_default_style(Value::Name("ArialMT".into()));
    assert_eq!(annotation.default_style.as_deref(), Some("ArialMT"));
    assert!(
        !annotation.has_default_style_language_escape(),
        "名字形态不携带转义"
    );
    assert_eq!(
        annotation.default_style_semantic_text().unwrap(),
        None,
        "名字形态没有语言视图，也不报错"
    );
    assert_eq!(
        annotation.default_style_text().unwrap().as_deref(),
        Some("ArialMT"),
        "名字形态的自然文本即名字本身"
    );
    assert_eq!(
        annotation
            .default_style_language(Some(&identifier("en")))
            .unwrap(),
        Some(identifier("en")),
        "名字永不覆盖 prevailing Lang"
    );
    assert_eq!(annotation.default_style_language(None).unwrap(), None);

    // 判别走的是形态而非字符：名字里真有 U+001B 也不按转义处理、不报错。
    let escaped_name = format!("Arial{}", escape("ja"));
    let annotation = free_text_with_default_style(Value::Name(escaped_name.clone()));
    assert!(!annotation.has_default_style_language_escape());
    assert_eq!(annotation.default_style_semantic_text().unwrap(), None);
    assert_eq!(
        annotation.default_style_text().unwrap().as_deref(),
        Some(escaped_name.as_str()),
        "原样返回名字，不剥离任何字符"
    );
    assert_eq!(
        annotation
            .default_style_language(Some(&identifier("de")))
            .unwrap(),
        Some(identifier("de")),
        "名字形态下 prevailing 保持生效"
    );
}

#[test]
fn free_text_default_style_unterminated_escape_is_typed_malformed_error() {
    // 文本串形态的残缺转义不影响注解读取，只在索取 `/DS` 语言视图时返回
    // 类型化 Malformed（不 panic），错误串带 `annotation /DS` 上下文。
    let style = format!("broken {}", unterminated_escape("en"));
    let annotation = free_text_with_default_style(unicode_text(&style));
    assert!(annotation.has_default_style_language_escape());
    assert_eq!(
        annotation.default_style.as_deref(),
        Some(style.as_str()),
        "原串仍按解析结果保留"
    );
    assert_eq!(
        annotation.name_semantic_text().unwrap(),
        None,
        "无 /NM 不受影响"
    );

    for result in [
        annotation.default_style_semantic_text().map(|_| ()),
        annotation.default_style_text().map(|_| ()),
        annotation.default_style_language(None).map(|_| ()),
    ] {
        let error = result.expect_err("unterminated escape shall be rejected");
        assert!(
            matches!(error, LanguageError::Malformed(_)),
            "残缺转义应是类型化 Malformed，got {error:?}"
        );
        assert!(error.to_string().contains("annotation /DS"), "{error}");
        assert!(error.to_string().contains("not terminated"), "{error}");
    }

    // 码长非法同样只报错、不 panic。
    let error = free_text_with_default_style(unicode_text(&format!("{}x", escape("eng"))))
        .default_style_text()
        .expect_err("three-byte codes are not a valid escape");
    assert!(error.to_string().contains("annotation /DS"), "{error}");
    assert!(error.to_string().contains("2-byte ISO 639"), "{error}");
}
