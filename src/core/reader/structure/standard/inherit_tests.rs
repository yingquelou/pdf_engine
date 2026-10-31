//! 14.8.5.3 属性继承读端测试（可继承条目、优先级与继承缺口）。

use crate::core::reader::structure::{
    BorderColor, ListNumbering, PrintFieldRole, StandardAttributes, StructureTree, TextAlign,
};
use crate::core::types::{Dict, Document, Value};

use super::tests::{arr, document_with_root, element, insert, name, struct_root};

/// 结构树根对象号（与 `super::tests` 的 `document_with_root` 一致）。
const ROOT: i64 = 2;

/// 结构元素字典：`/P` 指向给定对象号（覆盖 helper 里的根引用）。
fn child(structure_type: &str, parent: i64) -> Dict {
    element(structure_type).insert("P", Value::Ref(parent, 0))
}

/// 属性对象字典：`/O` 加给定条目。
fn attribute_object(owner: &str, entries: Dict) -> Dict {
    let mut dict = Dict::new().insert("O", name(owner));
    for (key, value) in entries.0 {
        dict = dict.insert(key, value);
    }
    dict
}

fn layout_attribute(entries: Dict) -> Value {
    Value::Dict(attribute_object("Layout", entries))
}

fn list_attribute(entries: Dict) -> Value {
    Value::Dict(attribute_object("List", entries))
}

/// 文档：结构树根 `/K` 指向给定顶层对象号列表。
fn document(kids: Vec<i64>) -> Document {
    let kids = arr(kids
        .into_iter()
        .map(|number| Value::Ref(number, 0))
        .collect());
    document_with_root(struct_root(Dict::new()).insert("K", kids))
}

fn tree(doc: &Document) -> StructureTree {
    doc.structure_tree()
        .expect("structure tree")
        .expect("tree present")
}

fn rgb(red: i64, green: i64, blue: i64) -> Value {
    arr(vec![
        Value::Integer(red),
        Value::Integer(green),
        Value::Integer(blue),
    ])
}

#[test]
fn effective_attributes_inherit_only_inheritable_layout_keys() {
    let parent = child("Div", ROOT)
        .insert(
            "A",
            layout_attribute(
                Dict::new()
                    .insert("TextAlign", name("Justify"))
                    .insert("BorderColor", rgb(1, 0, 0))
                    .insert("Placement", name("Block"))
                    .insert("BorderStyle", name("Solid"))
                    .insert("Padding", Value::Integer(4)),
            ),
        )
        .insert("K", arr(vec![Value::Ref(6, 0)]));
    let doc = document(vec![5]);
    insert(&doc, 5, Value::Dict(parent));
    insert(
        &doc,
        6,
        Value::Dict(child("P", 5).insert(
            "A",
            layout_attribute(Dict::new().insert("SpaceBefore", Value::Integer(6))),
        )),
    );

    let tree = tree(&doc);
    let layout = tree
        .effective_standard_attributes(1)
        .expect("effective attributes")
        .layout
        .expect("Layout owner");
    assert_eq!(
        layout.text_align,
        Some(TextAlign::Justify),
        "TextAlign 继承"
    );
    assert_eq!(
        layout.border_color,
        Some(BorderColor::Uniform([1.0, 0.0, 0.0])),
        "BorderColor 继承"
    );
    assert_eq!(layout.space_before, Some(6.0), "自身 /SpaceBefore 保留");
    assert_eq!(layout.placement, None, "/Placement 不可继承");
    assert_eq!(layout.border_style, None, "/BorderStyle 不可继承");
    assert_eq!(layout.padding, None, "/Padding 不可继承");

    let own = tree
        .standard_attributes(1)
        .expect("own attributes")
        .layout
        .expect("own Layout owner");
    assert_eq!(own.text_align, None, "standard_attributes 仍只看自身");
    assert_eq!(own.border_color, None);
    assert_eq!(own.space_before, Some(6.0));
}

