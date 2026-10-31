//! 逻辑结构树读端测试（ISO 32000-1 14.7.1–14.7.4.4）。

use crate::core::reader::structure::{ParentTreeEntry, StructureError, StructureKid};
use crate::core::types::{Array, Dict, Document, Object, Trailer, Value};

const CATALOG: i64 = 1;
const ROOT: i64 = 2;
const PAGE: i64 = 10;

fn arr(values: Vec<Value>) -> Value {
    Value::Array(Array(values))
}

fn name(value: &str) -> Value {
    Value::Name(value.to_owned())
}

fn insert(doc: &Document, num: i64, value: Value) {
    doc.set_object(Object::new(num, 0, value));
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

/// Catalog 1 → StructTreeRoot 2。
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

fn assert_malformed(doc: &Document, needle: &str) {
    match doc.structure_tree() {
        Err(StructureError::Malformed(message)) => {
            assert!(
                message.contains(needle),
                "expected {needle:?}, got {message}"
            );
        }
        other => panic!("expected malformed error containing {needle:?}, got {other:?}"),
    }
}

fn malformed_document(kid: Value) -> Document {
    let doc = document_with_root(struct_root(
        Dict::new().insert("K", arr(vec![Value::Ref(5, 0)])),
    ));
    insert(&doc, 5, kid);
    doc
}

#[test]
fn structure_tree_reads_elements_and_content_items() {
    let mcr = Dict::new()
        .insert("Type", name("MCR"))
        .insert("Pg", Value::Ref(PAGE, 0))
        .insert("Stm", Value::Ref(11, 0))
        .insert("StmOwn", Value::Ref(12, 0))
        .insert("MCID", Value::Integer(4));
    let objr = Dict::new()
        .insert("Type", name("OBJR"))
        .insert("Pg", Value::Ref(PAGE, 0))
        .insert("Obj", Value::Ref(50, 0));
    let attribute = Dict::new()
        .insert("O", name("Layout"))
        .insert("Placement", name("Block"));
    let first = element("P", ROOT)
        .insert("Pg", Value::Ref(PAGE, 0))
        .insert("ID", Value::String("elem-1".into()))
        .insert("T", Value::String("Title".into()))
        .insert("Lang", Value::String("en-US".into()))
        .insert("Alt", Value::String("alternate".into()))
        .insert("E", Value::String("expanded".into()))
        .insert("ActualText", Value::String("actual".into()))
        .insert("A", Value::Dict(attribute.clone()))
        .insert("C", arr(vec![name("Artifact"), Value::Integer(2)]))
        .insert("R", Value::Integer(3))
        .insert(
            "K",
            arr(vec![
                Value::Integer(0),
                Value::Dict(mcr.clone()),
                Value::Dict(objr.clone()),
            ]),
        )
        .insert("Custom", Value::Integer(7));
    let doc = document_with_root(struct_root(
        Dict::new().insert("K", arr(vec![Value::Ref(5, 0), Value::Ref(6, 0)])),
    ));
    insert(&doc, 5, Value::Dict(first));
    insert(&doc, 6, Value::Dict(element("Span", ROOT)));
    for number in [PAGE, 11, 12, 50] {
        insert(&doc, number, Value::Dict(Dict::new()));
    }

    let tree = doc.structure_tree().expect("structure tree").expect("root");
    assert_eq!(tree.elements().len(), 2);
    match &tree.root().kids[..] {
        [StructureKid::Element(0), StructureKid::Element(1)] => {}
        other => panic!("expected two top-level elements, got {other:?}"),
    }
    let first = tree.element(0).expect("first element");
    assert_eq!(first.object, Some((5, 0)));
    assert_eq!(first.structure_type, "P");
    assert_eq!(first.parent, None);
    assert_eq!(first.page, Some((PAGE, 0)));
    assert_eq!(first.id.as_deref(), Some(b"elem-1".as_slice()));
    assert_eq!(first.title.as_deref(), Some("Title"));
    assert_eq!(first.lang.as_deref(), Some("en-US"));
    assert_eq!(first.alt.as_deref(), Some("alternate"));
    assert_eq!(first.expanded.as_deref(), Some("expanded"));
    assert_eq!(first.actual_text.as_deref(), Some("actual"));
    assert_eq!(first.revision, 3);
    assert_eq!(
        first.dict.get("Custom"),
        Some(&Value::Integer(7)),
        "未建模条目应保留在原始字典中"
    );

    assert_eq!(first.classes.len(), 1);
    assert_eq!(first.classes[0].name, "Artifact");
    assert_eq!(first.classes[0].revision, Some(2));
    assert_eq!(first.attributes.len(), 1);
    assert_eq!(first.attributes[0].revision, None);
    assert_eq!(
        first.attributes[0].value,
        Value::Dict(attribute),
        "属性对象按解引用后的字典保存"
    );

    match &first.kids[..] {
        [
            StructureKid::Mcid(0),
            StructureKid::MarkedContent(mcr),
            StructureKid::ObjectReference(objr),
        ] => {
            assert_eq!(mcr.page, Some((PAGE, 0)));
            assert_eq!(mcr.stream, Some((11, 0)));
            assert_eq!(mcr.stream_own, Some((12, 0)));
            assert_eq!(mcr.mcid, 4);
            assert_eq!(mcr.dict.get("Type"), Some(&name("MCR")));
            assert_eq!(objr.page, Some((PAGE, 0)));
            assert_eq!(objr.object, (50, 0));
            assert_eq!(objr.dict.get("Type"), Some(&name("OBJR")));
        }
        other => panic!("expected MCID/MCR/OBJR kids, got {other:?}"),
    }

    let second = tree.element(1).expect("second element");
    assert_eq!(second.parent, None);
    assert_eq!(second.revision, 0, "/R 缺省为 0");
    assert_eq!(second.structure_type, "Span");
    assert_eq!(
        tree.element_by_object((6, 9))
            .map(|element| element.structure_type.as_str()),
        Some("Span"),
        "按对象号匹配，generation 容错"
    );
    assert_eq!(tree.element(2), None);
    assert_eq!(tree.root().dict.get("Type"), Some(&name("StructTreeRoot")));
}

#[test]
fn structure_tree_resolves_role_map_chains() {
    let role_map = Dict::new()
        .insert("A", name("B"))
        .insert("B", name("C"))
        .insert("X", name("Y"))
        .insert("Y", name("X"));
    let doc = document_with_root(struct_root(
        Dict::new().insert("RoleMap", Value::Dict(role_map)).insert(
            "ClassMap",
            Value::Dict(
                Dict::new().insert("Cls", Value::Dict(Dict::new().insert("O", name("Layout")))),
            ),
        ),
    ));

    let tree = doc.structure_tree().expect("structure tree").expect("root");
    assert_eq!(tree.resolved_role("A"), Some("C"));
    assert_eq!(tree.resolved_role("B"), Some("C"));
    assert_eq!(tree.resolved_role("C"), None);
    assert_eq!(tree.resolved_role("Missing"), None);
    assert!(
        tree.resolved_role("X").is_some(),
        "环上的解析必须终止并返回环内名字"
    );
    assert!(tree.resolved_role("Y").is_some());
    assert_eq!(tree.root().role_map.len(), 4);
    assert_eq!(tree.root().class_map.len(), 1);
    assert_eq!(
        tree.root()
            .class_map
            .get("Cls")
            .and_then(Value::as_dict)
            .and_then(|dict| dict.get("O")),
        Some(&name("Layout"))
    );
}

#[test]
fn structure_tree_id_tree_and_parent_tree_entries() {
    let root = struct_root(Dict::new())
        .insert("K", arr(vec![Value::Ref(5, 0)]))
        .insert(
            "IDTree",
            Value::Dict(Dict::new().insert(
                "Names",
                arr(vec![Value::String("id-a".into()), Value::Ref(5, 0)]),
            )),
        )
        .insert(
            "ParentTree",
            Value::Dict(Dict::new().insert(
                "Nums",
                arr(vec![
                    Value::Integer(7),
                    arr(vec![Value::Ref(5, 0), Value::Null]),
                    Value::Integer(8),
                    Value::Ref(6, 0),
                ]),
            )),
        )
        .insert("ParentTreeNextKey", Value::Integer(9));
    let doc = document_with_root(root);
    insert(
        &doc,
        5,
        Value::Dict(element("Document", ROOT).insert("K", arr(vec![Value::Ref(6, 0)]))),
    );
    insert(&doc, 6, Value::Dict(element("P", 5)));

    let tree = doc.structure_tree().expect("structure tree").expect("root");
    let by_id = tree.element_by_id(b"id-a").expect("ID tree entry");
    assert_eq!(by_id.object, Some((5, 0)));
    assert_eq!(tree.element_by_id(b"missing"), None);
    assert_eq!(tree.root().parent_tree_next_key, Some(9));

    match tree.parent_tree_entry(7) {
        Some(ParentTreeEntry::Page(parents)) => {
            assert_eq!(parents, &vec![Some(0), None]);
        }
        other => panic!("expected page parent array, got {other:?}"),
    }
    assert_eq!(tree.mcid_parent(7, 0), Some(0));
    assert_eq!(tree.mcid_parent(7, 1), None, "null 空位无父元素");
    assert_eq!(tree.mcid_parent(7, 2), None, "越界 MCID");
    assert_eq!(tree.object_parent(7), None, "页面数组型不是 OBJR 单元");
    assert_eq!(tree.object_parent(8), Some(1));
    assert_eq!(tree.mcid_parent(8, 0), None, "OBJR 单元型不是页面数组");
    assert_eq!(tree.parent_tree_entry(9), None);
}

#[test]
fn mark_info_reads_boolean_entries() {
    let mut doc = Document::new();
    insert(
        &doc,
        CATALOG,
        Value::Dict(
            Dict::new().insert("Type", name("Catalog")).insert(
                "MarkInfo",
                Value::Dict(
                    Dict::new()
                        .insert("Marked", Value::Bool(true))
                        .insert("Suspects", Value::Bool(true))
                        .insert("Custom", Value::Integer(1)),
                ),
            ),
        ),
    );
    doc.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(CATALOG, 0)),
    };

    let info = doc.mark_info().expect("mark info").expect("declared");
    assert!(info.marked);
    assert!(!info.user_properties, "缺省 false");
    assert!(info.suspects);
    assert_eq!(info.dict.get("Custom"), Some(&Value::Integer(1)));

    // 空 /MarkInfo：三个布尔条目均为缺省 false。
    insert(
        &doc,
        CATALOG,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Catalog"))
                .insert("MarkInfo", Value::Dict(Dict::new())),
        ),
    );
    let info = doc.mark_info().expect("mark info").expect("declared");
    assert!(!info.marked && !info.user_properties && !info.suspects);

    // 目录没有 /MarkInfo。
    insert(
        &doc,
        CATALOG,
        Value::Dict(Dict::new().insert("Type", name("Catalog"))),
    );
    assert_eq!(doc.mark_info(), Ok(None));

    // 非布尔条目是类型化错误。
    insert(
        &doc,
        CATALOG,
        Value::Dict(Dict::new().insert("Type", name("Catalog")).insert(
            "MarkInfo",
            Value::Dict(Dict::new().insert("Marked", Value::Integer(1))),
        )),
    );
    match doc.mark_info() {
        Err(StructureError::Malformed(message)) => {
            assert!(message.contains("must be a boolean"), "{message}");
        }
        other => panic!("expected malformed mark info, got {other:?}"),
    }
}

