//! 非法输入在构建期以 `WriteError::InvalidObject` 拒绝且零字节输出：元素 `/S`
//! 名字合法性、`/ID` 非空/ASCII/唯一、类名、`/RoleMap` 与 `/ClassMap` 键值与
//! 条目上限、MCR 页序与 MCID 范围/重复声称、深度/元素数/子项数上限镜像读端、
//! 属性对象 `/O` 所有者校验。

use super::super::*;
use super::support::{assert_invalid, attribute_tree, name, one_page};
use crate::core::types::PdfVersion;

#[test]
fn structure_element_type_must_be_a_name() {
    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(StructureTree::new().with_element(StructureElement::new("")));
    assert_invalid(&mut b, "structure element /S must not be empty");

    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(StructureTree::new().with_element(StructureElement::new("My Para")));
    assert_invalid(&mut b, "is not a valid PDF name");
}

#[test]
fn structure_ids_are_validated() {
    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(
        StructureTree::new().with_element(StructureElement::new("P").with_id(Vec::new())),
    );
    assert_invalid(&mut b, "structure element /ID must not be empty");

    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(
        StructureTree::new().with_element(StructureElement::new("P").with_id(vec![0x80u8])),
    );
    assert_invalid(&mut b, "must contain ASCII bytes only");

    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(
        StructureTree::new()
            .with_element(StructureElement::new("P").with_id("dup"))
            .with_element(StructureElement::new("Span").with_id("dup")),
    );
    assert_invalid(&mut b, "is declared more than once");
}

#[test]
fn structure_class_names_are_validated() {
    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(
        StructureTree::new().with_element(StructureElement::new("P").with_class("")),
    );
    assert_invalid(&mut b, "structure class name must not be empty");

    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(
        StructureTree::new().with_element(StructureElement::new("P").with_class("Bad Name")),
    );
    assert_invalid(&mut b, "is not a valid PDF name");
}

#[test]
fn structure_role_map_is_validated() {
    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(StructureTree::new().with_role("", "P"));
    assert_invalid(&mut b, "structure role map key must not be empty");

    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(StructureTree::new().with_role("MyPara", ""));
    assert_invalid(&mut b, "structure role map target for \"MyPara\"");

    let mut b = one_page(PdfVersion::V1_7);
    let mut tree = StructureTree::new();
    for index in 0..4097 {
        tree.role_map.insert(format!("R{index}"), "P".into());
    }
    b.set_structure_tree(tree);
    assert_invalid(&mut b, "structure /RoleMap has 4097 entries (limit 4096)");
}

#[test]
fn structure_class_map_is_validated() {
    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(StructureTree::new().with_class("", Value::Dict(Dict::new())));
    assert_invalid(&mut b, "structure class map key must not be empty");

    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(StructureTree::new().with_class("Body", Value::Integer(1)));
    assert_invalid(
        &mut b,
        "structure class map /Body must be an attribute dictionary or stream",
    );

    let mut b = one_page(PdfVersion::V1_7);
    let mut tree = StructureTree::new();
    for index in 0..4097 {
        tree.class_map
            .insert(format!("C{index}"), Value::Dict(Dict::new()));
    }
    b.set_structure_tree(tree);
    assert_invalid(&mut b, "structure /ClassMap has 4097 entries (limit 4096)");
}

#[test]
fn structure_mcr_targets_are_validated() {
    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(
        StructureTree::new().with_element(StructureElement::new("P").with_marked_content(1, 0)),
    );
    assert_invalid(
        &mut b,
        "structure MCR page 1 is out of range (page count 1)",
    );

    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(
        StructureTree::new().with_element(StructureElement::new("P").with_marked_content(0, -1)),
    );
    assert_invalid(&mut b, "structure MCR MCID must be non-negative, got -1");

    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(
        StructureTree::new().with_element(StructureElement::new("P").with_marked_content(0, 65536)),
    );
    assert_invalid(&mut b, "structure MCR MCID 65536 exceeds the limit 65535");

    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(
        StructureTree::new()
            .with_element(StructureElement::new("P").with_marked_content(0, 0))
            .with_element(StructureElement::new("Span").with_marked_content(0, 0)),
    );
    assert_invalid(
        &mut b,
        "page 0 MCID 0 is claimed by more than one structure element",
    );
}

#[test]
fn structure_limits_mirror_reader() {
    // 深度：顶层 + 64 层子元素，最深一层 depth = 64。
    let mut nested = StructureElement::new("P");
    for _ in 0..64 {
        nested = StructureElement::new("P").with_element(nested);
    }
    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(StructureTree::new().with_element(nested));
    assert_invalid(&mut b, "structure tree depth exceeds 64");

    // 元素数：4097 个顶层元素。
    let mut b = one_page(PdfVersion::V1_7);
    let mut tree = StructureTree::new();
    for _ in 0..4097 {
        tree = tree.with_element(StructureElement::new("P"));
    }
    b.set_structure_tree(tree);
    assert_invalid(&mut b, "structure tree has more than 4096 elements");

    // 单元素子项数：65537 个 MCR（在页范围校验之前先触发上限）。
    let mut element = StructureElement::new("P");
    element.kids = (0..65537)
        .map(|index| StructureKid::marked_content(0, index))
        .collect();
    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(StructureTree::new().with_element(element));
    assert_invalid(&mut b, "structure element has 65537 children (limit 65536)");
}

#[test]
fn structure_attribute_owner_is_validated() {
    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(attribute_tree(Value::Dict(
        Dict::new().insert("SpaceBefore", Value::Integer(1)),
    )));
    assert_invalid(&mut b, "/O is required");

    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(attribute_tree(Value::Dict(
        Dict::new().insert("O", Value::Integer(1)),
    )));
    assert_invalid(&mut b, "/O must be a name");

    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(attribute_tree(Value::Dict(
        Dict::new().insert("O", name("Bad Name")),
    )));
    assert_invalid(&mut b, "is not a valid PDF name");

    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(attribute_tree(Value::Integer(1)));
    assert_invalid(
        &mut b,
        "structure element /A attribute 0 must be an attribute dictionary or stream",
    );

    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(StructureTree::new().with_class(
        "Body",
        Value::Dict(Dict::new().insert("SpaceBefore", Value::Integer(1))),
    ));
    assert_invalid(&mut b, "structure class map /Body /O is required");
}
