use super::*;

/// ISO 32000-1 的注解字典没有 `/TM` 键（Table 164 raw L21262–L21349 与
/// Table 170 raw L21664–L21751 均无），解析器因此不建模它、值原样留在
/// `Annotation::dict`。本视图按 Acrobat/ISO 32000-2 的「修改后文本」形态
/// （text string，PDF 1.4）回查该未建模条目，转义规则走 14.9.2.1
/// （raw L31940–L31941）对一切 Unicode 文本串的 shall。
#[test]
fn annotation_modified_text_with_language_escape_returns_plain_text_and_declared_language() {
    let text = format!("{}修正済み", escape("ja"));
    let annotation = annotation_with_modified_text(unicode_text(&text));

    // 只读：解析路径未动，原值仍在字典里，`Annotation` 也没有 /TM 字段。
    assert_eq!(annotation.dict.get("TM"), Some(&unicode_text(&text)));
    assert!(annotation.has_modified_text_language_escape());
    assert_eq!(
        annotation
            .modified_text_semantic_text()
            .unwrap()
            .expect("escaped /TM")
            .escape_count(),
        1
    );
    assert_eq!(
        annotation
            .modified_text_text()
            .unwrap()
            .as_deref()
            .expect("/TM 视图"),
        "修正済み"
    );

    let prevailing = identifier("en");
    assert_eq!(
        language_tag(
            annotation
                .modified_text_language(Some(&prevailing))
                .unwrap()
                .as_ref()
        ),
        Some("ja"),
        "串内转义 shall 覆盖 prevailing Lang（14.9.2.1 raw L31940–L31941）"
    );
    // 无关条目不受影响：本条注解既没有 /Contents 也没有 /NM。
    assert_eq!(annotation.contents, None);
    assert_eq!(annotation.name_semantic_text().unwrap(), None);
}

/// 两次换语言的 `/TM` 逐段视图：每段自己的转义语言，整串生效语言取首个转义。
#[test]
fn annotation_modified_text_with_two_language_escapes_exposes_per_segment_languages() {
    let text = format!("{}Fixed {}Geändert", escape("en"), escape("deAT"));
    let annotation = annotation_with_modified_text(unicode_text(&text));

    let view = annotation
        .modified_text_semantic_text()
        .unwrap()
        .expect("escaped /TM");
    assert_eq!(view.escape_count(), 2);
    assert_eq!(view.plain_text(), "Fixed Geändert");
    assert_eq!(view.segments().len(), 2);
    assert_eq!(segments_language_tag(&view, 0), Some("en"));
    assert_eq!(view.segments()[0].text(), "Fixed ");
    assert_eq!(
        segments_language_tag(&view, 1),
        Some("de-AT"),
        "ISO 3166 国家码拼为区域标签（7.9.2.2 c）"
    );
    assert_eq!(
        annotation
            .modified_text_language(Some(&identifier("fr")))
            .unwrap(),
        Some(identifier("en")),
        "首个转义决定整串生效语言"
    );
}

#[test]
fn annotation_modified_text_without_language_escape_keeps_original_string_and_prevailing_lang() {
    let annotation = annotation_with_modified_text(Value::String("Revised wording".into()));
    assert!(!annotation.has_modified_text_language_escape());
    let view = annotation
        .modified_text_semantic_text()
        .unwrap()
        .expect("plain /TM still yields a view");
    assert_eq!(view.escape_count(), 0);
    assert_eq!(view.segments().len(), 1);
    assert_eq!(segments_language_tag(&view, 0), None);
    assert_eq!(
        annotation
            .modified_text_text()
            .unwrap()
            .as_deref()
            .expect("/TM 视图"),
        "Revised wording",
        "无转义时自然文本即原串"
    );
    assert_eq!(
        annotation
            .modified_text_language(Some(&identifier("en-US")))
            .unwrap(),
        Some(identifier("en-US")),
        "无转义时原样返回 prevailing"
    );
    assert_eq!(annotation.modified_text_language(None).unwrap(), None);
}

