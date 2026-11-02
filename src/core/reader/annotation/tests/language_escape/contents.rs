use super::*;

/// Table 164（raw L21272–L21277）把 `/Contents` 定为 text string：注解显示
/// 的文本，或不显示文本时的替代描述。`Annotation::contents` 保留**不去转义**
/// 的原串，逐段视图由本模块的访问器按需解析。
#[test]
fn annotation_contents_with_language_escape_returns_plain_text_and_declared_language() {
    let text = format!("{}付箋の内容", escape("ja"));
    let annotation = annotation_with_contents(unicode_text(&text));

    assert_eq!(annotation.contents, Some(unicode_text(&text)), "原串保留");
    assert!(annotation.has_contents_language_escape());
    assert_eq!(
        annotation
            .contents_semantic_text()
            .unwrap()
            .expect("escaped /Contents")
            .escape_count(),
        1
    );
    assert_eq!(
        annotation
            .contents_text()
            .unwrap()
            .as_deref()
            .expect("/Contents 视图"),
        "付箋の内容"
    );

    let prevailing = identifier("en");
    assert_eq!(
        language_tag(
            annotation
                .contents_language(Some(&prevailing))
                .unwrap()
                .as_ref()
        ),
        Some("ja"),
        "串内转义 shall 覆盖 prevailing Lang（14.9.2.1 raw L31940–L31941 与 14.9.3 NOTE 3）"
    );
    // 只读加法：其余条目视图互不影响。
    assert_eq!(annotation.title_semantic_text().unwrap(), None);
    assert_eq!(annotation.modified_text_semantic_text().unwrap(), None);
}

/// 「逐段」指 7.9.2.2 转义切出的语言段，不是 12.5.6.2 L21651–L21652 的回车
/// 段落：段内文本原样保留回车，视图不做段落切分。
#[test]
fn annotation_contents_multilingual_escapes_expose_per_segment_language_and_keep_paragraph_breaks()
{
    let text = format!(
        "{}First paragraph.\rSecond line.{}Zweiter Absatz.",
        escape("en"),
        escape("deCH")
    );
    let annotation = annotation_with_contents(unicode_text(&text));

    let view = annotation
        .contents_semantic_text()
        .unwrap()
        .expect("two escapes");
    assert_eq!(view.escape_count(), 2);
    assert_eq!(view.segments().len(), 2);
    assert_eq!(segments_language_tag(&view, 0), Some("en"));
    assert_eq!(
        view.segments()[0].text(),
        "First paragraph.\rSecond line.",
        "回车按原样保留，不按段落再切一段"
    );
    assert_eq!(segments_language_tag(&view, 1), Some("de-CH"));
    assert_eq!(
        annotation
            .contents_text()
            .unwrap()
            .as_deref()
            .expect("/Contents 视图"),
        "First paragraph.\rSecond line.Zweiter Absatz."
    );
    assert_eq!(
        annotation
            .contents_language(Some(&identifier("fr")))
            .unwrap(),
        Some(identifier("en")),
        "首个转义决定整串生效语言"
    );
}

#[test]
fn annotation_contents_without_language_escape_keeps_original_string_and_prevailing_lang() {
    let annotation = annotation_with_contents(Value::String("A plain note".into()));
    assert!(!annotation.has_contents_language_escape());
    let view = annotation
        .contents_semantic_text()
        .unwrap()
        .expect("plain /Contents still yields a view");
    assert_eq!(view.escape_count(), 0);
    assert_eq!(view.segments().len(), 1);
    assert_eq!(segments_language_tag(&view, 0), None);
    assert_eq!(
        annotation
            .contents_text()
            .unwrap()
            .as_deref()
            .expect("/Contents 视图"),
        "A plain note",
        "无转义时自然文本即原串"
    );
    assert_eq!(
        annotation
            .contents_language(Some(&identifier("en-US")))
            .unwrap(),
        Some(identifier("en-US")),
        "无转义时原样返回 prevailing"
    );
    assert_eq!(annotation.contents_language(None).unwrap(), None);
}

#[test]
fn annotation_without_contents_has_no_language_view() {
    let annotation = annotation_without_contents_or_modified_text();
    assert_eq!(annotation.contents, None);
    assert!(!annotation.has_contents_language_escape());
    assert_eq!(
        annotation.contents_semantic_text().unwrap(),
        None,
        "缺 /Contents 返回 Ok(None)"
    );
    assert_eq!(annotation.contents_text().unwrap(), None);
    assert_eq!(
        annotation
            .contents_language(Some(&identifier("en")))
            .unwrap(),
        None,
        "没有 /Contents 就无所谓语言"
    );
}

#[test]
fn annotation_contents_unterminated_escape_is_typed_malformed_error() {
    let broken = format!("broken {}", unterminated_escape("en"));
    let annotation = annotation_with_contents(unicode_text(&broken));
    assert!(annotation.has_contents_language_escape());
    assert_eq!(
        annotation.contents,
        Some(unicode_text(&broken)),
        "原串仍按解析结果保留，读取注解不失败"
    );

    for result in [
        annotation.contents_semantic_text().map(|_| ()),
        annotation.contents_text().map(|_| ()),
        annotation.contents_language(None).map(|_| ()),
    ] {
        let error = result.expect_err("unterminated escape shall be rejected");
        assert!(
            matches!(error, LanguageError::Malformed(_)),
            "残缺转义应是类型化 Malformed，got {error:?}"
        );
        assert!(
            error.to_string().contains("annotation /Contents"),
            "{error}"
        );
        assert!(error.to_string().contains("not terminated"), "{error}");
    }

    // 码字节非 ASCII 字母：同样只在索取视图时报错。
    let error = annotation_with_contents(unicode_text(&format!("x{}x", escape("e1"))))
        .contents_semantic_text()
        .expect_err("digits are not ISO 639 codes");
    assert!(
        error.to_string().contains("annotation /Contents"),
        "{error}"
    );
    assert!(error.to_string().contains("ASCII letters"), "{error}");
}

/// 分工：14.9.3 的来源判定（带 `/Parent` 的 `Popup` 其 `/Contents` 应被忽略，
/// 12.5.6.2 NOTE 2）只在 `alternate_description` 落实；本视图是**条目级**
/// 视图，仍按字典原值展开语言段。
#[test]
fn annotation_contents_view_of_child_popup_stays_available_while_alternate_description_is_ignored()
{
    let document = doc(vec![catalog_object()]);
    let text = format!("{}Note en réponse", escape("fr"));
    let annotation = parse_annotation(
        &document,
        0,
        None,
        &base_annotation("Popup")
            .insert("Parent", Value::Ref(4, 0))
            .insert("Contents", unicode_text(&text)),
    )
    .expect("child popup parses");
    assert_eq!(annotation.subtype, AnnotationSubtype::Popup);
    assert!(annotation.parent.is_some());

    let view = annotation
        .contents_semantic_text()
        .unwrap()
        .expect("条目级视图不受来源判定影响");
    assert_eq!(view.escape_count(), 1);
    assert_eq!(
        annotation
            .contents_text()
            .unwrap()
            .as_deref()
            .expect("/Contents 视图"),
        "Note en réponse"
    );

    // 14.9.3 通道按 NOTE 2 忽略带父 Popup 的内容文本。
    assert_eq!(
        annotation
            .alternate_description(Some(&identifier("en")))
            .unwrap(),
        None,
        "来源判定仍由 alternate_description 负责"
    );
}
