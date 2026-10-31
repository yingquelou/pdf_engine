//! 自然语言与多语言文本读端测试（ISO 32000-1 14.9.2）。

use std::collections::HashSet;

use crate::core::reader::accessibility::{
    AlternateText, ArtifactAttachment, ArtifactError, ArtifactProperties, ArtifactSubtype,
    ArtifactType, LanguageError, LanguageIdentifier, MultiLanguageTextArray, SpanProperties,
    first_declared_language,
};
use crate::core::reader::content::MarkedSection;
use crate::core::reader::rendition::{MediaClip, MediaClipDataSource};
use crate::core::types::{Array, Dict, Document, Object, StreamData, Trailer, Value};

use super::span::resolve_section_language;

const CATALOG: i64 = 1;
const ROOT: i64 = 2;
const PAGES: i64 = 3;
const PAGE: i64 = 4;
const ELEMENT: i64 = 5;

fn name(value: &str) -> Value {
    Value::Name(value.to_owned())
}

fn arr(values: Vec<Value>) -> Value {
    Value::Array(Array(values))
}

fn text(value: &str) -> Value {
    Value::String(value.to_owned())
}

fn insert(doc: &Document, num: i64, value: Value) {
    doc.set_object(Object::new(num, 0, value));
}

fn language(value: &str) -> LanguageIdentifier {
    LanguageIdentifier::parse(value).unwrap_or_else(|error| panic!("{value:?}: {error}"))
}

fn marked_section(tag: &str, properties: Option<Dict>, parent: Option<usize>) -> MarkedSection {
    MarkedSection {
        tag: tag.to_owned(),
        properties,
        visibility: None,
        start: 0,
        end: 0,
        point: false,
        parent,
    }
}

/// 多语言文本数组：按 `(identifier, text)` 顺序展开。
fn multi_language(pairs: &[(&str, &str)]) -> Value {
    let mut values = Vec::with_capacity(pairs.len() * 2);
    for (identifier, text_value) in pairs {
        values.push(text(identifier));
        values.push(text(text_value));
    }
    arr(values)
}

fn element(structure_type: &str, parent: i64) -> Dict {
    Dict::new()
        .insert("Type", name("StructElem"))
        .insert("S", name(structure_type))
        .insert("P", Value::Ref(parent, 0))
}

fn struct_root(entries: Dict) -> Dict {
    entries.insert("Type", name("StructTreeRoot"))
}

fn document_with_root(root: Dict) -> Document {
    let mut doc = Document::new();
    insert(&doc, ROOT, Value::Dict(root));
    insert(
        &doc,
        CATALOG,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Catalog"))
                .insert("StructTreeRoot", Value::Ref(ROOT, 0)),
        ),
    );
    doc.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(CATALOG, 0)),
    };
    doc
}

fn document_with_catalog(entries: Dict) -> Document {
    let mut doc = Document::new();
    insert(
        &doc,
        CATALOG,
        Value::Dict(entries.insert("Type", name("Catalog"))),
    );
    doc.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(CATALOG, 0)),
    };
    doc
}

/// 单页文档：目录 `/Lang`、结构树根、一个带 `/StructParents` 的页面。
fn structured_page_document(content: &[u8], root: Dict, element: Dict) -> Document {
    let mut doc = Document::new();
    insert(&doc, ROOT, Value::Dict(root));
    insert(&doc, ELEMENT, Value::Dict(element));
    insert(
        &doc,
        PAGES,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Pages"))
                .insert("Kids", arr(vec![Value::Ref(PAGE, 0)]))
                .insert("Count", Value::Integer(1)),
        ),
    );
    insert(
        &doc,
        PAGE,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Page"))
                .insert("Parent", Value::Ref(PAGES, 0))
                .insert("StructParents", Value::Integer(0))
                .insert(
                    "Contents",
                    Value::Stream(Dict::new(), StreamData::Owned(content.to_vec())),
                ),
        ),
    );
    insert(
        &doc,
        CATALOG,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Catalog"))
                .insert("Pages", Value::Ref(PAGES, 0))
                .insert("StructTreeRoot", Value::Ref(ROOT, 0))
                .insert("Lang", text("en-US")),
        ),
    );
    doc.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(CATALOG, 0)),
    };
    doc
}

