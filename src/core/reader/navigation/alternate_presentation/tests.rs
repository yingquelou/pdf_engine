//! 备用演示读端测试：`/Names /AlternatePresentations` 名称树、Table 297
//! 条目校验、资源解引用、名称解码与名称树错误传播。

use crate::core::types::{Array, Dict, Object, PdfTextString, Trailer, Value};

use super::*;

const CATALOG_OBJECT: i64 = 1;

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

/// UTF-16BE（带 BOM）文本串的十六进制；用于构造与 PDFDocEncoding
/// 表示不同的原始键，观察解码后的重名判定。
fn utf16_hex_of(text: &str) -> String {
    let mut hex = String::from("FEFF");
    for unit in text.encode_utf16() {
        hex.push_str(&format!("{unit:04X}"));
    }
    hex
}

fn insert(doc: &Document, num: i64, value: Value) {
    doc.set_object(Object::new(num, 0, value));
}

/// 含目录的最小文档；`catalog` 条目并入目录。
fn document(catalog: Dict) -> Document {
    let mut doc = Document::new();
    insert(
        &doc,
        CATALOG_OBJECT,
        Value::Dict(catalog.insert("Type", name("Catalog"))),
    );
    doc.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(CATALOG_OBJECT, 0)),
    };
    doc
}

fn names_catalog(category: &str, node: i64) -> Dict {
    Dict::new().insert(
        "Names",
        Value::Dict(Dict::new().insert(category, Value::Ref(node, 0))),
    )
}

fn resources(entries: Vec<Value>) -> Value {
    Value::Dict(Dict::new().insert("Names", arr(entries)))
}

fn slide_show(resources: Value, start: Value) -> Dict {
    Dict::new()
        .insert("Type", name("SlideShow"))
        .insert("Subtype", name("Embedded"))
        .insert("Resources", resources)
        .insert("StartResource", start)
}

/// 单个 `/AlternatePresentations` 条目的文档。
fn document_with_show(show: Value) -> Document {
    let doc = document(names_catalog("AlternatePresentations", 20));
    insert(
        &doc,
        20,
        Value::Dict(Dict::new().insert(
            "Names",
            arr(vec![Value::String("Show".into()), Value::Ref(21, 0)]),
        )),
    );
    insert(&doc, 21, show);
    doc
}

fn valid_resources() -> Value {
    resources(vec![
        Value::HexString(hex_of("a.svg")),
        Value::Dict(Dict::new()),
    ])
}

#[test]
fn alternate_presentations_traverse_multileaf_resources_name_tree() {
    let doc = document(names_catalog("AlternatePresentations", 20));
    insert(
        &doc,
        20,
        Value::Dict(Dict::new().insert(
            "Names",
            arr(vec![Value::String("Show".into()), Value::Ref(21, 0)]),
        )),
    );
    // `/Resources` 为含 `/Kids` 的多叶子名称树；读端应递归遍历而非仅单叶子。
    let show = slide_show(Value::Ref(32, 0), Value::String("z.svg".into()));
    insert(&doc, 21, Value::Dict(show));
    insert(
        &doc,
        32,
        Value::Dict(
            Dict::new()
                .insert("Kids", arr(vec![Value::Ref(30, 0), Value::Ref(31, 0)]))
                .insert(
                    "Limits",
                    arr(vec![
                        Value::String("a.svg".into()),
                        Value::String("z.svg".into()),
                    ]),
                ),
        ),
    );
    insert(
        &doc,
        30,
        Value::Dict(
            Dict::new()
                .insert(
                    "Names",
                    arr(vec![
                        Value::String("a.svg".into()),
                        Value::Dict(Dict::new()),
                    ]),
                )
                .insert(
                    "Limits",
                    arr(vec![
                        Value::String("a.svg".into()),
                        Value::String("a.svg".into()),
                    ]),
                ),
        ),
    );
    insert(
        &doc,
        31,
        Value::Dict(
            Dict::new()
                .insert(
                    "Names",
                    arr(vec![
                        Value::String("z.svg".into()),
                        Value::Dict(Dict::new()),
                    ]),
                )
                .insert(
                    "Limits",
                    arr(vec![
                        Value::String("z.svg".into()),
                        Value::String("z.svg".into()),
                    ]),
                ),
        ),
    );

    let shows = doc.alternate_presentations().expect("multileaf resources");
    let show = &shows["Show"];
    let keys: Vec<&str> = show.resources.keys().map(String::as_str).collect();
    assert_eq!(keys, ["a.svg", "z.svg"]);
    assert_eq!(show.start_resource, "z.svg");
}