#[test]
fn structure_tree_reports_malformed_entries() {
    // /S 缺失（无 /Type 的字典按结构元素处理）。
    assert_malformed(
        &malformed_document(Value::Dict(Dict::new().insert("P", Value::Ref(ROOT, 0)))),
        "/S is required",
    );

    // /P 不是间接引用。
    assert_malformed(
        &malformed_document(Value::Dict(
            element("P", ROOT).insert("P", Value::Integer(ROOT)),
        )),
        "/P must be an indirect reference",
    );

    // MCR 缺 /MCID。
    let doc = malformed_document(Value::Dict(
        element("P", ROOT).insert("Pg", Value::Ref(PAGE, 0)).insert(
            "K",
            arr(vec![Value::Dict(
                Dict::new()
                    .insert("Type", name("MCR"))
                    .insert("Pg", Value::Ref(PAGE, 0)),
            )]),
        ),
    ));
    insert(&doc, PAGE, Value::Dict(Dict::new()));
    assert_malformed(&doc, "/MCID is required");

    // MCR 无 /Pg 且父元素无 /Pg。
    assert_malformed(
        &malformed_document(Value::Dict(element("P", ROOT).insert(
            "K",
            arr(vec![Value::Dict(
                Dict::new()
                    .insert("Type", name("MCR"))
                    .insert("MCID", Value::Integer(1)),
            )]),
        ))),
        "parent structure element has no /Pg",
    );

    // 未知 /K 字典类型。
    assert_malformed(
        &malformed_document(Value::Dict(element("P", ROOT).insert(
            "K",
            arr(vec![Value::Dict(Dict::new().insert("Type", name("Bogus")))]),
        ))),
        "unknown /Type",
    );

    // /P 指向了错误的父元素。
    let doc = malformed_document(Value::Dict(
        element("P", ROOT).insert("K", arr(vec![Value::Ref(6, 0)])),
    ));
    insert(&doc, 6, Value::Dict(element("Span", 99)));
    insert(&doc, 99, Value::Dict(Dict::new()));
    assert_malformed(&doc, "must reference its parent object 5");

    // /K 内嵌数组。
    assert_malformed(
        &malformed_document(Value::Dict(
            element("P", ROOT).insert("K", arr(vec![arr(vec![Value::Integer(0)])])),
        )),
        "nested /K arrays",
    );
}

