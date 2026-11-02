//! IDTree/ParentTree 父树条目与页面 /StructParents 映射测试（14.7.3）。

use crate::core::reader::structure::ParentTreeEntry;

use super::*;

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
