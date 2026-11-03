//! 标准属性（14.8.5.5–14.8.5.7）的构建期校验与零字节契约：属性只作用于
//! 规范指定的结构元素（PrintField → Form、`/RowSpan` `/Scope` → TH/TD，
//! 可继承的 `/ListNumbering` 与经 `/RoleMap` 解析为 Form 的自定义类型放行）、
//! `/Headers` 必须引用同树内已知的 TH 元素 `/ID` 且不重复，以及属性值形态
//! （名字/整数/字节串数组）与 `/ClassMap` 复用属性对象时只查值不查适用性。

use super::*;

#[test]
fn standard_attribute_applicability_is_validated() {
    fn single(structure_type: &str, attribute: StructureAttribute) -> StructureTree {
        StructureTree::new()
            .with_element(StructureElement::new(structure_type).with_attribute(attribute))
    }

    let print_field =
        || StructureAttribute::print_field(PrintFieldAttributes::new(PrintFieldRole::CheckBox));
    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(single("P", print_field()));
    assert_invalid(&mut b, "applies only to Form structure elements");

    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(single(
        "Table",
        StructureAttribute::table(TableAttributes::new().with_row_span(2)),
    ));
    assert_invalid(
        &mut b,
        "/RowSpan applies only to TH or TD structure elements",
    );

    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(single(
        "TD",
        StructureAttribute::table(TableAttributes::new().with_scope(TableScope::Row)),
    ));
    assert_invalid(&mut b, "/Scope applies only to TH structure elements");

    // `/ListNumbering` 可继承（14.8.5.3），允许出现在任意元素上。
    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(single(
        "P",
        StructureAttribute::list(ListAttributes::new(ListNumbering::Decimal)),
    ));
    let mut out = Vec::new();
    b.build(&mut out)
        .expect("ListNumbering is allowed on any structure element");
    assert!(!out.is_empty());

    // 经 RoleMap 解析为 Form 的自定义类型同样允许 PrintField 属性。
    let tree = StructureTree::new()
        .with_role("MyForm", "Form")
        .with_element(StructureElement::new("MyForm").with_attribute(print_field()));
    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(tree);
    let mut out = Vec::new();
    b.build(&mut out)
        .expect("RoleMap-mapped Form supports PrintField attributes");
    assert!(!out.is_empty());
}

#[test]
fn table_header_ids_are_validated() {
    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(StructureTree::new().with_element(
        StructureElement::new("Table").with_element(StructureElement::new("TR").with_element(
            StructureElement::new("TD").with_attribute(StructureAttribute::table(
                TableAttributes::new().with_header("missing"),
            )),
        )),
    ));
    assert_invalid(&mut b, "references an unknown structure element /ID");

    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Table").with_element(
                StructureElement::new("TR")
                    .with_element(StructureElement::new("TD").with_id("cell"))
                    .with_element(StructureElement::new("TD").with_attribute(
                        StructureAttribute::table(TableAttributes::new().with_header("cell")),
                    )),
            ),
        ),
    );
    assert_invalid(&mut b, "must reference a TH element");

    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Table").with_element(
                StructureElement::new("TR")
                    .with_element(StructureElement::new("TH").with_id("header"))
                    .with_element(
                        StructureElement::new("TD").with_attribute(StructureAttribute::table(
                            TableAttributes::new()
                                .with_header("header")
                                .with_header("header"),
                        )),
                    ),
            ),
        ),
    );
    assert_invalid(&mut b, "duplicates another /ID");
}

#[test]
fn standard_attribute_value_types_are_validated() {
    let attribute = |entries: Dict| StructureAttribute::new(Value::Dict(entries));

    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("L").with_attribute(attribute(
                Dict::new()
                    .insert("O", Value::Name("List".into()))
                    .insert("ListNumbering", Value::Integer(1)),
            )),
        ),
    );
    assert_invalid(&mut b, "/ListNumbering must be a name");

    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Form").with_attribute(attribute(
                Dict::new()
                    .insert("O", Value::Name("PrintField".into()))
                    .insert("Role", Value::Integer(1)),
            )),
        ),
    );
    assert_invalid(&mut b, "/Role must be a name");

    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Table").with_element(
                StructureElement::new("TR").with_element(
                    StructureElement::new("TH").with_attribute(attribute(
                        Dict::new()
                            .insert("O", Value::Name("Table".into()))
                            .insert("RowSpan", Value::Integer(0)),
                    )),
                ),
            ),
        ),
    );
    assert_invalid(&mut b, "/RowSpan must be an integer of at least 1");

    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Table").with_element(
                StructureElement::new("TR").with_element(
                    StructureElement::new("TD").with_attribute(attribute(
                        Dict::new()
                            .insert("O", Value::Name("Table".into()))
                            .insert("Headers", Value::Integer(1)),
                    )),
                ),
            ),
        ),
    );
    assert_invalid(&mut b, "/Headers must be an array of byte strings");

    // `/ClassMap` 复用同一属性对象时只查值形态，不查元素适用性。
    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(
        StructureTree::new().with_class(
            "Body",
            Value::Dict(
                Dict::new()
                    .insert("O", Value::Name("Table".into()))
                    .insert("Scope", Value::Integer(1)),
            ),
        ),
    );
    assert_invalid(&mut b, "/Scope must be a name");

    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(
        StructureTree::new().with_class(
            "Body",
            Value::Dict(
                Dict::new()
                    .insert("O", Value::Name("Table".into()))
                    .insert("Scope", Value::Name("Column".into())),
            ),
        ),
    );
    let mut out = Vec::new();
    b.build(&mut out)
        .expect("class map attributes skip element applicability");
    assert!(!out.is_empty());
}