fn assert_malformed<T: std::fmt::Debug>(result: Result<T, LanguageError>, needle: &str) {
    match result {
        Err(LanguageError::Malformed(message)) => {
            assert!(
                message.contains(needle),
                "expected {needle:?}, got {message}"
            );
        }
        other => panic!("expected malformed error containing {needle:?}, got {other:?}"),
    }
}

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

#[test]
fn multi_language_array_requires_pairs_and_unique_identifiers() {
    let parsed =
        MultiLanguageTextArray::parse(&multi_language(&[("", "default"), ("en", "English")]))
            .expect("pair array");
    assert_eq!(parsed.entries().len(), 2);
    assert_eq!(parsed.entries()[0].0.as_str(), "");
    assert_eq!(parsed.entries()[0].1, "default");
    assert_eq!(parsed.entries()[1].0.as_str(), "en");
    assert_eq!(parsed.default_text(), Some("default"));

    assert_malformed(
        MultiLanguageTextArray::parse(&arr(vec![text("en")])),
        "odd number",
    );
    assert_malformed(
        MultiLanguageTextArray::parse(&text("en-US")),
        "not an array",
    );
    assert_malformed(
        MultiLanguageTextArray::parse(&arr(vec![Value::Integer(1), text("x")])),
        "identifier 0",
    );
    assert_malformed(
        MultiLanguageTextArray::parse(&arr(vec![text("en"), Value::Integer(1)])),
        "text 0",
    );
    assert_malformed(
        MultiLanguageTextArray::parse(&multi_language(&[("en", "a"), ("EN", "b")])),
        "more than once",
    );
    assert_malformed(
        MultiLanguageTextArray::parse(&multi_language(&[("", "a"), ("", "b")])),
        "more than once",
    );

    // 4097 对：在解析标识符之前即触发条目上限。
    let mut values = Vec::with_capacity(4097 * 2);
    for _ in 0..=4096 {
        values.push(text(""));
        values.push(text("x"));
    }
    match MultiLanguageTextArray::parse(&arr(values)) {
        Err(LanguageError::LimitExceeded(_)) => {}
        other => panic!("expected entry limit, got {other:?}"),
    }
}

#[test]
fn multi_language_text_prefers_exact_then_prefix_then_default() {
    let array = MultiLanguageTextArray::parse(&multi_language(&[
        ("", "fallback"),
        ("en", "English"),
        ("en-US", "American"),
        ("en-GB", "British"),
        ("zh-Hant", "Traditional"),
    ]))
    .expect("multi-language array");

    assert_eq!(array.text_for(&language("en-US")), Some("American"));
    assert_eq!(array.text_for(&language("EN-us")), Some("American"));
    // 精确匹配优先于前缀匹配。
    assert_eq!(array.text_for(&language("en")), Some("English"));
    // 给定标识符更长时不反向匹配短条目。
    assert_eq!(array.text_for(&language("en-AU")), Some("fallback"));
    assert_eq!(array.text_for(&language("zh-Hant-TW")), Some("fallback"));
    // 前缀匹配：给定 zh 是数组标识符 zh-Hant 的前导且以 `-` 继续。
    assert_eq!(array.text_for(&language("zh")), Some("Traditional"));
    assert_eq!(
        array.text_for(&LanguageIdentifier::unknown()),
        Some("fallback")
    );
    assert_eq!(array.default_text(), Some("fallback"));

    let ordered = MultiLanguageTextArray::parse(&multi_language(&[
        ("en-US", "first"),
        ("en-Latn", "second"),
    ]))
    .expect("ordered array");
    assert_eq!(
        ordered.text_for(&language("en")),
        Some("first"),
        "前缀匹配按数组顺序"
    );
    assert_eq!(ordered.default_text(), None);

    let bare =
        MultiLanguageTextArray::parse(&multi_language(&[("en", "English")])).expect("single entry");
    assert_eq!(bare.text_for(&language("en-US")), None);
    assert_eq!(
        bare.text_for(&LanguageIdentifier::unknown()),
        None,
        "无默认条目时未知语言得到 None"
    );
}

