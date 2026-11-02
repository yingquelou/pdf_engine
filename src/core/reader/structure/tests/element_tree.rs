//! 结构树元素条目、内容项与 RoleMap/ClassMap 解析测试（14.7.1–14.7.4.2）。

use crate::core::reader::structure::StructureKid;

use super::*;

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
    assert_eq!(first.attributes[0].owner, "Layout");
    assert_eq!(first.attributes[0].revision, None);
    assert!(first.attributes[0].user_properties.is_none());
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
    let attributes = tree.root().class_map.get("Cls").expect("class map entry");
    assert_eq!(attributes.len(), 1);
    assert_eq!(attributes[0].owner, "Layout");
    assert_eq!(
        attributes[0].value.as_dict().and_then(|dict| dict.get("O")),
        Some(&name("Layout"))
    );
}
