//! 属性对象与 `/ClassMap`（14.7.5，Table 327）：`/R` `/C` 修订号写出与缺省
//! 省略、类映射流的间接对象形态、属性解析往返、数组形式类映射与属性数组
//! 修订号语义。

use super::super::*;
use super::support::{
    assert_invalid, attribute_tree, build_and_parse, layout_dict, name, one_page,
};
use crate::core::types::{Array, PdfFeature, PdfVersion};

#[test]
fn structure_revision_and_class_revisions_roundtrip() {
    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document")
                .with_class_revision("Body", u32::MAX)
                .with_class("Inline")
                .with_revision(u32::MAX),
        ),
    );
    let doc = build_and_parse(&mut b);
    let tree = doc.structure_tree().expect("tree").expect("present");
    let element = &tree.elements()[0];
    assert_eq!(element.revision, u32::MAX);
    assert_eq!(element.classes.len(), 2);
    assert_eq!(element.classes[0].name, "Body");
    assert_eq!(element.classes[0].revision, Some(u32::MAX));
    assert_eq!(element.classes[1].name, "Inline");
    assert_eq!(element.classes[1].revision, None);

    // 修订号 0 为缺省，不写出 `/R`。
    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(StructureTree::new().with_element(StructureElement::new("Document")));
    let mut out = Vec::new();
    b.build(&mut out).expect("build");
    assert!(!String::from_utf8_lossy(&out).contains("/R "));
}

#[test]
fn structure_class_map_stream_is_written_indirectly() {
    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(StructureTree::new().with_class(
        "Big",
        Value::Stream(
            Dict::new().insert("O", Value::Name("Layout".into())),
            StreamData::Owned(b"layout attributes".to_vec()),
        ),
    ));
    let mut out = Vec::new();
    b.build(&mut out).expect("build");
    let text = String::from_utf8_lossy(&out);
    assert!(text.contains("/ClassMap << /Big "), "{text}");
    assert!(
        text.contains("endstream"),
        "类映射流应作为间接对象写出: {text}"
    );

    let doc = build_and_parse(&mut b);
    let tree = doc.structure_tree().expect("tree").expect("present");
    let attribute = tree
        .root()
        .class_map
        .get("Big")
        .and_then(|attributes| attributes.first())
        .expect("类映射流应为单个属性对象");
    assert_eq!(attribute.owner, "Layout");
    assert!(
        matches!(&attribute.value, Value::Stream(..)),
        "类映射流解析回解引用后的流值"
    );
    let bytes = doc.stream_bytes(&attribute.value).expect("stream bytes");
    assert_eq!(bytes, b"layout attributes");
}

#[test]
fn structure_attributes_roundtrip_through_parser() {
    let class_layout = layout_dict(12).insert("Placement", name("Block"));
    let class_user = StructureAttribute::user_properties(vec![
        UserProperty::new("role", Value::String("caption".into()))
            .with_formatted("Caption")
            .with_hidden(true),
    ]);
    let tree = StructureTree::new()
        .with_class(
            "Body",
            Value::Array(Array(vec![Value::Dict(class_layout), class_user.value])),
        )
        .with_element(
            StructureElement::new("Document")
                .with_class_revision("Body", 2)
                .with_attribute(
                    StructureAttribute::new(Value::Dict(
                        layout_dict(24).insert("InlineAlign", name("Center")),
                    ))
                    .with_revision(3),
                )
                .with_marked_content(0, 0),
        );
    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(tree);
    b.set_mark_info(
        StructureMarkInfo::new()
            .with_marked(true)
            .with_user_properties(true),
    );

    let doc = build_and_parse(&mut b);
    let tree = doc.structure_tree().expect("tree").expect("present");
    let class_attributes = tree.root().class_map.get("Body").expect("class map");
    assert_eq!(class_attributes.len(), 2, "类映射数组保留两个属性对象");
    assert_eq!(class_attributes[0].owner, "Layout");
    assert_eq!(class_attributes[1].owner, "UserProperties");
    let class_user = class_attributes[1]
        .user_properties
        .as_ref()
        .expect("class user properties");
    assert_eq!(class_user.properties[0].name, "role");
    assert_eq!(
        class_user.properties[0].value,
        Value::String("caption".into())
    );
    assert_eq!(
        class_user.properties[0].formatted.as_deref(),
        Some("Caption")
    );
    assert!(class_user.properties[0].hidden);

    let element = &tree.elements()[0];
    assert_eq!(element.classes[0].name, "Body");
    assert_eq!(element.classes[0].revision, Some(2));
    assert_eq!(element.classes[0].attributes.len(), 2, "类属性按类名回填");
    assert_eq!(element.attributes.len(), 1);
    assert_eq!(element.attributes[0].owner, "Layout");
    assert_eq!(element.attributes[0].revision, Some(3));

    let resolved = tree.resolved_attributes(0);
    assert_eq!(resolved.len(), 2, "/A 覆盖同类 owner，用户属性保留");
    assert_eq!(resolved[0].owner, "Layout");
    assert_eq!(resolved[0].revision, Some(3), "/A 修订号优先");
    let layout = resolved[0].value.as_dict().expect("layout dict");
    assert_eq!(layout.get("Placement"), Some(&name("Block")), "类键保留");
    assert_eq!(
        layout.get("SpaceBefore"),
        Some(&Value::Integer(24)),
        "/A 覆盖同名键"
    );
    assert_eq!(layout.get("InlineAlign"), Some(&name("Center")));
    assert_eq!(resolved[1].owner, "UserProperties");
    assert_eq!(resolved[1].revision, Some(2), "未被覆盖时沿用类修订号");
    assert!(resolved[1].user_properties.is_some());

    let features: Vec<PdfFeature> = doc.features.borrow().iter().copied().collect();
    assert!(
        features.contains(&PdfFeature::StructureUserProperties),
        "缺少特性 StructureUserProperties"
    );
}