#[test]
fn alternate_text_accepts_text_string_and_multi_language_array() {
    let plain = AlternateText::parse(&text("plain text")).expect("plain text");
    assert!(matches!(plain, AlternateText::Text(_)));
    assert_eq!(
        plain.text_for(&LanguageIdentifier::unknown()),
        Some("plain text"),
        "普通文本串与语言无关"
    );

    let multi = AlternateText::parse(&multi_language(&[
        ("", "default"),
        ("es-MX", "Spanish"),
        ("es-ES", "Castilian"),
    ]))
    .expect("multi-language alt");
    assert!(matches!(multi, AlternateText::MultiLanguage(_)));
    assert_eq!(multi.text_for(&language("es-MX")), Some("Spanish"));
    assert_eq!(multi.text_for(&language("es-ES")), Some("Castilian"));
    assert_eq!(multi.text_for(&language("fr")), Some("default"));

    let hex = AlternateText::parse(&Value::HexString("706C61696E".into())).expect("hex text");
    assert!(matches!(hex, AlternateText::Text(ref text) if text == "plain"));

    match AlternateText::parse(&Value::Integer(1)) {
        Err(LanguageError::Malformed(_)) => {}
        other => panic!("integer alt should be rejected, got {other:?}"),
    }
}

#[test]
fn document_language_reads_catalog_entry() {
    let doc = document_with_catalog(Dict::new().insert("Lang", text("en-US")));
    assert_eq!(
        doc.document_language().expect("catalog /Lang"),
        Some(language("en-US"))
    );

    let doc =
        document_with_catalog(Dict::new().insert("Lang", Value::HexString("656E2D5553".into())));
    assert_eq!(
        doc.document_language().expect("hex /Lang"),
        Some(language("en-US"))
    );

    let doc = document_with_catalog(Dict::new().insert("Lang", text("")));
    let unknown = doc
        .document_language()
        .expect("empty /Lang")
        .expect("declared");
    assert!(unknown.is_unknown());

    let doc = document_with_catalog(Dict::new());
    assert_eq!(doc.document_language().expect("missing /Lang"), None);

    // 间接引用只解一层。
    let doc = document_with_catalog(Dict::new().insert("Lang", Value::Ref(9, 0)));
    insert(&doc, 9, text("es-MX"));
    assert_eq!(
        doc.document_language().expect("referenced /Lang"),
        Some(language("es-MX"))
    );
}

#[test]
fn element_language_and_effective_language_follow_parent_chain() {
    let root = struct_root(Dict::new().insert("K", arr(vec![Value::Ref(5, 0), Value::Ref(8, 0)])));
    let doc = document_with_root(root);
    insert(
        &doc,
        5,
        Value::Dict(
            element("Document", ROOT)
                .insert("Lang", text("en-US"))
                .insert("K", arr(vec![Value::Ref(6, 0)])),
        ),
    );
    // 空串是显式未知：终止继承，不再回退到根元素。
    insert(
        &doc,
        6,
        Value::Dict(
            element("P", 5)
                .insert("Lang", text(""))
                .insert("K", arr(vec![Value::Ref(7, 0)])),
        ),
    );
    insert(&doc, 7, Value::Dict(element("Span", 6)));
    insert(&doc, 8, Value::Dict(element("Artifact", ROOT)));

    let tree = doc
        .structure_tree()
        .expect("structure tree")
        .expect("tree present");
    assert_eq!(tree.elements().len(), 4);
    assert_eq!(
        tree.element_language(0).expect("root language"),
        Some(language("en-US"))
    );
    let unknown = tree
        .element_language(1)
        .expect("explicit unknown")
        .expect("declared");
    assert!(unknown.is_unknown());
    assert_eq!(tree.element_language(2).expect("span /Lang"), None);
    assert_eq!(tree.element_language(3).expect("artifact /Lang"), None);

    assert_eq!(
        tree.effective_language(2).expect("span effective"),
        Some(unknown.clone()),
        "空串 /Lang 必须终止继承"
    );
    assert_eq!(
        tree.effective_language(1).expect("p effective"),
        Some(unknown)
    );
    assert_eq!(
        tree.effective_language(0).expect("root effective"),
        Some(language("en-US"))
    );
    assert_eq!(
        tree.effective_language(3).expect("artifact effective"),
        None,
        "没有任何声明时返回 None，由调用方回退到文档语言"
    );

    // 越界下标。
    assert_eq!(tree.element_language(99).expect("out of range"), None);
    assert_eq!(tree.effective_language(99).expect("out of range"), None);
}

