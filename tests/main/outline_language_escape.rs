use super::*;

use pdf_engine::core::builder::{LanguageIdentifier, OutlineEntry, PageSize, PdfBuilder};
use pdf_engine::core::reader::accessibility::SemanticText;
use pdf_engine::core::reader::navigation::{Destination, OutlineDestination};

/// 7.9.2.2 a)–d) 的完整语言转义序列（U+001B + 码 + U+001B）。
fn escape(code: &str) -> String {
    format!("\u{001B}{code}\u{001B}")
}

/// 只有开界符、没有闭界符的残缺转义。
fn unterminated_escape(code: &str) -> String {
    format!("\u{001B}{code}")
}

/// 写端大纲条目：给标题、目的地与子项，其余取默认。
fn entry(title: String, page: usize, children: Vec<OutlineEntry>) -> OutlineEntry {
    OutlineEntry {
        title,
        destination: Some(OutlineDestination::Explicit(Destination::Fit { page })),
        action: None,
        color: None,
        flags: 0,
        expanded: true,
        children,
    }
}

/// 写出带 7.9.2.2 语言转义的书签标题 → 重新解析。
///
/// U+001B 在 PDFDocEncoding 无表示，故 `PdfTextString` 整体退到 UTF-16BE +
/// BOM（7.9.2.2 a) 的「the byte sequence 0 followed by 27」）。解析回来的
/// `OutlineItem::title` 保留原串、`title_text()` 给去转义的显示文本、
/// `title_language()` 按 14.9.3（raw L32117–L32120）与 14.9.2.1（raw
/// L31940–L31941）的 shall 覆盖目录 `/Lang`；无转义的兄弟条目仍沿用
/// `/Lang`。
#[test]
fn builder_outline_title_language_escape_roundtrips_through_parser() {
    let escaped_title = format!("{}第一章", escape("ja"));
    let plain_title = "Chapter 2".to_owned();

    let mut builder = PdfBuilder::new();
    builder.page(PageSize::A4);
    builder.page(PageSize::A4);
    builder
        .set_document_language(LanguageIdentifier::parse("en").expect("valid tag"))
        .outline(&[
            entry(escaped_title.clone(), 0, Vec::new()),
            entry(plain_title.clone(), 1, Vec::new()),
        ]);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let doc = parse_buf(&buf);

    let prevailing = doc.document_language().expect("/Lang readable");
    assert_eq!(
        prevailing.as_ref().map(LanguageIdentifier::as_str),
        Some("en")
    );

    let outline = doc.outline().unwrap().expect("outline");
    assert_eq!(outline.items.len(), 2);

    let escaped = &outline.items[0];
    assert_eq!(escaped.title, escaped_title);
    assert!(escaped.has_title_language_escape());
    assert_eq!(escaped.title_text().expect("valid escape"), "第一章");
    assert_eq!(
        escaped
            .title_language(prevailing.as_ref())
            .unwrap()
            .as_ref()
            .map(LanguageIdentifier::as_str),
        Some("ja"),
        "串内转义 shall 覆盖 prevailing Lang（14.9.3）"
    );
    assert_eq!(
        escaped.destination,
        Some(OutlineDestination::Explicit(Destination::Fit { page: 0 }))
    );

    let plain = &outline.items[1];
    assert_eq!(plain.title, plain_title);
    assert!(!plain.has_title_language_escape());
    assert_eq!(plain.title_text().expect("no escape"), plain_title);
    assert_eq!(
        plain.title_language(prevailing.as_ref()).unwrap(),
        prevailing,
        "无转义时沿用 prevailing Lang"
    );

    // 写出的 `/Title` 确实是十六进制文本串：字典级路径
    // `SemanticText::from_value` 与条目访问器给出同一个结论。
    let title_value = (3..=64)
        .filter_map(|num| doc.get_object(num).ok().flatten())
        .filter_map(|object| object.value.as_dict().cloned())
        .filter(|dict| dict.get("Dest").is_some())
        .find_map(|dict| dict.get("Title").cloned())
        .expect("written outline item carries /Title");
    assert!(
        matches!(title_value, Value::HexString(_)),
        "带转义的标题应按 UTF-16BE 十六进制写出: {title_value:?}"
    );
    let text = SemanticText::from_value(&title_value).expect("written title parses");
    assert_eq!(text.plain_text(), "第一章");
    assert_eq!(
        text.escape_language().map(LanguageIdentifier::as_str),
        Some("ja")
    );
}

