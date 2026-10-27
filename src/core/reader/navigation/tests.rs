//! E1 读端导航结构测试。

use crate::core::types::{Array, Dict, Document, Object, PdfTextString, Trailer, Value};

use super::*;

const PAGES_OBJECT: i64 = 2;
const PAGE_BASE: i64 = 10;

fn arr(values: Vec<Value>) -> Value {
    Value::Array(Array(values))
}

fn name(value: &str) -> Value {
    Value::Name(value.to_owned())
}

fn page_ref(index: usize) -> Value {
    Value::Ref(PAGE_BASE + index as i64, 0)
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

/// 构造含 `page_count` 个页面的文档；`catalog` 的条目并入目录字典。
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
    doc.set_object(Object::new(1, 0, Value::Dict(catalog.clone())));
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

#[test]
fn name_tree_reads_flat_and_kid_nodes() {
    let doc = document_with_pages(1, names_catalog("Dests", 30));
    insert(
        &doc,
        30,
        Value::Dict(Dict::new().insert("Kids", arr(vec![Value::Ref(31, 0), Value::Ref(32, 0)]))),
    );
    insert(
        &doc,
        31,
        Value::Dict(Dict::new().insert(
            "Names",
            arr(vec![
                Value::String("Alpha".into()),
                Value::Integer(1),
                Value::String("Delta".into()),
                Value::Integer(4),
            ]),
        )),
    );
    insert(
        &doc,
        32,
        Value::Dict(Dict::new().insert(
            "Names",
            arr(vec![
                Value::String("Beta".into()),
                Value::Integer(2),
                Value::String("Gamma".into()),
                Value::Integer(3),
            ]),
        )),
    );

    let tree = doc.name_tree("Dests").unwrap();
    let keys: Vec<&str> = tree.keys().map(String::as_str).collect();
    assert_eq!(keys, ["Alpha", "Beta", "Delta", "Gamma"]);
    assert_eq!(tree["Gamma"].as_integer(), Some(3));
    assert!(doc.name_tree("EmbeddedFiles").unwrap().is_empty());
}

#[test]
fn name_tree_rejects_cycles_and_depth_overflow() {
    let doc = document_with_pages(1, names_catalog("Dests", 35));
    insert(
        &doc,
        35,
        Value::Dict(Dict::new().insert("Kids", arr(vec![Value::Ref(35, 0)]))),
    );
    let error = doc.name_tree("Dests").unwrap_err();
    assert!(matches!(error, NavigationError::Malformed(_)), "{error}");
    assert!(error.to_string().contains("cycle"), "{error}");

    let deep = document_with_pages(1, names_catalog("Dests", 100));
    for num in 100..170 {
        insert(
            &deep,
            num,
            Value::Dict(Dict::new().insert("Kids", arr(vec![Value::Ref(num + 1, 0)]))),
        );
    }
    insert(
        &deep,
        170,
        Value::Dict(Dict::new().insert("Names", arr(vec![]))),
    );
    let error = deep.name_tree("Dests").unwrap_err();
    assert!(
        matches!(error, NavigationError::LimitExceeded("name tree depth")),
        "{error}"
    );
}

#[test]
fn name_tree_rejects_malformed_nodes() {
    let doc = document_with_pages(1, names_catalog("Dests", 36));
    insert(&doc, 36, Value::Dict(Dict::new()));
    let error = doc.name_tree("Dests").unwrap_err();
    assert!(
        error.to_string().contains("neither /Kids nor /Names"),
        "{error}"
    );

    insert(
        &doc,
        36,
        Value::Dict(Dict::new().insert("Names", arr(vec![Value::String("Odd".into())]))),
    );
    let error = doc.name_tree("Dests").unwrap_err();
    assert!(error.to_string().contains("not even"), "{error}");
}

#[test]
fn explicit_destinations_parse_all_types() {
    let doc = document_with_pages(3, Dict::new());
    let index = page_tree_index(&doc).unwrap();
    let parse = |value: Value| destination::parse_destination(&doc, &value, &index);

    assert_eq!(
        parse(arr(vec![
            page_ref(0),
            name("XYZ"),
            Value::Null,
            Value::Integer(700),
            Value::Float(1.5),
        ]))
        .unwrap(),
        Destination::Xyz {
            page: 0,
            left: None,
            top: Some(700.0),
            zoom: Some(1.5),
        }
    );
    assert_eq!(
        parse(arr(vec![page_ref(1), name("Fit")])).unwrap(),
        Destination::Fit { page: 1 }
    );
    assert_eq!(
        parse(arr(vec![page_ref(2), name("FitH"), Value::Integer(720)])).unwrap(),
        Destination::FitH {
            page: 2,
            top: Some(720.0),
        }
    );
    assert_eq!(
        parse(arr(vec![page_ref(0), name("FitV"), Value::Null])).unwrap(),
        Destination::FitV {
            page: 0,
            left: None,
        }
    );
    assert_eq!(
        parse(arr(vec![
            page_ref(1),
            name("FitR"),
            Value::Integer(0),
            Value::Integer(0),
            Value::Integer(200),
            Value::Float(300.5),
        ]))
        .unwrap(),
        Destination::FitR {
            page: 1,
            rect: [0.0, 0.0, 200.0, 300.5],
        }
    );
    assert_eq!(
        parse(arr(vec![page_ref(2), name("FitB")])).unwrap(),
        Destination::FitB { page: 2 }
    );
    assert_eq!(
        parse(arr(vec![page_ref(0), name("FitBH"), Value::Integer(10)])).unwrap(),
        Destination::FitBH {
            page: 0,
            top: Some(10.0),
        }
    );
    assert_eq!(
        parse(arr(vec![page_ref(1), name("FitBV"), Value::Null])).unwrap(),
        Destination::FitBV {
            page: 1,
            left: None,
        }
    );
}

#[test]
fn malformed_destinations_are_typed_errors() {
    let doc = document_with_pages(2, Dict::new());
    let index = page_tree_index(&doc).unwrap();
    let parse = |value: Value| destination::parse_destination(&doc, &value, &index);

    let error = parse(arr(vec![page_ref(0), name("Nope")])).unwrap_err();
    assert!(
        error.to_string().contains("unknown destination type"),
        "{error}"
    );

    let error = parse(arr(vec![Value::Integer(10), name("Fit")])).unwrap_err();
    assert!(
        error.to_string().contains("indirect page reference"),
        "{error}"
    );

    let error = parse(arr(vec![Value::Ref(99, 0), name("Fit")])).unwrap_err();
    assert_eq!(error, NavigationError::PageNotFound);

    let error = parse(arr(vec![
        page_ref(0),
        name("FitR"),
        Value::Integer(0),
        Value::Integer(0),
        Value::Integer(1),
    ]))
    .unwrap_err();
    assert!(
        error.to_string().contains("operand 5 is missing"),
        "{error}"
    );

    let error = parse(arr(vec![page_ref(0), name("Fit"), Value::Integer(1)])).unwrap_err();
    assert!(error.to_string().contains("unexpected operand"), "{error}");
}

#[test]
fn named_destinations_resolve_from_name_tree_and_legacy_dict() {
    let mut catalog = names_catalog("Dests", 40);
    catalog = catalog.insert(
        "Dests",
        Value::Dict(
            Dict::new()
                .insert("Legacy", arr(vec![page_ref(0), name("Fit")]))
                .insert("Old#20Name", arr(vec![page_ref(1), name("Fit")])),
        ),
    );
    let doc = document_with_pages(3, catalog);
    insert(
        &doc,
        40,
        Value::Dict(Dict::new().insert(
            "Names",
            arr(vec![
                Value::String("Chapter1".into()),
                arr(vec![page_ref(1), name("Fit")]),
                Value::String("Chapter2".into()),
                Value::Dict(Dict::new().insert(
                    "D",
                    arr(vec![
                        page_ref(0),
                        name("XYZ"),
                        Value::Integer(0),
                        Value::Integer(800),
                        Value::Null,
                    ]),
                )),
                Value::HexString(hex_of("Hex")),
                arr(vec![page_ref(2), name("Fit")]),
            ]),
        )),
    );

    assert_eq!(
        doc.named_destination("Chapter1").unwrap(),
        Some(Destination::Fit { page: 1 })
    );
    assert_eq!(
        doc.named_destination("Chapter2").unwrap(),
        Some(Destination::Xyz {
            page: 0,
            left: Some(0.0),
            top: Some(800.0),
            zoom: None,
        })
    );
    assert_eq!(
        doc.named_destination("Hex").unwrap(),
        Some(Destination::Fit { page: 2 })
    );
    assert_eq!(
        doc.named_destination("Legacy").unwrap(),
        Some(Destination::Fit { page: 0 })
    );
    assert_eq!(
        doc.named_destination("Old Name").unwrap(),
        Some(Destination::Fit { page: 1 })
    );
    assert_eq!(doc.named_destination("Missing").unwrap(), None);
}

#[test]
fn malformed_named_destination_is_a_typed_error() {
    let doc = document_with_pages(1, names_catalog("Dests", 41));
    insert(
        &doc,
        41,
        Value::Dict(Dict::new().insert(
            "Names",
            arr(vec![
                Value::String("Broken".into()),
                Value::Dict(Dict::new()),
            ]),
        )),
    );
    let error = doc.named_destination("Broken").unwrap_err();
    assert!(error.to_string().contains("missing /D"), "{error}");
}

#[test]
fn named_pages_resolve_page_references() {
    let doc = document_with_pages(3, names_catalog("Pages", 45));
    insert(
        &doc,
        45,
        Value::Dict(Dict::new().insert(
            "Names",
            arr(vec![
                Value::String("Cover".into()),
                Value::Ref(12, 0),
                Value::String("Broken".into()),
                Value::Dict(Dict::new().insert("Type", name("Page"))),
            ]),
        )),
    );
    assert_eq!(doc.named_page("Cover").unwrap(), Some(2));
    assert_eq!(doc.named_page("Missing").unwrap(), None);
    let error = doc.named_page("Broken").unwrap_err();
    assert!(
        error.to_string().contains("indirect page reference"),
        "{error}"
    );
}

#[test]
fn page_labels_format_styles_prefixes_and_starts() {
    let doc = document_with_pages(10, Dict::new());
    insert(
        &doc,
        50,
        Value::Dict(Dict::new().insert(
            "Nums",
            arr(vec![
                Value::Integer(0),
                Value::Dict(
                    Dict::new()
                        .insert("S", name("D"))
                        .insert("P", Value::String("p-".into()))
                        .insert("St", Value::Integer(3)),
                ),
                Value::Integer(2),
                Value::Dict(
                    Dict::new()
                        .insert("S", name("R"))
                        .insert("St", Value::Integer(4)),
                ),
                Value::Integer(4),
                Value::Dict(Dict::new().insert("S", name("A"))),
                Value::Integer(6),
                Value::Dict(Dict::new().insert("S", name("r"))),
                Value::Integer(8),
                Value::Dict(Dict::new().insert("P", Value::String("Appendix".into()))),
            ]),
        )),
    );
    let mut catalog = Dict::new().insert("PageLabels", Value::Ref(50, 0));
    // 目录对象在构造后被替换，补入 /PageLabels。
    let doc = doc;
    catalog = catalog
        .insert("Type", name("Catalog"))
        .insert("Pages", Value::Ref(PAGES_OBJECT, 0));
    doc.set_object(Object::new(1, 0, Value::Dict(catalog)));

    let labels = doc.page_labels().unwrap().expect("page labels");
    assert_eq!(labels.page_count(), 10);
    assert_eq!(labels.label_for(0).as_deref(), Some("p-3"));
    assert_eq!(labels.label_for(1).as_deref(), Some("p-4"));
    assert_eq!(labels.label_for(2).as_deref(), Some("IV"));
    assert_eq!(labels.label_for(3).as_deref(), Some("V"));
    assert_eq!(labels.label_for(4).as_deref(), Some("A"));
    assert_eq!(labels.label_for(5).as_deref(), Some("B"));
    assert_eq!(labels.label_for(6).as_deref(), Some("i"));
    assert_eq!(labels.label_for(7).as_deref(), Some("ii"));
    assert_eq!(labels.label_for(8).as_deref(), Some("Appendix"));
    assert_eq!(labels.label_for(9).as_deref(), Some("Appendix"));
    assert_eq!(labels.label_for(10), None);
    assert_eq!(doc.page_label(2).unwrap().as_deref(), Some("IV"));
    assert_eq!(
        doc.page_label(10).unwrap_err(),
        NavigationError::PageNotFound
    );
}

#[test]
fn invalid_page_label_entries_are_typed_errors() {
    let doc = document_with_pages(2, Dict::new());
    insert(
        &doc,
        51,
        Value::Dict(Dict::new().insert(
            "Nums",
            arr(vec![
                Value::Integer(0),
                Value::Dict(Dict::new().insert("S", name("X"))),
            ]),
        )),
    );
    doc.set_object(Object::new(
        1,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Catalog"))
                .insert("Pages", Value::Ref(PAGES_OBJECT, 0))
                .insert("PageLabels", Value::Ref(51, 0)),
        ),
    ));
    let error = doc.page_labels().unwrap_err();
    assert!(
        error.to_string().contains("unknown page label style"),
        "{error}"
    );

    insert(
        &doc,
        51,
        Value::Dict(Dict::new().insert(
            "Nums",
            arr(vec![
                Value::Integer(0),
                Value::Dict(Dict::new().insert("St", Value::Integer(0))),
            ]),
        )),
    );
    let error = doc.page_labels().unwrap_err();
    assert!(
        error.to_string().contains("/St must be at least 1"),
        "{error}"
    );
}