#[test]
fn effective_language_stops_on_cycles_and_the_depth_limit() {
    let no_language = |_: usize| None::<String>;

    // 0 ↔ 1 的环：静默返回 None。
    let cycle = |index: usize| Some(if index == 0 { 1 } else { 0 });
    assert_eq!(
        first_declared_language(0, cycle, no_language).expect("cycle"),
        None
    );

    // 深链：第 63 层仍在 64 个节点的检查范围内，第 64 层不再检查。
    let chain = |index: usize| (index < 64).then_some(index + 1);
    let at_63 = |index: usize| (index == 63).then(|| "en".to_owned());
    assert_eq!(
        first_declared_language(0, chain, at_63).expect("depth 63"),
        Some(language("en"))
    );
    let at_64 = |index: usize| (index == 64).then(|| "en".to_owned());
    assert_eq!(
        first_declared_language(0, chain, at_64).expect("depth limit"),
        None
    );
}

#[test]
fn media_clip_alt_decodes_plain_and_multi_language_text() {
    fn filespec() -> Dict {
        Dict::new()
            .insert("Type", name("Filespec"))
            .insert("FS", name("URL"))
            .insert("F", text("clip.mov"))
    }

    fn clip_data(alt: Value) -> Dict {
        Dict::new()
            .insert("Type", name("MediaClip"))
            .insert("S", name("MCD"))
            .insert("D", Value::Dict(filespec()))
            .insert("Alt", alt)
    }

    let doc = Document::new();
    let clip = doc
        .media_clip(&Value::Dict(clip_data(multi_language(&[
            ("", "default"),
            ("es-MX", "Spanish"),
        ]))))
        .expect("media clip data");
    let MediaClip::Data(data) = clip else {
        panic!("expected media clip data, got {clip:?}");
    };
    assert!(matches!(data.data, MediaClipDataSource::FileSpec(_)));
    let alt = data
        .alternate_text()
        .expect("decode /Alt")
        .expect("/Alt present");
    assert_eq!(alt.text_for(&language("es-MX")), Some("Spanish"));
    assert_eq!(alt.text_for(&language("fr")), Some("default"));

    let clip = doc
        .media_clip(&Value::Dict(clip_data(text("plain"))))
        .expect("media clip data");
    let MediaClip::Data(data) = clip else {
        panic!("expected media clip data");
    };
    assert!(matches!(
        data.alternate_text().expect("decode /Alt"),
        Some(AlternateText::Text(ref text)) if text == "plain"
    ));

    let section = Dict::new()
        .insert("Type", name("MediaClip"))
        .insert("S", name("MCS"))
        .insert("D", Value::Dict(clip_data(text("nested"))))
        .insert("Alt", multi_language(&[("en-GB", "British")]));
    let clip = doc.media_clip(&Value::Dict(section)).expect("clip section");
    let MediaClip::Section(section) = clip else {
        panic!("expected media clip section, got {clip:?}");
    };
    let alt = section
        .alternate_text()
        .expect("decode /Alt")
        .expect("/Alt present");
    assert_eq!(alt.text_for(&language("en-GB")), Some("British"));
}

#[test]
fn invalid_language_entries_are_typed_errors() {
    let doc = document_with_catalog(Dict::new().insert("Lang", text("en_US")));
    assert_malformed(doc.document_language(), "primary language");

    let doc = document_with_catalog(Dict::new().insert("Lang", Value::Integer(1)));
    assert_malformed(doc.document_language(), "not a text string");

    let doc = document_with_catalog(Dict::new().insert("Lang", Value::Ref(9, 0)));
    assert_malformed(doc.document_language(), "does not exist");

    // 结构元素非法 /Lang：结构树本身仍可读取，只有类型化语言 API 报错。
    let doc = document_with_root(struct_root(
        Dict::new().insert("K", arr(vec![Value::Ref(5, 0)])),
    ));
    insert(
        &doc,
        5,
        Value::Dict(element("Document", ROOT).insert("Lang", text("en_US"))),
    );
    let tree = doc
        .structure_tree()
        .expect("invalid tag must not fail the tree")
        .expect("tree present");
    assert_malformed(tree.element_language(0), "primary language");
    assert_malformed(tree.effective_language(0), "primary language");

    let doc = Document::new();
    let clip = Dict::new()
        .insert("Type", name("MediaClip"))
        .insert("S", name("MCD"))
        .insert(
            "D",
            Value::Dict(
                Dict::new()
                    .insert("Type", name("Filespec"))
                    .insert("FS", name("URL"))
                    .insert("F", text("clip.mov")),
            ),
        )
        .insert("Alt", Value::Integer(1));
    let MediaClip::Data(data) = doc.media_clip(&Value::Dict(clip)).expect("clip") else {
        panic!("expected media clip data");
    };
    assert_malformed(data.alternate_text(), "alternate text");
}