fn rejected(show: Dict, expected: &str) {
    let doc = document_with_show(Value::Dict(show));
    let error = doc.alternate_presentations().unwrap_err();
    assert!(matches!(error, NavigationError::Malformed(_)), "{error}");
    assert!(error.to_string().contains(expected), "{error}");
}

/// 未声明名称树或只有其他类别时返回空映射。
#[test]
fn alternate_presentations_are_empty_by_default() {
    // 完全没有 /Names。
    let doc = document(Dict::new());
    assert!(doc.alternate_presentations().unwrap().is_empty());

    // /Names 存在但只有其他类别。
    let doc = document(names_catalog("Templates", 30));
    insert(
        &doc,
        30,
        Value::Dict(Dict::new().insert("Names", arr(vec![]))),
    );
    assert!(doc.alternate_presentations().unwrap().is_empty());
}

/// 读取幻灯片、解码名称与 `/StartResource`，并保留资源值。
#[test]
fn alternate_presentations_read_shows_and_resources() {
    let doc = document(names_catalog("AlternatePresentations", 20));
    insert(
        &doc,
        20,
        Value::Dict(Dict::new().insert(
            "Names",
            arr(vec![
                Value::String("MySlideShow".into()),
                Value::Ref(21, 0),
                Value::HexString(hex_of("Deck Two")),
                Value::Ref(22, 0),
            ]),
        )),
    );
    insert(
        &doc,
        21,
        Value::Dict(slide_show(
            // 资源名称树可以间接引用。
            Value::Ref(23, 0),
            Value::String("mysvg.svg".into()),
        )),
    );
    insert(
        &doc,
        22,
        Value::Dict(slide_show(
            resources(vec![
                Value::HexString(hex_of("cover.png")),
                Value::Ref(24, 0),
            ]),
            Value::HexString(hex_of("cover.png")),
        )),
    );
    insert(
        &doc,
        23,
        resources(vec![
            Value::HexString(hex_of("abc0001.jpg")),
            Value::Ref(31, 0),
            Value::HexString(hex_of("mysvg.svg")),
            Value::Ref(32, 0),
            Value::HexString(hex_of("图")),
            Value::Ref(33, 0),
        ]),
    );
    insert(
        &doc,
        31,
        Value::Dict(Dict::new().insert("Type", name("Image"))),
    );
    insert(
        &doc,
        32,
        Value::Dict(Dict::new().insert("Type", name("Filespec"))),
    );
    insert(
        &doc,
        33,
        Value::Dict(Dict::new().insert("Type", name("Metadata"))),
    );
    insert(
        &doc,
        24,
        Value::Dict(Dict::new().insert("Type", name("Image"))),
    );

    let shows = doc
        .alternate_presentations()
        .expect("alternate presentations");
    let keys: Vec<&str> = shows.keys().map(String::as_str).collect();
    assert_eq!(keys, ["Deck Two", "MySlideShow"], "键按解码文本排序");

    let show = &shows["MySlideShow"];
    assert_eq!(
        show.dict.get("Type"),
        Some(&name("SlideShow")),
        "保留幻灯片字典"
    );
    assert_eq!(show.dict.get("Subtype"), Some(&name("Embedded")));
    assert_eq!(show.start_resource, "mysvg.svg");
    let resources: Vec<&str> = show.resources.keys().map(String::as_str).collect();
    assert_eq!(resources, ["abc0001.jpg", "mysvg.svg", "图"]);
    assert_eq!(
        show.resources["abc0001.jpg"],
        Value::Dict(Dict::new().insert("Type", name("Image")))
    );

    let second = &shows["Deck Two"];
    assert_eq!(second.resources.len(), 1);
    assert_eq!(second.start_resource, "cover.png");
}