#[test]
fn outline_reads_titles_destinations_and_children() {
    let doc = document_with_pages(2, Dict::new().insert("Outlines", Value::Ref(60, 0)));
    insert(
        &doc,
        60,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Outlines"))
                .insert("First", Value::Ref(61, 0))
                .insert("Last", Value::Ref(62, 0))
                .insert("Count", Value::Integer(2)),
        ),
    );
    insert(
        &doc,
        61,
        Value::Dict(
            Dict::new()
                .insert("Title", Value::HexString(hex_of("封面 Ω")))
                .insert("Parent", Value::Ref(60, 0))
                .insert("Next", Value::Ref(62, 0))
                .insert("Dest", arr(vec![page_ref(0), name("Fit")]))
                .insert("First", Value::Ref(63, 0))
                .insert("Last", Value::Ref(63, 0))
                .insert("Count", Value::Integer(1))
                .insert(
                    "C",
                    arr(vec![
                        Value::Integer(1),
                        Value::Float(0.5),
                        Value::Integer(0),
                    ]),
                )
                .insert("F", Value::Integer(3)),
        ),
    );
    insert(
        &doc,
        62,
        Value::Dict(
            Dict::new()
                .insert("Title", Value::String("Chapter 1".into()))
                .insert("Parent", Value::Ref(60, 0))
                .insert("Prev", Value::Ref(61, 0))
                .insert("Dest", name("Chapter1")),
        ),
    );
    insert(
        &doc,
        63,
        Value::Dict(
            Dict::new()
                .insert("Title", Value::String("Section".into()))
                .insert("Parent", Value::Ref(61, 0)),
        ),
    );

    let outline = doc.outline().unwrap().expect("outline");
    assert_eq!(outline.items.len(), 2);
    let first = &outline.items[0];
    assert_eq!(first.title, "封面 Ω");
    assert_eq!(
        first.destination,
        Some(OutlineDestination::Explicit(Destination::Fit { page: 0 }))
    );
    assert_eq!(first.color, Some([1.0, 0.5, 0.0]));
    assert_eq!(first.flags, 3);
    assert_eq!(first.count, 1);
    assert_eq!(first.children.len(), 1);
    assert_eq!(first.children[0].title, "Section");
    assert!(first.action.is_none());

    let second = &outline.items[1];
    assert_eq!(second.title, "Chapter 1");
    assert_eq!(
        second.destination,
        Some(OutlineDestination::Named("Chapter1".into()))
    );
    assert_eq!(second.flags, 0);
    assert!(second.children.is_empty());
}