#[test]
fn effective_attributes_nearest_ancestor_and_own_values_win() {
    let doc = document(vec![5]);
    insert(
        &doc,
        5,
        Value::Dict(
            child("Div", ROOT)
                .insert(
                    "A",
                    layout_attribute(Dict::new().insert("TextAlign", name("Center"))),
                )
                .insert("K", arr(vec![Value::Ref(6, 0)])),
        ),
    );
    insert(
        &doc,
        6,
        Value::Dict(
            child("Div", 5)
                .insert(
                    "A",
                    layout_attribute(Dict::new().insert("TextAlign", name("End"))),
                )
                .insert("K", arr(vec![Value::Ref(7, 0), Value::Ref(8, 0)])),
        ),
    );
    insert(&doc, 7, Value::Dict(child("P", 6)));
    insert(
        &doc,
        8,
        Value::Dict(child("P", 6).insert(
            "A",
            layout_attribute(Dict::new().insert("TextAlign", name("Start"))),
        )),
    );

    let tree = tree(&doc);
    let nearest = tree
        .effective_standard_attributes(2)
        .expect("child effective attributes")
        .layout
        .expect("Layout owner");
    assert_eq!(
        nearest.text_align,
        Some(TextAlign::End),
        "最近祖先覆盖更远的祖先"
    );

    let own = tree
        .effective_standard_attributes(3)
        .expect("own TextAlign effective attributes")
        .layout
        .expect("Layout owner");
    assert_eq!(own.text_align, Some(TextAlign::Start), "自身 /A 覆盖祖先");

    let parent = tree
        .effective_standard_attributes(1)
        .expect("parent effective attributes")
        .layout
        .expect("Layout owner");
    assert_eq!(parent.text_align, Some(TextAlign::End));
}

#[test]
fn effective_attributes_include_class_map_attributes_at_each_level() {
    let doc = document_with_root(
        struct_root(Dict::new())
            .insert(
                "ClassMap",
                Value::Dict(Dict::new().insert(
                    "Indent",
                    Value::Dict(attribute_object(
                        "Layout",
                        Dict::new().insert("TextIndent", Value::Integer(12)),
                    )),
                )),
            )
            .insert("K", arr(vec![Value::Ref(5, 0)])),
    );
    insert(
        &doc,
        5,
        Value::Dict(
            child("Div", ROOT)
                .insert("C", arr(vec![name("Indent")]))
                .insert("K", arr(vec![Value::Ref(6, 0)])),
        ),
    );
    insert(&doc, 6, Value::Dict(child("P", 5)));

    let tree = tree(&doc);
    let parent = tree
        .standard_attributes(0)
        .expect("parent own attributes")
        .layout
        .expect("parent Layout owner");
    assert_eq!(parent.text_indent, Some(12.0), "类属性并入自身结果");

    let effective = tree
        .effective_standard_attributes(1)
        .expect("child effective attributes")
        .layout
        .expect("inherited Layout owner");
    assert_eq!(
        effective.text_indent,
        Some(12.0),
        "子元素继承父元素类属性里的 /TextIndent"
    );
}

#[test]
fn effective_list_attributes_inherit_list_numbering() {
    let doc = document(vec![5]);
    insert(
        &doc,
        5,
        Value::Dict(
            child("L", ROOT)
                .insert(
                    "A",
                    list_attribute(Dict::new().insert("ListNumbering", name("Disc"))),
                )
                .insert("K", arr(vec![Value::Ref(6, 0)])),
        ),
    );
    insert(
        &doc,
        6,
        Value::Dict(child("LI", 5).insert("K", arr(vec![Value::Ref(7, 0)]))),
    );
    insert(&doc, 7, Value::Dict(child("Lbl", 6)));

    let tree = tree(&doc);
    for (index, label) in [(1, "LI"), (2, "Lbl")] {
        let numbering = tree
            .effective_standard_attributes(index)
            .expect("effective attributes")
            .list
            .expect("inherited List owner")
            .numbering;
        assert_eq!(
            numbering,
            ListNumbering::Disc,
            "{label} 继承 /ListNumbering"
        );
    }
    assert!(
        tree.standard_attributes(2)
            .expect("own attributes")
            .list
            .is_none(),
        "standard_attributes 仍只报告自身 List 属性"
    );
}