#[test]
fn structure_tree_limits_are_enforced() {
    let doc = document_with_root(struct_root(
        Dict::new().insert("K", arr(vec![Value::Ref(100, 0)])),
    ));
    for index in 0..65 {
        let number = 100 + index;
        let parent = if index == 0 { ROOT } else { number - 1 };
        let mut entry = element("P", parent);
        if index + 1 < 65 {
            entry = entry.insert("K", arr(vec![Value::Ref(number + 1, 0)]));
        }
        insert(&doc, number, Value::Dict(entry));
    }

    match doc.structure_tree() {
        Err(StructureError::LimitExceeded(what)) => {
            assert_eq!(what, "structure tree depth");
        }
        other => panic!("expected depth limit error, got {other:?}"),
    }
}

#[test]
fn page_struct_parents_maps_page_mcids() {
    let build = |page: Dict| {
        let mut doc = Document::new();
        insert(
            &doc,
            3,
            Value::Dict(
                Dict::new()
                    .insert("Type", name("Pages"))
                    .insert("Kids", arr(vec![Value::Ref(5, 0)]))
                    .insert("Count", Value::Integer(1)),
            ),
        );
        insert(&doc, 5, Value::Dict(page));
        insert(
            &doc,
            6,
            Value::Dict(element("P", 4).insert("Pg", Value::Ref(5, 0))),
        );
        insert(
            &doc,
            4,
            Value::Dict(
                struct_root(Dict::new())
                    .insert("K", arr(vec![Value::Ref(6, 0)]))
                    .insert(
                        "ParentTree",
                        Value::Dict(Dict::new().insert(
                            "Nums",
                            arr(vec![
                                Value::Integer(7),
                                arr(vec![Value::Ref(6, 0), Value::Null]),
                            ]),
                        )),
                    ),
            ),
        );
        insert(
            &doc,
            CATALOG,
            Value::Dict(
                Dict::new()
                    .insert("Type", name("Catalog"))
                    .insert("Pages", Value::Ref(3, 0))
                    .insert("StructTreeRoot", Value::Ref(4, 0)),
            ),
        );
        doc.trailer = Trailer {
            dict: Dict::new().insert("Root", Value::Ref(CATALOG, 0)),
        };
        doc
    };
    let page = Dict::new()
        .insert("Type", name("Page"))
        .insert("Parent", Value::Ref(3, 0))
        .insert("StructParents", Value::Integer(7));
    let doc = build(page);

    let parents = doc
        .page_struct_parents(0)
        .expect("page parents")
        .expect("declared");
    assert_eq!(parents.key, 7);
    assert_eq!(parents.parents, vec![Some(0), None]);
    assert!(
        matches!(
            doc.page_struct_parents(1),
            Err(StructureError::Malformed(_))
        ),
        "页码越界应为 Malformed"
    );

    // 页面没有 /StructParents。
    let doc = build(
        Dict::new()
            .insert("Type", name("Page"))
            .insert("Parent", Value::Ref(3, 0)),
    );
    assert_eq!(doc.page_struct_parents(0), Ok(None));

    // 父树没有该键。
    let doc = build(
        Dict::new()
            .insert("Type", name("Page"))
            .insert("Parent", Value::Ref(3, 0))
            .insert("StructParents", Value::Integer(8)),
    );
    match doc.page_struct_parents(0) {
        Err(StructureError::Malformed(message)) => {
            assert!(message.contains("parent tree has no entry"), "{message}");
        }
        other => panic!("expected missing parent tree entry, got {other:?}"),
    }
}