/// 资源名称树与资源值中的间接引用均被解引用；悬空引用报类型化错误。
#[test]
fn alternate_presentations_resolve_indirect_values() {
    let show_dict = Value::Ref(22, 0);
    let doc = document(names_catalog("AlternatePresentations", 20));
    insert(
        &doc,
        20,
        Value::Dict(
            Dict::new().insert("Names", arr(vec![Value::String("Show".into()), show_dict])),
        ),
    );
    insert(
        &doc,
        22,
        Value::Dict(slide_show(
            resources(vec![Value::HexString(hex_of("a.svg")), Value::Ref(40, 0)]),
            Value::HexString(hex_of("a.svg")),
        )),
    );
    insert(
        &doc,
        40,
        Value::Dict(Dict::new().insert("Type", name("EmbeddedFile"))),
    );

    let shows = doc.alternate_presentations().expect("show");
    assert_eq!(
        shows["Show"].resources["a.svg"],
        Value::Dict(Dict::new().insert("Type", name("EmbeddedFile"))),
        "资源值必须解引用后保留"
    );

    // 悬空的资源引用由 resolve 报类型化错误。
    insert(
        &doc,
        22,
        Value::Dict(slide_show(
            resources(vec![Value::HexString(hex_of("a.svg")), Value::Ref(99, 0)]),
            Value::HexString(hex_of("a.svg")),
        )),
    );
    let error = doc.alternate_presentations().unwrap_err();
    assert!(matches!(error, NavigationError::Malformed(_)), "{error}");
    assert!(error.to_string().contains("not found"), "{error}");
}

/// 违反 Table 297 必填条目时均返回 Malformed。
#[test]
fn alternate_presentations_require_table297_entries() {
    // /Type 缺失或错误。
    rejected(
        slide_show(valid_resources(), Value::HexString(hex_of("a.svg")))
            .insert("Type", name("Page")),
        "/Type is not SlideShow",
    );
    rejected(
        Dict::new()
            .insert("Subtype", name("Embedded"))
            .insert("Resources", valid_resources())
            .insert("StartResource", Value::HexString(hex_of("a.svg"))),
        "/Type is not SlideShow",
    );

    // /Subtype 缺失或错误。
    rejected(
        slide_show(valid_resources(), Value::HexString(hex_of("a.svg")))
            .insert("Subtype", name("Slideshow")),
        "/Subtype is not Embedded",
    );
    rejected(
        Dict::new()
            .insert("Type", name("SlideShow"))
            .insert("Resources", valid_resources())
            .insert("StartResource", Value::HexString(hex_of("a.svg"))),
        "/Subtype is not Embedded",
    );

    // /Resources 缺失或不是字典。
    rejected(
        Dict::new()
            .insert("Type", name("SlideShow"))
            .insert("Subtype", name("Embedded"))
            .insert("StartResource", Value::HexString(hex_of("a.svg"))),
        "has no /Resources",
    );
    rejected(
        slide_show(Value::Integer(7), Value::HexString(hex_of("a.svg"))),
        "/Resources is not a dictionary",
    );

    // /StartResource 缺失、不是字符串或不指向资源键。
    rejected(
        Dict::new()
            .insert("Type", name("SlideShow"))
            .insert("Subtype", name("Embedded"))
            .insert("Resources", valid_resources()),
        "has no /StartResource",
    );
    rejected(
        slide_show(valid_resources(), Value::Name("a.svg".into())),
        "/StartResource is not a string",
    );
    rejected(
        slide_show(valid_resources(), Value::HexString(hex_of("b.svg"))),
        "is not a /Resources key",
    );

    // 条目不是字典或引用不可解析。
    let doc = document_with_show(Value::Integer(7));
    let error = doc.alternate_presentations().unwrap_err();
    assert!(matches!(error, NavigationError::Malformed(_)), "{error}");
    assert!(error.to_string().contains("is not a dictionary"), "{error}");

    let doc = document(names_catalog("AlternatePresentations", 20));
    insert(
        &doc,
        20,
        Value::Dict(Dict::new().insert(
            "Names",
            arr(vec![Value::String("Show".into()), Value::Ref(99, 0)]),
        )),
    );
    let error = doc.alternate_presentations().unwrap_err();
    assert!(matches!(error, NavigationError::Malformed(_)), "{error}");
    assert!(error.to_string().contains("not found"), "{error}");
}

