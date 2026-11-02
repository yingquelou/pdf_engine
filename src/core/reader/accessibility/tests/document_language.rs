//! 文档默认语言与结构元素语言（ISO 32000-1 14.9.2.1–14.9.2.3）：目录
//! `/Lang` 读取、`/Lang` 沿父链继承与生效语言、环与深度限制，以及非法
//! 语言条目的类型化错误。

use crate::core::reader::rendition::MediaClip;
use crate::core::types::{Dict, Document, Trailer, Value};

use super::super::first_declared_language;
use super::support::{
    CATALOG, ROOT, arr, assert_malformed, element, insert, language, name, struct_root, text,
};

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
