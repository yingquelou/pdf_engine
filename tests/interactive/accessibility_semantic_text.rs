//! 14.9.3–14.9.5 的语义落地：Unicode 语言转义（7.9.2.2）覆盖 prevailing
//! Lang、`/Alt` 与 `/ActualText` 的词边界判定、注解 `/Contents` 的来源
//! 优先级，全部走「写出 → 重新解析 → 回读」往返。

use super::*;

/// 写端写出带 Unicode 语言转义的 `/Alt` `/ActualText` `/E`（Span 属性表与
/// 结构元素字典）→ 重新解析 → 读端按 7.9.2.2 解析转义、按 14.9.3–14.9.5
/// 的 shall 让转义覆盖目录 `/Lang`，并给出去转义后的纯文本。
#[test]
fn builder_span_and_element_language_escapes_override_document_lang_through_parser() {
    use pdf_engine::core::builder::{
        Font, PageSize, PdfBuilder, SpanProperties, StructureElement, StructureTree,
    };
    use pdf_engine::core::reader::accessibility::{LanguageIdentifier, SemanticTextEntry};
    use pdf_engine::core::types::PdfVersion;

    const ESCAPE: char = '\u{1B}';
    let language = |value: &str| {
        LanguageIdentifier::parse(value).unwrap_or_else(|error| panic!("{value:?}: {error}"))
    };

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_5);
    builder.set_document_language(language("en-US"));
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 700.0, "See ");
    builder.begin_span(
        page,
        &SpanProperties::new()
            .with_alternate_text(format!("{ESCAPE}ja{ESCAPE}six-point star"))
            .with_actual_text(format!("plain{ESCAPE}fr{ESCAPE}étoile"))
            .with_expansion(format!("{ESCAPE}enus{ESCAPE}St.")),
    );
    builder.text(page, &Font::helvetica(12.0), 110.0, 700.0, "A");
    builder.end_span(page);

    builder.set_structure_tree(StructureTree::new().with_element(
        StructureElement::new("Document").with_element(
            StructureElement::new("Span").with_alt(format!("{ESCAPE}de{ESCAPE}Stern")),
        ),
    ));

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let doc = parse_buf(&buf);
    let prevailing = doc.document_language().expect("catalog /Lang");
    assert_eq!(prevailing, Some(language("en-US")));

    let sections = doc.page_marked_content(0).expect("marked content");
    assert_eq!(sections.len(), 1, "{sections:?}");
    let span = &sections[0];

    let alternate = span
        .span_semantic_text(SemanticTextEntry::AlternateText)
        .expect("span /Alt")
        .expect("alternate text present");
    assert_eq!(alternate.plain_text(), "six-point star");
    assert_eq!(alternate.escape_count(), 1);
    assert_eq!(
        alternate.effective_language(prevailing.as_ref()),
        Some(language("ja")),
        "14.9.3：/Alt 里的转义 shall 覆盖目录 /Lang"
    );

    let actual = span
        .span_semantic_text(SemanticTextEntry::ActualText)
        .expect("span /ActualText")
        .expect("actual text present");
    assert_eq!(actual.plain_text(), "plain\u{E9}toile");
    assert_eq!(actual.segments().len(), 2);
    assert_eq!(
        actual.segments()[0].language(),
        None,
        "转义之前的段没有转义语言，沿用 prevailing Lang"
    );
    assert_eq!(
        actual.effective_language(prevailing.as_ref()),
        Some(language("fr")),
        "14.9.4：ActualText 的转义同样 shall 覆盖 prevailing Lang"
    );

    let expansion = span
        .span_semantic_text(SemanticTextEntry::Expansion)
        .expect("span /E")
        .expect("expansion present");
    assert_eq!(expansion.plain_text(), "St.");
    assert_eq!(expansion.escape_language(), Some(&language("en-us")));

    // 既有类型化视图不变：原始解码串仍保留 U+001B，转义只在语义视图里剥离。
    let properties = span
        .span_properties()
        .expect("typed span properties")
        .expect("Span property list");
    assert!(
        properties
            .alternate_text
            .as_deref()
            .is_some_and(|text| text.contains(ESCAPE)),
        "{:?}",
        properties.alternate_text
    );
    assert_eq!(
        doc.page_marked_content_languages(0)
            .expect("section languages"),
        vec![Some(language("en-US"))],
        "转义不改变 14.9.2 的区段语言解析：属性表没有 /Lang 时回退目录语言"
    );

    let tree = doc
        .structure_tree()
        .expect("structure tree")
        .expect("tree present");
    let element_alt = tree
        .element_semantic_text(1, SemanticTextEntry::AlternateText)
        .expect("element /Alt")
        .expect("structure element alternate text");
    assert_eq!(element_alt.plain_text(), "Stern");
    assert_eq!(
        element_alt.effective_language(prevailing.as_ref()),
        Some(language("de")),
        "结构元素的 /Alt 同样按转义取语言"
    );
    assert_eq!(
        tree.element_semantic_text(0, SemanticTextEntry::AlternateText)
            .expect("root element has no /Alt"),
        None
    );
}

