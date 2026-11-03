//! 标准结构属性 owner 分类、类型化错误与类映射合并测试（ISO 32000-1 14.8.5.4 Table 341、14.8.5.3）。

use super::*;

#[test]
fn standard_attribute_owners_are_classified() {
    for (name, expected) in [
        ("Layout", StandardAttributeOwner::Layout),
        ("List", StandardAttributeOwner::List),
        ("PrintField", StandardAttributeOwner::PrintField),
        ("Table", StandardAttributeOwner::Table),
        ("XML-1.00", StandardAttributeOwner::Other("XML-1.00".into())),
        (
            "HTML-3.20",
            StandardAttributeOwner::Other("HTML-3.20".into()),
        ),
        (
            "HTML-4.01",
            StandardAttributeOwner::Other("HTML-4.01".into()),
        ),
        ("OEB-1.00", StandardAttributeOwner::Other("OEB-1.00".into())),
        ("RTF-1.05", StandardAttributeOwner::Other("RTF-1.05".into())),
        ("CSS-1.00", StandardAttributeOwner::Other("CSS-1.00".into())),
        ("CSS-2.00", StandardAttributeOwner::Other("CSS-2.00".into())),
        ("Vendor", StandardAttributeOwner::Other("Vendor".into())),
    ] {
        let owner = StandardAttributeOwner::from_name(name);
        assert_eq!(owner, expected);
        assert_eq!(owner.as_str(), name);
    }
}

#[test]
fn malformed_standard_attributes_are_typed_errors() {
    let list = Dict::new()
        .insert("O", name("List"))
        .insert("ListNumbering", Value::Integer(1));
    let doc = attribute_document(list);
    assert_malformed(&doc, "structure element 0 /ListNumbering must be a name");

    let doc = attribute_document(
        Dict::new()
            .insert("O", name("PrintField"))
            .insert("checked", Value::Integer(1)),
    );
    assert_malformed(&doc, "structure element 0 /checked must be a name");

    let doc = attribute_document(
        Dict::new()
            .insert("O", name("PrintField"))
            .insert("Desc", Value::Integer(1)),
    );
    assert_malformed(&doc, "structure element 0 /Desc must be a text string");

    for span in [0, -1] {
        let doc = attribute_document(
            Dict::new()
                .insert("O", name("Table"))
                .insert("RowSpan", Value::Integer(span)),
        );
        assert_malformed(&doc, "structure element 0 /RowSpan must be an integer");
    }

    let doc = attribute_document(
        Dict::new()
            .insert("O", name("Table"))
            .insert("Headers", Value::Integer(1)),
    );
    assert_malformed(&doc, "structure element 0 /Headers must be an array");

    let doc = attribute_document(
        Dict::new()
            .insert("O", name("Table"))
            .insert("Headers", arr(vec![Value::String(String::new())])),
    );
    assert_malformed(
        &doc,
        "structure element 0 /Headers entry 0 must not be empty",
    );

    let doc = attribute_document(
        Dict::new()
            .insert("O", name("Table"))
            .insert("Headers", arr(vec![Value::Integer(1)])),
    );
    assert_malformed(
        &doc,
        "structure element 0 /Headers entry 0 must be a byte string",
    );
}

#[test]
fn standard_attributes_merge_class_and_element_objects() {
    let class_map = Dict::new().insert(
        "Body",
        Value::Dict(
            Dict::new()
                .insert("O", name("List"))
                .insert("ListNumbering", name("Disc")),
        ),
    );
    let layout = Dict::new()
        .insert("O", name("Layout"))
        .insert("SpaceBefore", Value::Integer(12));
    let element = element("L").insert("C", name("Body")).insert(
        "A",
        arr(vec![
            Value::Dict(
                Dict::new()
                    .insert("O", name("List"))
                    .insert("ListNumbering", name("Circle")),
            ),
            Value::Dict(layout),
        ]),
    );
    let doc = document_with_root(
        struct_root(Dict::new())
            .insert("ClassMap", Value::Dict(class_map))
            .insert("K", arr(vec![Value::Ref(5, 0)])),
    );
    insert(&doc, 5, Value::Dict(element));
    let tree = doc
        .structure_tree()
        .expect("structure tree")
        .expect("tree present");

    let resolved = tree.resolved_attributes(0);
    assert_eq!(resolved.len(), 2, "同一 owner 合并，Layout 保留");
    assert_eq!(resolved[0].owner, "List");

    let standard = tree.standard_attributes(0).expect("standard attributes");
    let list = standard.list.expect("List owner");
    assert_eq!(
        list.numbering,
        ListNumbering::Circle,
        "元素 /A 覆盖 ClassMap 的 /ListNumbering"
    );
    let layout = standard.layout.expect("Layout owner");
    assert_eq!(layout.space_before, Some(12.0), "Layout 对象进入类型化字段");
    assert_eq!(
        layout.dict.get("O").and_then(Value::as_name),
        Some("Layout")
    );
    assert!(standard.other.is_empty(), "Layout 不再归入 other");
}
