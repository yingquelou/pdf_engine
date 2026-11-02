//! 标记内容层面的语言解析：`resolve_section_language` 的 Span 继承、结构
//! 语言优先与目录兜底，以及 `page_marked_content_languages` 的整页序列。

use crate::core::types::{Dict, Document, StreamData, Trailer, Value};

use super::super::span::resolve_section_language;
use super::super::*;
use super::support::{
    CATALOG, ROOT, arr, element, insert, language, marked_section, name, struct_root, text,
};

const PAGES: i64 = 3;
const PAGE: i64 = 4;
const ELEMENT: i64 = 5;

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