#[test]
fn span_properties_parse_text_entries_and_language() {
    let properties = Dict::new()
        .insert("Alt", text("plain alt"))
        .insert("ActualText", Value::HexString("FEFF4E2D".into()))
        .insert("E", text("expanded"))
        .insert("Lang", text(""));
    let parsed = SpanProperties::parse("Span", Some(&properties))
        .expect("typed span")
        .expect("span properties");
    assert_eq!(parsed.alternate_text.as_deref(), Some("plain alt"));
    assert_eq!(
        parsed.actual_text.as_deref(),
        Some("中"),
        "UTF-16BE 十六进制串"
    );
    assert_eq!(parsed.expansion.as_deref(), Some("expanded"));
    let language = parsed.language.as_ref().expect("/Lang declared");
    assert!(language.is_unknown());
    assert_eq!(language.as_str(), "");

    let section = marked_section("Span", Some(properties), None);
    assert_eq!(
        section.span_properties().expect("section span"),
        Some(parsed)
    );
}

#[test]
fn span_properties_require_span_tag_and_text_strings() {
    let properties = Dict::new().insert("Alt", text("alt"));
    assert_eq!(
        SpanProperties::parse("P", Some(&properties)).expect("non-Span tag"),
        None
    );
    assert_eq!(
        SpanProperties::parse("span", Some(&properties)).expect("tag is case-sensitive"),
        None
    );
    assert_eq!(
        SpanProperties::parse("Span", None).expect("missing properties"),
        None
    );
    assert_eq!(
        SpanProperties::parse("Span", Some(&Dict::new())).expect("empty properties"),
        Some(SpanProperties {
            alternate_text: None,
            actual_text: None,
            expansion: None,
            language: None,
        })
    );

    let invalid_alt = Dict::new().insert("Alt", Value::Integer(1));
    match SpanProperties::parse("Span", Some(&invalid_alt)) {
        Err(LanguageError::Malformed(message)) => {
            assert!(message.contains("Alt"), "{message}");
            assert!(message.contains("integer"), "{message}");
        }
        other => panic!("expected malformed /Alt, got {other:?}"),
    }
    let invalid_lang = Dict::new().insert("Lang", name("en"));
    assert_malformed(SpanProperties::parse("Span", Some(&invalid_lang)), "Lang");
}

#[test]
fn resolve_section_language_prefers_structure_and_document_default() {
    let document_language = language("en-US");
    let sections = vec![
        // 0：外层 Span 声明 de-DE。
        marked_section(
            "Span",
            Some(Dict::new().insert("Lang", text("de-DE"))),
            None,
        ),
        // 1：无语言的嵌套标记内容，应继承外层 Span。
        marked_section("P", None, Some(0)),
        // 2：自己的 Span /Lang 覆盖外层。
        marked_section(
            "Span",
            Some(Dict::new().insert("Lang", text("ja"))),
            Some(0),
        ),
        // 3：空串表示显式未知并终止继承。
        marked_section("Span", Some(Dict::new().insert("Lang", text(""))), Some(0)),
        // 4：结构内容，结构语言优先于外层 Span。
        marked_section(
            "P",
            Some(Dict::new().insert("MCID", Value::Integer(0))),
            Some(0),
        ),
        // 5：结构语言取不到时回退目录语言，而不是外层 Span。
        marked_section(
            "P",
            Some(Dict::new().insert("MCID", Value::Integer(1))),
            Some(0),
        ),
        // 6：整条链都没有声明。
        marked_section("P", None, None),
        // 7：内容流内联数字解析为 Float，整数值仍按 MCID 处理。
        marked_section(
            "P",
            Some(Dict::new().insert("MCID", Value::Float(0.0))),
            Some(0),
        ),
    ];
    let structured_language = |properties: &Dict| -> Option<LanguageIdentifier> {
        match properties.get("MCID").and_then(Value::as_integer) {
            Some(0) => Some(language("fr-FR")),
            _ => None,
        }
    };
    let resolve = |index: usize| {
        resolve_section_language(
            &sections,
            index,
            &structured_language,
            Some(&document_language),
        )
        .expect("language")
    };

    assert_eq!(
        resolve(1),
        Some(language("de-DE")),
        "外层 Span 覆盖无语言的内层标记内容"
    );
    assert_eq!(resolve(2), Some(language("ja")), "内层 Span 覆盖外层");
    let unknown = resolve(3).expect("explicit unknown");
    assert!(unknown.is_unknown(), "空串终止继承");
    assert_eq!(resolve(4), Some(language("fr-FR")), "MCID 优先结构语言");
    assert_eq!(
        resolve(5),
        Some(document_language.clone()),
        "结构语言缺失时回退目录语言，不得继续外层 Span"
    );
    assert_eq!(
        resolve(6),
        Some(document_language.clone()),
        "无声明时回退目录语言"
    );
    assert_eq!(
        resolve(7),
        Some(language("fr-FR")),
        "内联字典的 Float 整数值按 MCID 处理"
    );
    assert_eq!(
        resolve_section_language(&sections, 6, &structured_language, None)
            .expect("no document language"),
        None
    );
}