/// 写入 → 重新解析：多层书签里的多转义标题逐段往返——父条目串内两次换
/// 语言、子条目用 ISO 3166 国家码；`segments()` 的逐段语言与 `escape_count`
/// 在写出/重解析后保持一致。
#[test]
fn builder_outline_nested_multilingual_title_escapes_roundtrip_through_parser() {
    let parent_title = format!("{}Overview {}Überblick", escape("en"), escape("de"));
    let child_title = format!("{}Übersicht", escape("deAT"));

    let mut builder = PdfBuilder::new();
    builder.page(PageSize::A4);
    builder.set_document_language(LanguageIdentifier::parse("en").expect("valid tag"));
    builder.outline(&[entry(
        parent_title.clone(),
        0,
        vec![entry(child_title.clone(), 0, Vec::new())],
    )]);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let doc = parse_buf(&buf);
    let prevailing = doc.document_language().expect("/Lang readable");

    let outline = doc.outline().unwrap().expect("outline");
    let parent = &outline.items[0];
    assert_eq!(parent.title, parent_title);
    let parent_text = parent.title_semantic_text().expect("two escapes");
    assert_eq!(parent_text.escape_count(), 2);
    assert_eq!(parent_text.plain_text(), "Overview Überblick");
    assert_eq!(parent_text.segments().len(), 2);
    assert_eq!(
        parent_text.segments()[0]
            .language()
            .map(LanguageIdentifier::as_str),
        Some("en")
    );
    assert_eq!(
        parent_text.segments()[1]
            .language()
            .map(LanguageIdentifier::as_str),
        Some("de")
    );
    assert_eq!(
        parent
            .title_language(prevailing.as_ref())
            .unwrap()
            .as_ref()
            .map(LanguageIdentifier::as_str),
        Some("en")
    );

    let child = &parent.children[0];
    assert_eq!(child.title, child_title);
    assert_eq!(child.title_text().expect("valid escape"), "Übersicht");
    assert_eq!(
        child
            .title_language(prevailing.as_ref())
            .unwrap()
            .as_ref()
            .map(LanguageIdentifier::as_str),
        Some("de-AT"),
        "ISO 3166 国家码拼为区域标签（7.9.2.2 c）"
    );
}

/// 写入 → 重新解析：残缺（未闭合）的语言转义不破坏大纲读取——标题仍按
/// 原串往返、目的地仍可解析；只有索取自然文本或语言结论时才返回类型化
/// `Malformed`（库策略：报错而非静默把残缺转义当文本）。
#[test]
fn builder_outline_unterminated_title_escape_roundtrip_reports_malformed_on_request() {
    let broken_title = format!("{}Chapter", unterminated_escape("en"));

    let mut builder = PdfBuilder::new();
    builder.page(PageSize::A4);
    builder.outline(&[entry(broken_title.clone(), 0, Vec::new())]);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let doc = parse_buf(&buf);

    let outline = doc.outline().unwrap().expect("outline stays readable");
    let first = &outline.items[0];
    assert_eq!(first.title, broken_title);
    assert!(first.has_title_language_escape());
    assert_eq!(
        first.destination,
        Some(OutlineDestination::Explicit(Destination::Fit { page: 0 }))
    );

    let error = first
        .title_text()
        .expect_err("unterminated escape shall be rejected");
    assert!(error.to_string().contains("outline /Title"), "{error}");
    assert!(error.to_string().contains("not terminated"), "{error}");
    let error = first
        .title_language(None)
        .expect_err("language conclusion needs a valid escape");
    assert!(error.to_string().contains("not terminated"), "{error}");

    // 同一份原串经 `SemanticText::parse` 也判为残缺，读端口径一致。
    let error = SemanticText::parse(&broken_title).expect_err("same policy");
    assert!(
        error.to_string().contains("is not terminated by U+001B"),
        "{error}"
    );
}
