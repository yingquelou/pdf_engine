//! 书签标题的语言转义读端测试（ISO 32000-1 12.3.3 Table 153、14.9.3、
//! 7.9.2.2）。
//!
//! Table 153（raw L20578）把 `/Title` 定为**必填文本串**，其值是「显示在
//! 屏幕上的文本」；7.9.2.2（raw L4242–L4248）允许任意 Unicode 文本串内带
//! U+001B 定界的语言转义；14.9.2.1（raw L31940–L31941）与 14.9.3 NOTE 3
//! （raw L32117–L32120）要求「串内转义覆盖 prevailing Lang」。
//! 这里钉住 `OutlineItem` 的口径：`title` 保留解码原串、`title_text()` 给
//! 去转义的显示文本、`title_language(prevailing)` 给生效语言。
//!
//! 7.9.2.2 a) 把转义写成「the byte sequence 0 followed by 27」，即 UTF-16BE
//! 的 `00 1B`，故夹具一律用 `PdfTextString` 编出的 UTF-16BE 十六进制串；
//! PDFDocEncoding 侧另有一条测试钉住「字面串里的 0x1B 解不出 U+001B、因而
//! 不构成转义」。

use crate::core::reader::accessibility::{LANGUAGE_ESCAPE, LanguageIdentifier};
use crate::core::types::{Array, Dict, Document, Object, PdfTextString, Trailer, Value};

use super::{Destination, NavigationError, OutlineDestination};

const PAGES_OBJECT: i64 = 2;
const PAGE_BASE: i64 = 10;
const OUTLINE_ROOT: i64 = 60;
const FIRST_ITEM: i64 = 61;

fn arr(values: Vec<Value>) -> Value {
    Value::Array(Array(values))
}

fn name(value: &str) -> Value {
    Value::Name(value.to_owned())
}

fn insert(doc: &Document, num: i64, value: Value) {
    doc.set_object(Object::new(num, 0, value));
}

/// 7.9.2.2 a)–d) 的一个完整语言转义序列（U+001B + 码 + U+001B）。
fn escape(code: &str) -> String {
    format!("{LANGUAGE_ESCAPE}{code}{LANGUAGE_ESCAPE}")
}

/// 只有开界符、没有闭界符的残缺转义。
fn unterminated_escape(code: &str) -> String {
    format!("{LANGUAGE_ESCAPE}{code}")
}

/// 按 `PdfTextString` 编码：含 U+001B 等非 PDFDocEncoding 字符时整体退到
/// UTF-16BE + BOM，再写成十六进制文本串。
fn hex_of(text: &str) -> String {
    PdfTextString::from(text)
        .encode()
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect()
}

/// 单页、目录带 `/Lang (en)` 与 `/Outlines /First` 的文档。
fn outline_document() -> Document {
    let mut doc = Document::new();
    insert(
        &doc,
        PAGE_BASE,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Page"))
                .insert("Parent", Value::Ref(PAGES_OBJECT, 0)),
        ),
    );
    insert(
        &doc,
        PAGES_OBJECT,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Pages"))
                .insert("Kids", arr(vec![Value::Ref(PAGE_BASE, 0)]))
                .insert("Count", Value::Integer(1)),
        ),
    );
    insert(
        &doc,
        OUTLINE_ROOT,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Outlines"))
                .insert("First", Value::Ref(FIRST_ITEM, 0)),
        ),
    );
    let catalog = Dict::new()
        .insert("Type", name("Catalog"))
        .insert("Pages", Value::Ref(PAGES_OBJECT, 0))
        .insert("Lang", Value::String("en".into()))
        .insert("Outlines", Value::Ref(OUTLINE_ROOT, 0));
    doc.set_object(Object::new(1, 0, Value::Dict(catalog)));
    doc.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(1, 0)),
    };
    doc
}

/// 一条大纲条目字典：`/Title` 必填，`next`/`first_child` 按需链接。
fn item_dict(title: Value, next: Option<i64>, first_child: Option<i64>) -> Dict {
    let mut dict = Dict::new()
        .insert("Title", title)
        .insert("Parent", Value::Ref(OUTLINE_ROOT, 0));
    if let Some(next) = next {
        dict = dict.insert("Next", Value::Ref(next, 0));
    }
    if let Some(child) = first_child {
        dict = dict
            .insert("First", Value::Ref(child, 0))
            .insert("Last", Value::Ref(child, 0))
            .insert("Count", Value::Integer(1));
    }
    dict
}

/// 单条目大纲：`/Title` 以 UTF-16BE 十六进制文本串写入（转义可达的形态）。
fn outline_document_with_title(title: &str) -> Document {
    let doc = outline_document();
    insert(
        &doc,
        FIRST_ITEM,
        Value::Dict(item_dict(Value::HexString(hex_of(title)), None, None)),
    );
    doc
}

/// 目录 `/Lang` 即 prevailing Lang（14.9.2.1）。
fn catalog_lang(doc: &Document) -> Option<LanguageIdentifier> {
    doc.document_language().expect("/Lang (en) is readable")
}

