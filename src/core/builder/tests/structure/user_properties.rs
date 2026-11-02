//! `/MarkInfo`（Table 321）与用户属性（Table 328）：`/Marked` `/UserProperties`
//! `/Suspects` 的写出与版本门禁（PDF 1.4/1.6）、用户属性对 `/MarkInfo` 标志的
//! 交叉校验与属性条目（`/N` `/V` `/F` `/H`）的构建期校验。

use super::super::*;
use super::support::{
    assert_invalid, assert_version_error, attribute_tree, build_and_parse, name, one_page,
};
use crate::core::types::{Array, PdfFeature, PdfVersion};

#[test]
fn mark_info_requires_pdf_1_4() {
    let mut b = one_page(PdfVersion::V1_3);
    b.set_mark_info(StructureMarkInfo::new().with_marked(true));
    assert_version_error(&mut b, "mark info require PDF 1.4");

    let mut b = one_page(PdfVersion::V1_4);
    b.set_mark_info(StructureMarkInfo::new().with_marked(true));
    let mut out = Vec::new();
    b.build(&mut out).expect("PDF 1.4 supports /MarkInfo");
    assert!(String::from_utf8_lossy(&out).contains("/MarkInfo <<"));
}

#[test]
fn mark_info_user_properties_and_suspects_require_pdf_1_6() {
    let mut b = one_page(PdfVersion::V1_5);
    b.set_mark_info(
        StructureMarkInfo::new()
            .with_marked(true)
            .with_user_properties(true),
    );
    assert_version_error(
        &mut b,
        "/MarkInfo /UserProperties and /Suspects require PDF 1.6",
    );

    let mut b = one_page(PdfVersion::V1_5);
    b.set_mark_info(
        StructureMarkInfo::new()
            .with_marked(true)
            .with_suspects(true),
    );
    assert_version_error(
        &mut b,
        "/MarkInfo /UserProperties and /Suspects require PDF 1.6",
    );

    let mut b = one_page(PdfVersion::V1_6);
    b.set_mark_info(
        StructureMarkInfo::new()
            .with_marked(true)
            .with_user_properties(true)
            .with_suspects(true),
    );
    let doc = build_and_parse(&mut b);
    let info = doc.mark_info().expect("mark info").expect("present");
    assert!(info.marked);
    assert!(info.user_properties);
    assert!(info.suspects);
    let features: Vec<PdfFeature> = doc.features.borrow().iter().copied().collect();
    assert!(features.contains(&PdfFeature::MarkInfo));
    assert!(features.contains(&PdfFeature::MarkInfoUserProperties));
    assert!(features.contains(&PdfFeature::MarkInfoSuspects));
}

/// 用户属性属性对象（Table 328）。
fn user_property_attribute(properties: Value) -> Value {
    Value::Dict(
        Dict::new()
            .insert("O", name("UserProperties"))
            .insert("P", properties),
    )
}

#[test]
fn structure_user_properties_require_pdf_1_6() {
    let tree = || {
        StructureTree::new().with_element(StructureElement::new("P").with_user_properties(vec![
            UserProperty::new("role", Value::String("caption".into())),
        ]))
    };
    let mut b = one_page(PdfVersion::V1_5);
    b.set_structure_tree(tree());
    b.set_mark_info(StructureMarkInfo::new().with_marked(true));
    assert_version_error(&mut b, "structure user properties require PDF 1.6");

    let mut b = one_page(PdfVersion::V1_5);
    b.set_structure_tree(
        StructureTree::new().with_class(
            "Body",
            StructureAttribute::user_properties(vec![UserProperty::new(
                "role",
                Value::String("caption".into()),
            )])
            .value,
        ),
    );
    assert_version_error(&mut b, "structure user properties require PDF 1.6");

    let mut b = one_page(PdfVersion::V1_6);
    b.set_structure_tree(tree());
    b.set_mark_info(
        StructureMarkInfo::new()
            .with_marked(true)
            .with_user_properties(true),
    );
    b.build(&mut Vec::new())
        .expect("PDF 1.6 supports user properties");
}

#[test]
fn structure_user_properties_require_mark_info_flag() {
    let tree = || {
        StructureTree::new().with_element(StructureElement::new("P").with_user_properties(vec![
            UserProperty::new("role", Value::String("caption".into())),
        ]))
    };
    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(tree());
    assert_invalid(
        &mut b,
        "structure user properties require /MarkInfo /UserProperties true",
    );

    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(tree());
    b.set_mark_info(StructureMarkInfo::new().with_marked(true));
    assert_invalid(
        &mut b,
        "structure user properties require /MarkInfo /UserProperties true",
    );
}

#[test]
fn structure_user_property_entries_are_validated() {
    let user = |properties: Value| attribute_tree(user_property_attribute(properties));

    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(user(Value::Array(Array::new())));
    b.set_mark_info(
        StructureMarkInfo::new()
            .with_marked(true)
            .with_user_properties(true),
    );
    let doc = build_and_parse(&mut b);
    let tree = doc.structure_tree().expect("tree").expect("present");
    let attribute = &tree.elements()[0].attributes[0];
    assert!(attribute.user_properties.is_some(), "空 /P 数组允许");

    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(attribute_tree(Value::Dict(
        Dict::new().insert("O", name("UserProperties")),
    )));
    assert_invalid(&mut b, "/P is required");

    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(user(Value::Integer(1)));
    assert_invalid(&mut b, "/P must be an array of user property dictionaries");

    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(user(Value::Array(Array(vec![Value::Integer(1)]))));
    assert_invalid(&mut b, "must be a user property dictionary");

    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(user(Value::Array(Array(vec![Value::Dict(
        Dict::new().insert("V", Value::Integer(1)),
    )]))));
    assert_invalid(&mut b, "/N is required");

    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(user(Value::Array(Array(vec![Value::Dict(
        Dict::new()
            .insert("N", Value::Integer(1))
            .insert("V", Value::Integer(1)),
    )]))));
    assert_invalid(&mut b, "/N must be a text string");

    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(user(Value::Array(Array(vec![Value::Dict(
        Dict::new().insert("N", Value::String("role".into())),
    )]))));
    assert_invalid(&mut b, "/V is required");

    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(user(Value::Array(Array(vec![Value::Dict(
        Dict::new()
            .insert("N", Value::String("role".into()))
            .insert("V", Value::Integer(1))
            .insert("F", Value::Integer(2)),
    )]))));
    assert_invalid(&mut b, "/F must be a text string");

    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(user(Value::Array(Array(vec![Value::Dict(
        Dict::new()
            .insert("N", Value::String("role".into()))
            .insert("V", Value::Integer(1))
            .insert("H", Value::Integer(2)),
    )]))));
    assert_invalid(&mut b, "/H must be a boolean");
}