#[test]
fn structure_class_map_accepts_attribute_arrays() {
    let stream = Value::Stream(
        Dict::new().insert("O", name("Layout")),
        StreamData::Owned(b"layout attributes".to_vec()),
    );
    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(StructureTree::new().with_class(
        "Body",
        Value::Array(Array(vec![Value::Dict(layout_dict(12)), stream])),
    ));
    let mut out = Vec::new();
    b.build(&mut out).expect("build");
    assert!(
        String::from_utf8_lossy(&out).contains("/ClassMap << /Body ["),
        "数组形式类映射应原样写出"
    );

    let doc = build_and_parse(&mut b);
    let tree = doc.structure_tree().expect("tree").expect("present");
    let body = tree.root().class_map.get("Body").expect("class map");
    assert_eq!(body.len(), 2);
    assert_eq!(body[0].owner, "Layout");
    assert_eq!(
        body[0].value.as_dict().and_then(|d| d.get("SpaceBefore")),
        Some(&Value::Integer(12))
    );
    assert_eq!(body[1].owner, "Layout");
    assert!(matches!(&body[1].value, Value::Stream(..)));
    assert_eq!(
        doc.stream_bytes(&body[1].value).expect("stream bytes"),
        b"layout attributes"
    );

    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(StructureTree::new().with_class("Empty", Value::Array(Array::new())));
    let doc = build_and_parse(&mut b);
    let tree = doc.structure_tree().expect("tree").expect("present");
    assert!(
        tree.root()
            .class_map
            .get("Empty")
            .expect("empty class")
            .is_empty()
    );

    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(
        StructureTree::new().with_class("Body", Value::Array(Array(vec![Value::Integer(1)]))),
    );
    assert_invalid(
        &mut b,
        "structure class map /Body must be an attribute dictionary or stream",
    );

    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(StructureTree::new().with_class(
        "Body",
        Value::Array(Array(vec![
            Value::Dict(layout_dict(0));
            crate::core::reader::structure::MAX_STRUCTURE_CHILDREN
                + 1
        ])),
    ));
    assert_invalid(
        &mut b,
        "structure class map /Body has 65537 attribute objects",
    );
}

#[test]
fn structure_attribute_revisions_are_written() {
    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(attribute_tree(Value::Dict(layout_dict(12))));
    let mut out = Vec::new();
    b.build(&mut out).expect("build");
    assert!(
        String::from_utf8_lossy(&out).contains("/A << /O /Layout"),
        "单个无修订号属性直接写对象"
    );

    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("P")
                .with_attribute(StructureAttribute::new(Value::Dict(layout_dict(12))))
                .with_attribute(
                    StructureAttribute::new(Value::Dict(layout_dict(24))).with_revision(3),
                ),
        ),
    );
    let mut out = Vec::new();
    b.build(&mut out).expect("build");
    assert!(
        String::from_utf8_lossy(&out).contains("/A ["),
        "多属性或带修订号时写数组"
    );

    let doc = build_and_parse(&mut b);
    let tree = doc.structure_tree().expect("tree").expect("present");
    let attributes = &tree.elements()[0].attributes;
    assert_eq!(attributes.len(), 2);
    assert_eq!(attributes[0].revision, None, "缺省修订号为 0");
    assert_eq!(attributes[1].revision, Some(3));
    assert_eq!(attributes[1].owner, "Layout");
    assert_eq!(
        attributes[1]
            .value
            .as_dict()
            .and_then(|d| d.get("SpaceBefore")),
        Some(&Value::Integer(24))
    );
}