/// 写出两个都带 `/Alt` 的 Span 与两个都带 `/ActualText` 的 Span →
/// 重新解析 → 读端按 14.9.3 给出词边界、按 14.9.4 给出「无词边界」；
/// 并回读注解 `/Contents` 的 14.9.3 来源优先级与其中的语言转义。
#[test]
fn builder_word_break_pairs_and_annotation_description_source_roundtrip_through_parser() {
    use pdf_engine::core::builder::{
        AnnotationKind, Font, PageSize, PdfAnnotation, PdfBuilder, SpanProperties,
    };
    use pdf_engine::core::reader::accessibility::{
        AnnotationDescriptionSource, LanguageIdentifier, WordBreak, WordBreakRule,
    };
    use pdf_engine::core::reader::annotation::AnnotationSubtype;
    use pdf_engine::core::types::PdfVersion;

    const ESCAPE: char = '\u{1B}';
    let language = |value: &str| {
        LanguageIdentifier::parse(value).unwrap_or_else(|error| panic!("{value:?}: {error}"))
    };

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_5);
    builder.set_document_language(language("en-US"));
    let page = builder.page(PageSize::A4);
    for (index, alternate) in ["first star", "second star"].iter().enumerate() {
        builder.begin_span(page, &SpanProperties::new().with_alternate_text(*alternate));
        builder.text(
            page,
            &Font::helvetica(12.0),
            72.0,
            700.0 - index as f64 * 20.0,
            "A",
        );
        builder.end_span(page);
    }
    for (index, actual) in ["c", "k"].iter().enumerate() {
        builder.begin_span(page, &SpanProperties::new().with_actual_text(*actual));
        builder.text(
            page,
            &Font::helvetica(12.0),
            72.0,
            650.0 - index as f64 * 20.0,
            "k",
        );
        builder.end_span(page);
    }
    builder.add_annotation(
        page,
        PdfAnnotation::new(AnnotationKind::Text, [72.0, 700.0, 92.0, 720.0])
            .with_contents(format!("{ESCAPE}ja{ESCAPE}Review note")),
    );
    builder.add_annotation(
        page,
        PdfAnnotation::new(AnnotationKind::Link, [72.0, 600.0, 172.0, 620.0])
            .with_contents("human readable link target"),
    );

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let doc = parse_buf(&buf);
    let prevailing = doc.document_language().expect("catalog /Lang");

    let boundaries = doc
        .page_marked_content_word_breaks(0)
        .expect("word breaks between marked-content sequences");
    assert_eq!(boundaries.len(), 3, "{boundaries:?}");
    assert_eq!(boundaries[0].decision.word_break, WordBreak::Present);
    assert_eq!(
        boundaries[0].decision.rule,
        Some(WordBreakRule::AlternateTextPair),
        "14.9.3：连续两个都带 /Alt 的标记内容序列之间应有词边界"
    );
    assert_eq!(boundaries[1].decision.word_break, WordBreak::NotDeclared);
    assert_eq!(boundaries[2].decision.word_break, WordBreak::Absent);
    assert_eq!(
        boundaries[2].decision.rule,
        Some(WordBreakRule::ActualTextPair),
        "14.9.4：连续两个都带 /ActualText 的序列之间不应插入词边界"
    );

    let annotations = doc.page_annotations(0).expect("page annotations");
    let note = annotations
        .iter()
        .find(|annotation| annotation.subtype == AnnotationSubtype::Text)
        .expect("Text annotation");
    let description = note
        .alternate_description(prevailing.as_ref())
        .expect("alternate description")
        .expect("contents present");
    assert_eq!(
        description.source,
        AnnotationDescriptionSource::ContentsRequired
    );
    assert!(description.source.is_required());
    assert_eq!(description.plain_text(), "Review note");
    assert_eq!(
        description.language,
        Some(language("ja")),
        "14.9.3：/Contents 里的转义 shall 覆盖 prevailing Lang"
    );

    let link = annotations
        .iter()
        .find(|annotation| annotation.subtype == AnnotationSubtype::Link)
        .expect("Link annotation");
    let link_description = link
        .alternate_description(prevailing.as_ref())
        .expect("link description")
        .expect("contents present");
    assert_eq!(
        link_description.source,
        AnnotationDescriptionSource::ContentsOptional,
        "14.9.3：不显示文本的注解类型，/Contents 只是可选的替代描述"
    );
    assert_eq!(link_description.plain_text(), "human readable link target");
    assert_eq!(
        link_description.language,
        Some(language("en-US")),
        "没有转义时沿用目录 /Lang"
    );
}
