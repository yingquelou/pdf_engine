//! 格式专用 owner（XML/CSS 等）属性读端测试（ISO 32000-1 14.8.5.7）。

use super::*;

#[test]
fn format_specific_owners_cover_table341() {
    for (name, expected) in [
        ("XML-1.00", FormatSpecificOwner::Xml1_00),
        ("HTML-3.20", FormatSpecificOwner::Html3_20),
        ("HTML-4.01", FormatSpecificOwner::Html4_01),
        ("OEB-1.00", FormatSpecificOwner::Oeb1_00),
        ("RTF-1.05", FormatSpecificOwner::Rtf1_05),
        ("CSS-1.00", FormatSpecificOwner::Css1_00),
        ("CSS-2.00", FormatSpecificOwner::Css2_00),
    ] {
        assert_eq!(FormatSpecificOwner::from_name(name), Some(expected));
        assert_eq!(expected.as_str(), name);
    }
    assert_eq!(FormatSpecificOwner::from_name("Vendor"), None);
}

#[test]
fn format_specific_attributes_are_exposed_with_raw_entries() {
    let stylesheet = Dict::new()
        .insert("O", name("CSS-2.00"))
        .insert("text-align", Value::String("left".into()))
        .insert("margin", Value::Integer(4));
    let user_properties = Dict::new().insert("O", name("UserProperties")).insert(
        "P",
        arr(vec![Value::Dict(
            Dict::new()
                .insert("N", Value::String("role".into()))
                .insert("V", Value::String("caption".into())),
        )]),
    );
    let doc = document_with_root(
        struct_root(Dict::new())
            .insert(
                "ClassMap",
                Value::Dict(Dict::new().insert("CssClass", Value::Dict(stylesheet))),
            )
            .insert("K", arr(vec![Value::Ref(5, 0), Value::Ref(6, 0)])),
    );
    insert(
        &doc,
        5,
        Value::Dict(element("Figure").insert(
            "A",
            arr(vec![
                Value::Dict(
                    Dict::new()
                        .insert("O", name("XML-1.00"))
                        .insert("role", Value::String("caption".into()))
                        .insert("level", Value::Integer(2)),
                ),
                Value::Dict(user_properties),
                Value::Dict(
                    Dict::new()
                        .insert("O", name("Layout"))
                        .insert("SpaceBefore", Value::Integer(12)),
                ),
            ]),
        )),
    );
    insert(
        &doc,
        6,
        Value::Dict(element("Figure").insert("C", name("CssClass")).insert(
            "A",
            arr(vec![Value::Dict(
                Dict::new()
                    .insert("O", name("CSS-2.00"))
                    .insert("margin", Value::Integer(8)),
            )]),
        )),
    );
    let tree = doc
        .structure_tree()
        .expect("structure tree")
        .expect("tree present");

    let own = tree.standard_attributes(0).expect("standard attributes");
    assert!(
        own.layout
            .as_ref()
            .is_some_and(|layout| layout.space_before == Some(12.0)),
        "Layout 仍进入类型化字段"
    );
    assert_eq!(
        own.format_specific.len(),
        1,
        "XML-1.00 进入 format_specific"
    );
    let xml = &own.format_specific[0];
    assert_eq!(xml.owner, FormatSpecificOwner::Xml1_00);
    assert_eq!(xml.revision, 0);
    let dict = xml.dict().expect("raw dictionary");
    assert_eq!(
        dict.get("role"),
        Some(&Value::String("caption".into())),
        "自定义条目原样保留"
    );
    assert_eq!(dict.get("level"), Some(&Value::Integer(2)));
    assert_eq!(own.other.len(), 1, "UserProperties 仍归入 other");
    assert_eq!(own.other[0].owner, "UserProperties");

    let classed = tree.standard_attributes(1).expect("standard attributes");
    assert!(classed.layout.is_none(), "元素自身没有 Layout 属性对象");
    assert_eq!(
        classed.format_specific.len(),
        1,
        "类与 /A 的同一 CSS-2.00 合并为单项"
    );
    let css = &classed.format_specific[0];
    assert_eq!(css.owner, FormatSpecificOwner::Css2_00);
    let dict = css.dict().expect("raw dictionary");
    assert_eq!(
        dict.get("margin"),
        Some(&Value::Integer(8)),
        "元素 /A 的键优先于类属性"
    );
    assert_eq!(
        dict.get("text-align"),
        Some(&Value::String("left".into())),
        "类属性其余键保留"
    );
    assert!(classed.other.is_empty());
}

#[test]
fn format_specific_attributes_are_not_inherited() {
    let doc = document_with_root(struct_root(Dict::new()).insert("K", arr(vec![Value::Ref(5, 0)])));
    insert(
        &doc,
        5,
        Value::Dict(
            element("Div")
                .insert(
                    "A",
                    arr(vec![Value::Dict(
                        Dict::new()
                            .insert("O", name("XML-1.00"))
                            .insert("role", Value::String("caption".into())),
                    )]),
                )
                .insert("K", arr(vec![Value::Ref(6, 0)])),
        ),
    );
    insert(
        &doc,
        6,
        Value::Dict(element("P").insert("P", Value::Ref(5, 0))),
    );
    let tree = doc
        .structure_tree()
        .expect("structure tree")
        .expect("tree present");

    assert!(
        tree.standard_attributes(0)
            .expect("parent attributes")
            .format_specific
            .len()
            == 1,
        "父元素自身报告 XML-1.00"
    );
    assert!(
        tree.effective_standard_attributes(1)
            .expect("child effective attributes")
            .format_specific
            .is_empty(),
        "格式专用属性不参与 14.8.5.3 继承"
    );
    assert!(
        tree.effective_attributes(1)
            .iter()
            .all(|attribute| attribute.owner != "XML-1.00"),
        "子元素 effective_attributes 不含 XML-1.00 所有者"
    );
}