#[test]
fn page_marked_content_languages_resolves_spans_structure_and_catalog() {
    let content = b"/Span <</Lang (de-DE)>> BDC \
                      /P <</MCID 0>> BDC /Span BMC (one) Tj EMC EMC EMC \
                    /Span <</MCID 0>> BDC (two) Tj EMC \
                    /Span <</Lang (de-DE)>> BDC (three) Tj EMC \
                    /Span BMC (four) Tj EMC";
    let root = struct_root(Dict::new())
        .insert("K", arr(vec![Value::Ref(ELEMENT, 0)]))
        .insert(
            "ParentTree",
            Value::Dict(Dict::new().insert(
                "Nums",
                arr(vec![Value::Integer(0), arr(vec![Value::Ref(ELEMENT, 0)])]),
            )),
        );
    let element = element("Document", ROOT).insert("Lang", text("fr-FR"));
    let doc = structured_page_document(content, root, element);

    let languages = doc
        .page_marked_content_languages(0)
        .expect("page languages");
    assert_eq!(
        languages,
        vec![
            Some(language("fr-FR")),
            Some(language("fr-FR")),
            Some(language("de-DE")),
            Some(language("fr-FR")),
            Some(language("de-DE")),
            Some(language("en-US")),
        ]
    );
}

#[test]
fn artifact_properties_parse_table330_entries() {
    let properties = Dict::new()
        .insert("Type", name("Pagination"))
        .insert(
            "BBox",
            arr(vec![
                Value::Integer(0),
                Value::Integer(0),
                Value::Float(612.0),
                Value::Integer(792),
            ]),
        )
        .insert("Attached", arr(vec![name("Top"), name("Bottom")]))
        .insert("Subtype", name("Header"));
    let parsed = ArtifactProperties::parse("Artifact", Some(&properties))
        .expect("typed artifact")
        .expect("artifact properties");
    assert_eq!(parsed.artifact_type, Some(ArtifactType::Pagination));
    assert_eq!(parsed.bounding_box, Some([0.0, 0.0, 612.0, 792.0]));
    assert_eq!(
        parsed.attached,
        vec![ArtifactAttachment::Top, ArtifactAttachment::Bottom]
    );
    assert_eq!(parsed.subtype, Some(ArtifactSubtype::Header));
    assert_eq!(
        parsed.artifact_type.as_ref().map(ArtifactType::as_str),
        Some("Pagination")
    );
    assert_eq!(
        parsed.subtype.as_ref().map(ArtifactSubtype::as_str),
        Some("Header")
    );

    let section = marked_section("Artifact", Some(properties), None);
    assert_eq!(
        section.artifact_properties().expect("section artifact"),
        Some(parsed)
    );

    let layout = Dict::new().insert("Type", name("Layout"));
    let parsed = ArtifactProperties::parse("Artifact", Some(&layout))
        .expect("typed layout")
        .expect("layout properties");
    assert_eq!(parsed.artifact_type, Some(ArtifactType::Layout));
    assert_eq!(parsed.bounding_box, None);
    assert!(parsed.attached.is_empty());
    assert_eq!(parsed.subtype, None);
}