/// `title_language` 的结论写成可读的标签串。
fn language_tag(language: Option<&LanguageIdentifier>) -> Option<&str> {
    language.map(LanguageIdentifier::as_str)
}

#[test]
fn outline_title_with_language_escape_overrides_prevailing_lang() {
    let title = format!("{}第一章", escape("ja"));
    let doc = outline_document_with_title(&title);

    let outline = doc.outline().unwrap().expect("outline");
    let first = &outline.items[0];
    assert!(first.has_title_language_escape());
    // `title` 保留解码原串：转义声明的语言是显示文本里没有的信息。
    assert_eq!(first.title, title);

    let prevailing = catalog_lang(&doc);
    assert_eq!(language_tag(prevailing.as_ref()), Some("en"));
    let language = first
        .title_language(prevailing.as_ref())
        .unwrap()
        .expect("escape declares a language");
    assert_eq!(
        language,
        LanguageIdentifier::parse("ja").expect("valid tag")
    );
    assert_eq!(first.title_text().unwrap(), "第一章");
}

#[test]
fn outline_title_plain_text_strips_escape_sequences() {
    // 转义落在串中：转义本身不进显示文本（Table 153 的「the text that shall
    // be displayed on the screen」）。
    let title = format!("Preface {}Vorwort", escape("de"));
    let doc = outline_document_with_title(&title);

    let outline = doc.outline().unwrap().expect("outline");
    let first = &outline.items[0];
    assert_eq!(first.title, title);
    assert_eq!(first.title_text().unwrap(), "Preface Vorwort");
    assert_eq!(first.title_semantic_text().unwrap().escape_count(), 1);
}

#[test]
fn outline_title_segments_split_at_each_escape() {
    let title = format!("{}Chapter {}Chapitre", escape("en"), escape("fr"));
    let doc = outline_document_with_title(&title);

    let outline = doc.outline().unwrap().expect("outline");
    let text = outline.items[0]
        .title_semantic_text()
        .expect("both escapes terminate");
    assert_eq!(text.escape_count(), 2);
    assert!(text.has_language_escape());
    let segments = text.segments();
    assert_eq!(segments.len(), 2);
    assert_eq!(language_tag(segments[0].language()), Some("en"));
    assert_eq!(segments[0].text(), "Chapter ");
    assert_eq!(language_tag(segments[1].language()), Some("fr"));
    assert_eq!(segments[1].text(), "Chapitre");
    assert_eq!(text.plain_text(), "Chapter Chapitre");
    // 整串语言取首个转义（14.9.3 的 shall）；逐段结论以 `segments` 为准。
    assert_eq!(
        language_tag(
            text.effective_language(catalog_lang(&doc).as_ref())
                .as_ref()
        ),
        Some("en")
    );
}

#[test]
fn outline_title_segment_before_first_escape_follows_prevailing_lang() {
    // 7.9.2.2 只约束「subsequent text」，转义之前的段没有转义语言。
    let title = format!("Vorwort {}Ende", escape("de"));
    let doc = outline_document_with_title(&title);

    let outline = doc.outline().unwrap().expect("outline");
    let text = outline.items[0]
        .title_semantic_text()
        .expect("escape terminates");
    assert_eq!(text.segments().len(), 2);
    assert_eq!(text.segments()[0].language(), None);
    assert_eq!(text.segments()[0].text(), "Vorwort ");
    assert_eq!(
        text.segments()[1]
            .language()
            .map(LanguageIdentifier::as_str),
        Some("de")
    );
    assert_eq!(text.segments()[1].text(), "Ende");
    assert_eq!(text.plain_text(), "Vorwort Ende");
}

#[test]
fn outline_title_without_escape_keeps_prevailing_lang() {
    let doc = outline_document();
    insert(
        &doc,
        FIRST_ITEM,
        Value::Dict(item_dict(Value::String("Chapter 1".into()), None, None)),
    );

    let outline = doc.outline().unwrap().expect("outline");
    let first = &outline.items[0];
    assert!(!first.has_title_language_escape());
    assert_eq!(first.title_text().unwrap(), first.title);
    let prevailing = catalog_lang(&doc);
    assert_eq!(
        first.title_language(prevailing.as_ref()).unwrap(),
        prevailing
    );
    // 文档没声明 /Lang 时不猜语言。
    assert_eq!(first.title_language(None).unwrap(), None);
}

#[test]
fn outline_title_literal_pdfdocencoding_string_carries_no_language_escape() {
    let doc = outline_document();
    // 字面串里的 0x1B 按 PDFDocEncoding 解为 U+02D9，不是 U+001B，故不
    // 构成转义： prevailing Lang 不被覆盖、也无转义可剥离。
    let raw = format!("{}Chapter", escape("ja"));
    insert(
        &doc,
        FIRST_ITEM,
        Value::Dict(item_dict(Value::String(raw), None, None)),
    );

    let outline = doc.outline().unwrap().expect("outline");
    let first = &outline.items[0];
    assert!(!first.has_title_language_escape());
    assert_eq!(first.title, "\u{02D9}ja\u{02D9}Chapter");
    assert_eq!(first.title_text().unwrap(), first.title);
    assert_eq!(
        first.title_language(catalog_lang(&doc).as_ref()).unwrap(),
        catalog_lang(&doc)
    );
}