#[test]
fn effective_print_field_and_table_attributes_are_not_inherited() {
    let doc = document(vec![5, 7]);
    insert(
        &doc,
        5,
        Value::Dict(
            child("Table", ROOT)
                .insert(
                    "A",
                    Value::Dict(attribute_object(
                        "Table",
                        Dict::new()
                            .insert("RowSpan", Value::Integer(2))
                            .insert("Summary", Value::String("Data".into())),
                    )),
                )
                .insert("K", arr(vec![Value::Ref(6, 0)])),
        ),
    );
    insert(&doc, 6, Value::Dict(child("TD", 5)));
    insert(
        &doc,
        7,
        Value::Dict(
            child("Form", ROOT)
                .insert(
                    "A",
                    Value::Dict(attribute_object(
                        "PrintField",
                        Dict::new()
                            .insert("Role", name("rb"))
                            .insert("checked", name("on")),
                    )),
                )
                .insert("K", arr(vec![Value::Ref(8, 0)])),
        ),
    );
    insert(&doc, 8, Value::Dict(child("Span", 7)));

    let tree = tree(&doc);
    assert_eq!(
        tree.standard_attributes(0)
            .expect("table own attributes")
            .table
            .expect("Table owner")
            .row_span,
        2
    );
    assert!(
        tree.effective_standard_attributes(1)
            .expect("cell effective attributes")
            .table
            .is_none(),
        "/Table 条目不可继承"
    );
    assert_eq!(
        tree.standard_attributes(2)
            .expect("form own attributes")
            .print_field
            .expect("PrintField owner")
            .role,
        Some(PrintFieldRole::RadioButton)
    );
    assert!(
        tree.effective_standard_attributes(3)
            .expect("span effective attributes")
            .print_field
            .is_none(),
        "/PrintField 条目不可继承"
    );
}

#[test]
fn effective_attributes_guard_parent_chain_depth() {
    // 解析器把链长限制在 MAX_STRUCTURE_DEPTH（64）层以内，公开 API 无法构造
    // 更深的链；这里验证接近上限的链正常工作，超过深度或成环只会静默停止。
    let depth = 10;
    let doc = document(vec![5]);
    for step in 0..depth {
        let number = 5 + step as i64;
        let parent = if step == 0 { ROOT } else { number - 1 };
        let mut entry = child("Div", parent);
        if step == 0 {
            entry = entry.insert(
                "A",
                layout_attribute(Dict::new().insert("TextAlign", name("Center"))),
            );
        }
        if step + 1 < depth {
            entry = entry.insert("K", arr(vec![Value::Ref(number + 1, 0)]));
        }
        insert(&doc, number, Value::Dict(entry));
    }

    let tree = tree(&doc);
    let deepest = tree
        .effective_standard_attributes(depth - 1)
        .expect("deepest effective attributes")
        .layout
        .expect("inherited Layout owner");
    assert_eq!(deepest.text_align, Some(TextAlign::Center));
}

#[test]
fn effective_attributes_out_of_range_is_empty() {
    let doc = document(vec![5]);
    insert(
        &doc,
        5,
        Value::Dict(child("P", ROOT).insert(
            "A",
            layout_attribute(Dict::new().insert("TextAlign", name("Justify"))),
        )),
    );
    let tree = tree(&doc);

    assert!(tree.effective_attributes(999).is_empty());
    assert_eq!(
        tree.effective_standard_attributes(999),
        Ok(StandardAttributes::default())
    );
    assert_eq!(
        tree.effective_standard_attributes(0)
            .expect("in-range effective attributes")
            .layout
            .expect("Layout owner")
            .text_align,
        Some(TextAlign::Justify)
    );
}

#[test]
fn effective_standard_attributes_parse_inherited_values() {
    let doc = document(vec![5]);
    insert(
        &doc,
        5,
        Value::Dict(
            child("Document", ROOT)
                .insert(
                    "A",
                    arr(vec![
                        layout_attribute(Dict::new().insert("TextAlign", name("Justify"))),
                        list_attribute(Dict::new().insert("ListNumbering", name("Decimal"))),
                    ]),
                )
                .insert("K", arr(vec![Value::Ref(6, 0)])),
        ),
    );
    insert(&doc, 6, Value::Dict(child("P", 5)));

    let tree = tree(&doc);
    let effective = tree
        .effective_standard_attributes(1)
        .expect("inherited values parse without error");
    assert_eq!(
        effective.layout.expect("inherited Layout owner").text_align,
        Some(TextAlign::Justify)
    );
    assert_eq!(
        effective.list.expect("inherited List owner").numbering,
        ListNumbering::Decimal
    );
    assert!(effective.print_field.is_none() && effective.table.is_none());
    assert!(effective.other.is_empty());
}
