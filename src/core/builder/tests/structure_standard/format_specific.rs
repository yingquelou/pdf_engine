//! 格式专用属性所有者（14.8.5.2 Table 341）写端形态：`FormatSpecificOwner`
//! 的名字映射与 `/O` 写出（调用方同名条目由所有者覆盖）、经元素直接属性与
//! `/ClassMap` 两条通道解析回读后均登记 `StandardStructureAttributes` 画像。

use super::*;

#[test]
fn format_specific_attributes_encode_owner_and_entries() {
    let owners = [
        (FormatSpecificOwner::Xml1_00, "XML-1.00"),
        (FormatSpecificOwner::Html3_20, "HTML-3.20"),
        (FormatSpecificOwner::Html4_01, "HTML-4.01"),
        (FormatSpecificOwner::Oeb1_00, "OEB-1.00"),
        (FormatSpecificOwner::Rtf1_05, "RTF-1.05"),
        (FormatSpecificOwner::Css1_00, "CSS-1.00"),
        (FormatSpecificOwner::Css2_00, "CSS-2.00"),
    ];
    for (owner, name) in owners {
        assert_eq!(owner.name(), name);
        let attribute = StructureAttribute::format_specific(
            FormatSpecificAttributes::new(owner)
                .with_entry("role", Value::String("note".into()))
                .with_entry("level", Value::Integer(2)),
        );
        let dict = attribute.value.as_dict().expect("attribute dictionary");
        assert_eq!(dict.get("O").and_then(Value::as_name), Some(name));
        assert_eq!(dict.get("role"), Some(&Value::String("note".into())));
        assert_eq!(dict.get("level"), Some(&Value::Integer(2)));
    }

    // 调用方 entries 里的 /O 与同名条目都被最后写入的所有者覆盖。
    let attribute = StructureAttribute::format_specific(
        FormatSpecificAttributes::new(FormatSpecificOwner::Css2_00)
            .with_entry("O", Value::Name("Bogus".into()))
            .with_entry("display", Value::String("block".into()))
            .with_entry("O", Value::Name("StillBogus".into())),
    );
    let dict = attribute.value.as_dict().expect("attribute dictionary");
    assert_eq!(dict.get("O").and_then(Value::as_name), Some("CSS-2.00"));
    assert_eq!(dict.get("display"), Some(&Value::String("block".into())));
}

#[test]
fn format_specific_attributes_are_profiled() {
    let tree = StructureTree::new().with_element(
        StructureElement::new("Div")
            .with_id("box")
            .with_attribute(StructureAttribute::format_specific(
                FormatSpecificAttributes::new(FormatSpecificOwner::Html3_20)
                    .with_entry("align", Value::String("left".into())),
            ))
            .with_marked_content(0, 0),
    );
    let mut b = one_page(PdfVersion::V1_4);
    b.set_structure_tree(tree);

    let doc = build_and_parse(&mut b);
    // 懒加载：先触发结构树解析，属性对象才会被扫描进特性画像。
    doc.structure_tree()
        .expect("structure tree")
        .expect("tree present");
    let features = reader_features(&doc);
    assert!(
        features.contains(&PdfFeature::StandardStructureAttributes),
        "{features:?}"
    );
}

#[test]
fn format_specific_class_map_attributes_are_profiled() {
    let tree = StructureTree::new()
        .with_class(
            "css",
            Value::Dict(
                Dict::new()
                    .insert("O", Value::Name("CSS-1.00".into()))
                    .insert("display", Value::String("block".into())),
            ),
        )
        .with_element(
            StructureElement::new("Div").with_element(StructureElement::new("P").with_class("css")),
        );
    let mut b = one_page(PdfVersion::V1_4);
    b.set_structure_tree(tree);

    let doc = build_and_parse(&mut b);
    let tree = doc
        .structure_tree()
        .expect("structure tree")
        .expect("tree present");
    let paragraph = tree
        .standard_attributes(1)
        .expect("paragraph standard attributes");
    assert_eq!(paragraph.format_specific.len(), 1);
    assert_eq!(
        paragraph.format_specific[0].owner,
        crate::core::reader::structure::FormatSpecificOwner::Css1_00
    );

    let features = reader_features(&doc);
    assert!(
        features.contains(&PdfFeature::StandardStructureAttributes),
        "{features:?}"
    );
}