#[test]
fn artifact_properties_preserve_unknown_names_and_ignore_other_tag() {
    let properties = Dict::new()
        .insert("Type", name("Background"))
        .insert("Subtype", name("Bates"))
        .insert("Unmodelled", Value::Integer(1));
    let parsed = ArtifactProperties::parse("Artifact", Some(&properties))
        .expect("typed artifact")
        .expect("artifact properties");
    assert_eq!(parsed.artifact_type, Some(ArtifactType::Background));
    assert_eq!(
        parsed.subtype,
        Some(ArtifactSubtype::Other("Bates".to_owned()))
    );
    assert_eq!(
        parsed.subtype.as_ref().map(ArtifactSubtype::as_str),
        Some("Bates"),
        "unknown subtype keeps its spelling"
    );
    assert_eq!(ArtifactType::from_name("Slug").as_str(), "Slug");

    assert_eq!(
        ArtifactProperties::parse("Span", Some(&properties)).expect("non-Artifact tag"),
        None
    );
    assert_eq!(
        ArtifactProperties::parse("Artifact", None).expect("missing property list"),
        None
    );
    assert_eq!(
        ArtifactProperties::parse("artifact", Some(&properties)).expect("case-sensitive tag"),
        None
    );
}

#[test]
fn malformed_artifact_properties_are_typed_errors() {
    fn message(properties: &Dict) -> String {
        match ArtifactProperties::parse("Artifact", Some(properties)) {
            Err(ArtifactError::Malformed(message)) => message,
            other => panic!("expected malformed Artifact properties, got {other:?}"),
        }
    }

    assert_eq!(
        ArtifactError::Malformed("x".to_owned()).to_string(),
        "invalid artifact property list: x"
    );

    let bad_type = message(&Dict::new().insert("Type", text("Pagination")));
    assert!(
        bad_type.contains("Type") && bad_type.contains("string"),
        "{bad_type}"
    );

    let short_bbox = message(&Dict::new().insert("BBox", arr(vec![Value::Integer(0); 3])));
    assert!(
        short_bbox.contains("BBox") && short_bbox.contains("4"),
        "{short_bbox}"
    );
    let named_bbox = message(&Dict::new().insert(
        "BBox",
        arr(vec![
            Value::Integer(0),
            Value::Integer(0),
            name("x"),
            Value::Integer(0),
        ]),
    ));
    assert!(
        named_bbox.contains("BBox") && named_bbox.contains("name"),
        "{named_bbox}"
    );
    let infinite_bbox = message(&Dict::new().insert(
        "BBox",
        arr(vec![
            Value::Float(f64::INFINITY),
            Value::Integer(0),
            Value::Integer(1),
            Value::Integer(1),
        ]),
    ));
    assert!(infinite_bbox.contains("BBox"), "{infinite_bbox}");
    let scalar_bbox = message(&Dict::new().insert("BBox", Value::Integer(0)));
    assert!(
        scalar_bbox.contains("BBox") && scalar_bbox.contains("integer"),
        "{scalar_bbox}"
    );

    let empty_attached = message(&Dict::new().insert("Attached", arr(Vec::new())));
    assert!(empty_attached.contains("Attached"), "{empty_attached}");
    let crowded_attached = message(&Dict::new().insert(
        "Attached",
        arr(vec![
            name("Top"),
            name("Bottom"),
            name("Left"),
            name("Right"),
            name("Top"),
        ]),
    ));
    assert!(crowded_attached.contains("Attached"), "{crowded_attached}");
    let duplicate_attached =
        message(&Dict::new().insert("Attached", arr(vec![name("Top"), name("Top")])));
    assert!(duplicate_attached.contains("Top"), "{duplicate_attached}");
    let unknown_attached = message(&Dict::new().insert("Attached", arr(vec![name("Middle")])));
    assert!(unknown_attached.contains("Middle"), "{unknown_attached}");
    let scalar_attached = message(&Dict::new().insert("Attached", name("Top")));
    assert!(
        scalar_attached.contains("Attached") && scalar_attached.contains("name"),
        "{scalar_attached}"
    );

    let bad_subtype = message(&Dict::new().insert("Subtype", text("Header")));
    assert!(
        bad_subtype.contains("Subtype") && bad_subtype.contains("string"),
        "{bad_subtype}"
    );
}