#[test]
fn annotation_without_modified_text_has_no_language_view() {
    let annotation = annotation_without_contents_or_modified_text();
    assert_eq!(annotation.dict.get("TM"), None);
    assert!(!annotation.has_modified_text_language_escape());
    assert_eq!(
        annotation.modified_text_semantic_text().unwrap(),
        None,
        "缺 /TM 返回 Ok(None)"
    );
    assert_eq!(annotation.modified_text_text().unwrap(), None);
    assert_eq!(
        annotation
            .modified_text_language(Some(&identifier("en")))
            .unwrap(),
        None,
        "没有 /TM 就无所谓语言"
    );
}

/// 残缺转义不影响注解读取，只在索取语言视图时返回带 `annotation /TM` 上下文的
/// 类型化 Malformed（库策略：报错而非静默把残缺转义当文本）。
#[test]
fn annotation_modified_text_unterminated_escape_is_typed_malformed_error() {
    let broken = format!("broken {}", unterminated_escape("en"));
    let annotation = annotation_with_modified_text(unicode_text(&broken));
    assert!(annotation.has_modified_text_language_escape());
    assert_eq!(
        annotation.dict.get("TM"),
        Some(&unicode_text(&broken)),
        "原值按解析结果留在字典"
    );

    for result in [
        annotation.modified_text_semantic_text().map(|_| ()),
        annotation.modified_text_text().map(|_| ()),
        annotation.modified_text_language(None).map(|_| ()),
    ] {
        let error = result.expect_err("unterminated escape shall be rejected");
        assert!(
            matches!(error, LanguageError::Malformed(_)),
            "残缺转义应是类型化 Malformed，got {error:?}"
        );
        assert!(error.to_string().contains("annotation /TM"), "{error}");
        assert!(error.to_string().contains("not terminated"), "{error}");
    }

    // 码长非法同样只报错、不 panic。
    let error = annotation_with_modified_text(unicode_text(&format!("x{}", escape("eng"))))
        .modified_text_text()
        .expect_err("three-byte codes are not a valid escape");
    assert!(error.to_string().contains("annotation /TM"), "{error}");
    assert!(error.to_string().contains("2-byte ISO 639"), "{error}");
}

/// 形态判别走字典值形态：`/TM` 若是名字而非文本串，本视图按 `/NM` 那条口径
/// 返回 Malformed（`/TM` 没有 `/DS` 那样的名字宽容解析分支）。
#[test]
fn annotation_modified_text_with_name_value_is_typed_malformed_error() {
    let annotation = annotation_with_modified_text(Value::Name("fixed".into()));
    assert!(
        !annotation.has_modified_text_language_escape(),
        "名字里没有转义可判"
    );
    let error = annotation
        .modified_text_semantic_text()
        .expect_err("name is not a text string");
    assert!(matches!(error, LanguageError::Malformed(_)), "{error:?}");
    assert!(error.to_string().contains("annotation /TM"), "{error}");
    assert!(
        error
            .to_string()
            .contains("shall be a text string, found name"),
        "{error}"
    );
    assert!(
        annotation.modified_text_language(None).is_err(),
        "语言视图同口径报错"
    );
}

/// 间接引用形态的 `/TM` 与既有四条一致：本切片不扩解引用，报 Malformed。
#[test]
fn annotation_modified_text_with_indirect_reference_is_typed_malformed_error() {
    let annotation = annotation_with_modified_text(Value::Ref(9, 0));
    assert!(!annotation.has_modified_text_language_escape());
    let error = annotation
        .modified_text_text()
        .expect_err("dereferencing /TM is out of scope");
    assert!(error.to_string().contains("annotation /TM"), "{error}");
    assert!(error.to_string().contains("found ref"), "{error}");
}
