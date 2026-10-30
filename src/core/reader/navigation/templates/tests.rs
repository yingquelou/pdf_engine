//! 命名模板页读端测试：`/Names /Templates` 名称树、键解码与 `/Type` 校验。

use crate::core::types::{Array, Dict, Object, PdfRect, PdfTextString, Trailer, Value};

use super::*;

const PAGES_OBJECT: i64 = 2;
const PAGE_BASE: i64 = 10;

fn arr(values: Vec<Value>) -> Value {
    Value::Array(Array(values))
}

fn name(value: &str) -> Value {
    Value::Name(value.to_owned())
}

fn hex_of(text: &str) -> String {
    PdfTextString::from(text)
        .encode()
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect()
}

fn insert(doc: &Document, num: i64, value: Value) {
    doc.set_object(Object::new(num, 0, value));
}

/// 含 `page_count` 个页面的文档；`catalog` 条目并入目录。
fn document_with_pages(page_count: usize, catalog: Dict) -> Document {
    let mut doc = Document::new();
    let catalog = catalog
        .insert("Type", name("Catalog"))
        .insert("Pages", Value::Ref(PAGES_OBJECT, 0));
    let mut kids = Array::new();
    for index in 0..page_count {
        let num = PAGE_BASE + index as i64;
        kids = kids.push(Value::Ref(num, 0));
        insert(
            &doc,
            num,
            Value::Dict(
                Dict::new()
                    .insert("Type", name("Page"))
                    .insert("Parent", Value::Ref(PAGES_OBJECT, 0)),
            ),
        );
    }
    insert(
        &doc,
        PAGES_OBJECT,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Pages"))
                .insert("Kids", Value::Array(kids))
                .insert("Count", Value::Integer(page_count as i64)),
        ),
    );
    doc.set_object(Object::new(1, 0, Value::Dict(catalog)));
    doc.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(1, 0)),
    };
    doc
}

fn names_catalog(category: &str, node: i64) -> Dict {
    Dict::new().insert(
        "Names",
        Value::Dict(Dict::new().insert(category, Value::Ref(node, 0))),
    )
}

fn template_dict(media_box: Value) -> Value {
    Value::Dict(
        Dict::new()
            .insert("Type", name("Template"))
            .insert("MediaBox", media_box),
    )
}

fn media_box(values: [f64; 4]) -> Value {
    arr(values.iter().map(|value| Value::Float(*value)).collect())
}

#[test]
fn page_templates_read_entries_and_decode_names() {
    let doc = document_with_pages(2, names_catalog("Templates", 30));
    insert(
        &doc,
        30,
        Value::Dict(Dict::new().insert(
            "Names",
            arr(vec![
                Value::String("Cover".into()),
                Value::Ref(40, 0),
                Value::HexString(hex_of("Zeta")),
                Value::Ref(41, 0),
                Value::HexString(hex_of("图章")),
                Value::Ref(42, 0),
            ]),
        )),
    );
    insert(&doc, 40, template_dict(media_box([0.0, 0.0, 595.0, 842.0])));
    // MediaBox 可以间接引用；读端须解引用后解析。
    insert(
        &doc,
        41,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Template"))
                .insert("MediaBox", Value::Ref(43, 0)),
        ),
    );
    insert(
        &doc,
        42,
        Value::Dict(Dict::new().insert("Type", name("Template"))),
    );
    insert(&doc, 43, media_box([0.0, 0.0, 144.0, 72.0]));

    let templates = doc.page_templates().expect("template name tree");
    let keys: Vec<&str> = templates.keys().map(String::as_str).collect();
    assert_eq!(keys, ["Cover", "Zeta", "图章"], "键按文本串解码并排序");

    let cover = &templates["Cover"];
    assert_eq!(cover.dict.get("Type"), Some(&name("Template")));
    assert_eq!(cover.media_box, Some(PdfRect::new(0.0, 0.0, 595.0, 842.0)));

    let zeta = &templates["Zeta"];
    assert_eq!(zeta.media_box, Some(PdfRect::new(0.0, 0.0, 144.0, 72.0)));

    let stamp = &templates["图章"];
    assert_eq!(stamp.media_box, None, "缺少 /MediaBox 时保留字典但不报错");
    assert_eq!(stamp.dict.get("Type"), Some(&name("Template")));
}

#[test]
fn page_templates_are_empty_by_default() {
    // 完全没有 /Names。
    let doc = document_with_pages(1, Dict::new());
    assert!(doc.page_templates().unwrap().is_empty());

    // /Names 存在但只有其他类别。
    let doc = document_with_pages(1, names_catalog("Dests", 30));
    insert(
        &doc,
        30,
        Value::Dict(Dict::new().insert("Names", arr(vec![]))),
    );
    assert!(doc.page_templates().unwrap().is_empty());
}

#[test]
fn page_templates_reject_non_template_entries() {
    let doc = document_with_pages(1, names_catalog("Templates", 30));
    insert(
        &doc,
        30,
        Value::Dict(Dict::new().insert(
            "Names",
            arr(vec![Value::String("Wrong".into()), Value::Ref(40, 0)]),
        )),
    );
    insert(
        &doc,
        40,
        Value::Dict(Dict::new().insert("Type", name("Page"))),
    );
    let error = doc.page_templates().unwrap_err();
    assert!(
        error.to_string().contains("/Type is not Template"),
        "{error}"
    );

    // 缺少 /Type 同样按 Malformed 处理。
    insert(&doc, 40, Value::Dict(Dict::new()));
    let error = doc.page_templates().unwrap_err();
    assert!(
        error.to_string().contains("/Type is not Template"),
        "{error}"
    );

    // 条目不是字典。
    insert(&doc, 40, Value::Integer(7));
    let error = doc.page_templates().unwrap_err();
    assert!(error.to_string().contains("is not a dictionary"), "{error}");

    // 间接引用不可解析。
    insert(
        &doc,
        30,
        Value::Dict(Dict::new().insert(
            "Names",
            arr(vec![Value::String("Dangling".into()), Value::Ref(99, 0)]),
        )),
    );
    let error = doc.page_templates().unwrap_err();
    assert!(matches!(error, NavigationError::Malformed(_)), "{error}");
    assert!(error.to_string().contains("not found"), "{error}");
}

#[test]
fn page_templates_surface_name_tree_errors() {
    // 节点既无 /Kids 也无 /Names。
    let doc = document_with_pages(1, names_catalog("Templates", 30));
    insert(&doc, 30, Value::Dict(Dict::new()));
    let error = doc.page_templates().unwrap_err();
    assert!(
        error.to_string().contains("neither /Kids nor /Names"),
        "{error}"
    );

    // 键值数组长度不是偶数。
    insert(
        &doc,
        30,
        Value::Dict(Dict::new().insert("Names", arr(vec![Value::String("Odd".into())]))),
    );
    let error = doc.page_templates().unwrap_err();
    assert!(error.to_string().contains("not even"), "{error}");

    // 节点成环由既有加固遍历拒绝。
    insert(
        &doc,
        30,
        Value::Dict(Dict::new().insert("Kids", arr(vec![Value::Ref(30, 0)]))),
    );
    let error = doc.page_templates().unwrap_err();
    assert!(error.to_string().contains("cycle"), "{error}");
}