#[test]
fn outline_title_with_iso_3166_country_code_escape_resolves_regional_tag() {
    // 7.9.2.2 c) 的可选 2 字节 ISO 3166 码拼为 `de-AT`。
    let title = format!("{}Einleitung", escape("deAT"));
    let doc = outline_document_with_title(&title);

    let outline = doc.outline().unwrap().expect("outline");
    let first = &outline.items[0];
    assert_eq!(first.title_text().unwrap(), "Einleitung");
    assert_eq!(
        language_tag(first.title_language(None).unwrap().as_ref()),
        Some("de-AT")
    );
}

#[test]
fn outline_title_with_unterminated_escape_returns_malformed_error() {
    // 残缺转义不毁掉整棵树的读取，但索取语言结论时必须报错（库策略：不
    // 静默把残缺转义当普通文本）。
    let title = format!("broken {}", unterminated_escape("en"));
    let doc = outline_document_with_title(&title);

    let outline = doc.outline().unwrap().expect("outline still readable");
    let first = &outline.items[0];
    assert!(first.has_title_language_escape());
    // `title` 原样保留，残缺只影响语义文本视图。
    assert_eq!(first.title, title);
    for result in [
        first.title_semantic_text().map(|_| ()),
        first.title_text().map(|_| ()),
        first.title_language(None).map(|_| ()),
    ] {
        let error = result.expect_err("unterminated escape shall be rejected");
        let NavigationError::Malformed(message) = error else {
            panic!("expected a malformed error, got {error:?}");
        };
        assert!(message.contains("outline /Title"), "{message}");
        assert!(message.contains("not terminated"), "{message}");
    }
}

#[test]
fn outline_title_with_non_alphabetic_escape_code_returns_malformed_error() {
    let title = format!("{}x", escape("e1"));
    let doc = outline_document_with_title(&title);

    let outline = doc.outline().unwrap().expect("outline still readable");
    let error = outline.items[0]
        .title_text()
        .expect_err("digits are no ISO 639 code");
    assert!(
        error.to_string().contains("shall be ASCII letters"),
        "{error}"
    );
}

#[test]
fn outline_title_with_wrong_length_escape_code_returns_malformed_error() {
    let title = format!("{}x", escape("eng"));
    let doc = outline_document_with_title(&title);

    let outline = doc.outline().unwrap().expect("outline still readable");
    let error = outline.items[0]
        .title_language(None)
        .expect_err("3-byte code is neither 2 nor 2+2");
    assert!(error.to_string().contains("2-byte ISO 639"), "{error}");
}

#[test]
fn outline_reads_nested_items_with_escaped_and_plain_titles() {
    let doc = outline_document();
    insert(
        &doc,
        FIRST_ITEM,
        Value::Dict(
            item_dict(
                Value::HexString(hex_of(&format!("{}第一章", escape("ja")))),
                Some(62),
                Some(63),
            )
            .insert("Dest", arr(vec![Value::Ref(PAGE_BASE, 0), name("Fit")])),
        ),
    );
    insert(
        &doc,
        62,
        Value::Dict(
            item_dict(Value::String("Chapter 2".into()), None, None)
                .insert("Prev", Value::Ref(FIRST_ITEM, 0))
                .insert("Dest", name("Chapter2")),
        ),
    );
    insert(
        &doc,
        63,
        Value::Dict(item_dict(Value::String("Section".into()), None, None)),
    );

    let outline = doc.outline().unwrap().expect("outline");
    assert_eq!(outline.items.len(), 2);
    let first = &outline.items[0];
    assert_eq!(first.children.len(), 1);
    assert_eq!(first.title_text().unwrap(), "第一章");
    assert_eq!(
        language_tag(
            first
                .title_language(catalog_lang(&doc).as_ref())
                .unwrap()
                .as_ref()
        ),
        Some("ja")
    );
    assert_eq!(
        first.destination,
        Some(OutlineDestination::Explicit(Destination::Fit { page: 0 }))
    );
    // 子条目自身没有转义，prevailing Lang 继续生效。
    let child = &first.children[0];
    assert!(!child.has_title_language_escape());
    assert_eq!(child.title_text().unwrap(), "Section");
    assert_eq!(
        language_tag(
            child
                .title_language(catalog_lang(&doc).as_ref())
                .unwrap()
                .as_ref()
        ),
        Some("en")
    );
    let second = &outline.items[1];
    assert_eq!(second.title_text().unwrap(), "Chapter 2");
    assert_eq!(
        second.destination,
        Some(OutlineDestination::Named("Chapter2".into()))
    );
}