#[test]
fn outline_rejects_cycles_and_missing_titles() {
    let doc = document_with_pages(1, Dict::new().insert("Outlines", Value::Ref(70, 0)));
    insert(
        &doc,
        70,
        Value::Dict(Dict::new().insert("First", Value::Ref(71, 0))),
    );
    insert(
        &doc,
        71,
        Value::Dict(
            Dict::new()
                .insert("Title", Value::String("Loop".into()))
                .insert("Next", Value::Ref(71, 0)),
        ),
    );
    let error = doc.outline().unwrap_err();
    assert!(error.to_string().contains("cycle"), "{error}");

    let doc = document_with_pages(1, Dict::new().insert("Outlines", Value::Ref(72, 0)));
    insert(
        &doc,
        72,
        Value::Dict(Dict::new().insert("First", Value::Ref(73, 0))),
    );
    insert(&doc, 73, Value::Dict(Dict::new()));
    let error = doc.outline().unwrap_err();
    assert!(error.to_string().contains("missing /Title"), "{error}");
}

#[test]
fn outline_is_absent_without_catalog_entry() {
    let doc = document_with_pages(1, Dict::new());
    assert_eq!(doc.outline().unwrap(), None);
    assert_eq!(doc.page_labels().unwrap(), None);
}

#[test]
fn page_tree_index_maps_indirect_pages_and_detects_cycles() {
    let doc = document_with_pages(3, Dict::new());
    assert_eq!(doc.page_count().unwrap(), 3);
    let index = page_tree_index(&doc).unwrap();
    assert_eq!(index.len(), 3);
    assert_eq!(index.index_of(PAGE_BASE, 0), Some(0));
    assert_eq!(index.index_of(PAGE_BASE + 2, 0), Some(2));
    assert_eq!(index.index_of(99, 0), None);

    let mut cyclic = Document::new();
    insert(
        &cyclic,
        1,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Catalog"))
                .insert("Pages", Value::Ref(PAGES_OBJECT, 0)),
        ),
    );
    insert(
        &cyclic,
        2,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Pages"))
                .insert("Kids", arr(vec![Value::Ref(PAGES_OBJECT, 0)])),
        ),
    );
    cyclic.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(1, 0)),
    };
    let error = page_tree_index(&cyclic).unwrap_err();
    assert!(error.to_string().contains("cycle"), "{error}");
}
