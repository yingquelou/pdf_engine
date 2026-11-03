//! 标准结构 List owner 属性读端测试（ISO 32000-1 14.8.5.5）。

use super::*;

#[test]
fn list_attributes_parse_numbering_and_default() {
    let doc = document_with_elements(vec![
        (
            5,
            element("L").insert(
                "A",
                Value::Dict(
                    Dict::new()
                        .insert("O", name("List"))
                        .insert("ListNumbering", name("Decimal")),
                ),
            ),
        ),
        (
            6,
            element("L").insert("A", Value::Dict(Dict::new().insert("O", name("List")))),
        ),
    ]);
    let tree = doc
        .structure_tree()
        .expect("structure tree")
        .expect("tree present");

    let first = tree.standard_attributes(0).expect("list attributes");
    let list = first.list.expect("List owner");
    assert_eq!(list.numbering, ListNumbering::Decimal);
    assert_eq!(
        list.dict.get("O").and_then(Value::as_name),
        Some("List"),
        "dict 保留原始条目"
    );
    assert!(first.print_field.is_none());
    assert!(first.table.is_none());
    assert!(first.other.is_empty());

    let second = tree.standard_attributes(1).expect("list attributes");
    assert_eq!(
        second.list.expect("List owner").numbering,
        ListNumbering::None,
        "/ListNumbering 缺省为 None"
    );

    let missing = tree.standard_attributes(99).expect("out of range");
    assert_eq!(missing, Default::default(), "越界下标返回全空属性");
}
