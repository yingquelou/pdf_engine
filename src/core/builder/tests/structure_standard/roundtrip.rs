//! List/PrintField/Table（14.8.5.5–14.8.5.7，Table 347–349）类型化属性的
//! 写出 → 解析往返：`/ListNumbering`、`/Role` `/Checked` `/Description`、
//! `/RowSpan` `/ColSpan` `/Scope` `/Summary` `/Headers` 的读端类型化取值，
//! 以及对应读端特性画像（`StandardStructureAttributes`、
//! `PrintFieldAttributes`、`TableHeaderAttributes`、`TableSummary`）的登记。

use super::*;
use crate::core::reader::structure::{
    ListNumbering as ReaderListNumbering, PrintFieldChecked as ReaderChecked,
    PrintFieldRole as ReaderRole, TableScope as ReaderTableScope,
};

#[test]
fn list_attributes_roundtrip_through_parser() {
    let tree = StructureTree::new().with_element(
        StructureElement::new("L")
            .with_id("list")
            .with_attribute(StructureAttribute::list(ListAttributes::new(
                ListNumbering::UpperRoman,
            )))
            .with_marked_content(0, 0),
    );
    let mut b = one_page(PdfVersion::V1_4);
    b.set_structure_tree(tree);

    let doc = build_and_parse(&mut b);
    let tree = doc
        .structure_tree()
        .expect("structure tree")
        .expect("tree present");
    let standard = tree.standard_attributes(0).expect("standard attributes");
    assert!(standard.print_field.is_none());
    assert!(standard.table.is_none());
    assert!(standard.other.is_empty());
    let list = standard.list.expect("List attributes");
    assert_eq!(list.numbering, ReaderListNumbering::UpperRoman);
    assert_eq!(list.dict.get("O").and_then(Value::as_name), Some("List"));

    let features = reader_features(&doc);
    assert!(
        features.contains(&PdfFeature::StandardStructureAttributes),
        "{features:?}"
    );
}

#[test]
fn print_field_attributes_roundtrip_through_parser() {
    // `/RoleMap` 把自定义类型解析为 Form 后应允许 PrintField 属性。
    let tree = StructureTree::new()
        .with_role("MyForm", "Form")
        .with_element(
            StructureElement::new("MyForm")
                .with_attribute(StructureAttribute::print_field(
                    PrintFieldAttributes::new(PrintFieldRole::RadioButton)
                        .with_checked(PrintFieldChecked::On)
                        .with_description("Choice"),
                ))
                .with_marked_content(0, 0),
        );
    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(tree);

    let doc = build_and_parse(&mut b);
    let tree = doc
        .structure_tree()
        .expect("structure tree")
        .expect("tree present");
    let standard = tree.standard_attributes(0).expect("standard attributes");
    assert!(standard.list.is_none());
    assert!(standard.table.is_none());
    let field = standard.print_field.expect("PrintField attributes");
    assert_eq!(field.role, Some(ReaderRole::RadioButton));
    assert_eq!(field.checked, ReaderChecked::On);
    assert_eq!(field.description.as_deref(), Some("Choice"));

    let features = reader_features(&doc);
    for feature in [
        PdfFeature::StandardStructureAttributes,
        PdfFeature::PrintFieldAttributes,
    ] {
        assert!(
            features.contains(&feature),
            "缺少特性 {feature}: {features:?}"
        );
    }
}

#[test]
fn table_attributes_roundtrip_through_parser() {
    let tree = StructureTree::new().with_element(
        StructureElement::new("Table")
            .with_id("table")
            .with_attribute(StructureAttribute::table(
                TableAttributes::new().with_summary("Revenue"),
            ))
            .with_element(
                StructureElement::new("TR").with_element(
                    StructureElement::new("TH")
                        .with_id("header")
                        .with_attribute(StructureAttribute::table(
                            TableAttributes::new()
                                .with_row_span(2)
                                .with_scope(TableScope::Column),
                        ))
                        .with_element(
                            StructureElement::new("TD")
                                .with_attribute(StructureAttribute::table(
                                    TableAttributes::new()
                                        .with_col_span(2)
                                        .with_header("header"),
                                ))
                                .with_marked_content(0, 0),
                        ),
                ),
            ),
    );
    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(tree);

    let doc = build_and_parse(&mut b);
    let tree = doc
        .structure_tree()
        .expect("structure tree")
        .expect("tree present");

    let table = tree
        .standard_attributes(0)
        .expect("table standard attributes")
        .table
        .expect("Table attributes");
    assert_eq!(table.row_span, 1);
    assert_eq!(table.col_span, 1);
    assert!(table.headers.is_empty());
    assert_eq!(table.scope, None);
    assert_eq!(table.summary.as_deref(), Some("Revenue"));

    let header = tree
        .standard_attributes(2)
        .expect("header standard attributes")
        .table
        .expect("TH Table attributes");
    assert_eq!(header.row_span, 2);
    assert_eq!(header.col_span, 1);
    assert_eq!(header.scope, Some(ReaderTableScope::Column));
    assert!(header.headers.is_empty());

    let cell = tree
        .standard_attributes(3)
        .expect("cell standard attributes")
        .table
        .expect("TD Table attributes");
    assert_eq!(cell.row_span, 1);
    assert_eq!(cell.col_span, 2);
    assert_eq!(cell.headers, vec![b"header".to_vec()]);
    assert_eq!(cell.scope, None);

    let features = reader_features(&doc);
    for feature in [
        PdfFeature::StandardStructureAttributes,
        PdfFeature::TableHeaderAttributes,
        PdfFeature::TableSummary,
    ] {
        assert!(
            features.contains(&feature),
            "缺少特性 {feature}: {features:?}"
        );
    }
}