/// 不同原始编码解码后同名的幻灯片或资源键应被拒绝。
#[test]
fn alternate_presentations_reject_duplicate_decoded_names() {
    // 两个原始键（PDFDocEncoding 与 UTF-16BE）解码后同名。
    let doc = document(names_catalog("AlternatePresentations", 20));
    insert(
        &doc,
        20,
        Value::Dict(Dict::new().insert(
            "Names",
            arr(vec![
                Value::String("Dup".into()),
                Value::Ref(21, 0),
                Value::HexString(utf16_hex_of("Dup")),
                Value::Ref(22, 0),
            ]),
        )),
    );
    insert(
        &doc,
        21,
        Value::Dict(slide_show(valid_resources(), Value::String("a.svg".into()))),
    );
    let error = doc.alternate_presentations().unwrap_err();
    assert!(matches!(error, NavigationError::Malformed(_)), "{error}");
    assert!(
        error.to_string().contains("duplicate name \"Dup\""),
        "{error}"
    );

    // 资源名称树的重名同样拒绝。
    let doc = document_with_show(Value::Dict(slide_show(
        resources(vec![
            Value::String("a".into()),
            Value::Dict(Dict::new()),
            Value::HexString(utf16_hex_of("a")),
            Value::Dict(Dict::new()),
        ]),
        Value::String("a".into()),
    )));
    let error = doc.alternate_presentations().unwrap_err();
    assert!(matches!(error, NavigationError::Malformed(_)), "{error}");
    assert!(
        error.to_string().contains("duplicate name \"a\""),
        "{error}"
    );
}

/// 名称树结构错误（缺 `/Names` `/Kids`、奇数条目、成环）向外传播。
#[test]
fn alternate_presentations_surface_name_tree_errors() {
    // 节点既无 /Kids 也无 /Names。
    let doc = document(names_catalog("AlternatePresentations", 20));
    insert(&doc, 20, Value::Dict(Dict::new()));
    let error = doc.alternate_presentations().unwrap_err();
    assert!(
        error.to_string().contains("neither /Kids nor /Names"),
        "{error}"
    );

    // 键值数组长度不是偶数。
    insert(
        &doc,
        20,
        Value::Dict(Dict::new().insert("Names", arr(vec![Value::String("Odd".into())]))),
    );
    let error = doc.alternate_presentations().unwrap_err();
    assert!(error.to_string().contains("not even"), "{error}");

    // 节点成环由既有加固遍历拒绝。
    insert(
        &doc,
        20,
        Value::Dict(Dict::new().insert("Kids", arr(vec![Value::Ref(20, 0)]))),
    );
    let error = doc.alternate_presentations().unwrap_err();
    assert!(error.to_string().contains("cycle"), "{error}");

    // 资源名称树的结构错误同样向上传播。
    let doc = document_with_show(Value::Dict(slide_show(
        Value::Dict(Dict::new()),
        Value::String("a".into()),
    )));
    let error = doc.alternate_presentations().unwrap_err();
    assert!(
        error.to_string().contains("neither /Kids nor /Names"),
        "{error}"
    );
}
