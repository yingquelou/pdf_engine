//! 标准结构属性写端测试（ISO 32000-1 14.8.5.2、14.8.5.5–14.8.5.7）。
//!
//! 覆盖 List（Table 347）、PrintField（Table 348）、Table（Table 349）
//! 类型化构造器的写出 → 解析往返，元素适用性与 `/Headers` 引用的校验，
//! 以及 PDF 1.4/1.5/1.7 版本门禁；失败均要求零字节输出。

use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::core::reader::structure::{
    ListNumbering as ReaderListNumbering, PrintFieldChecked as ReaderChecked,
    PrintFieldRole as ReaderRole, TableScope as ReaderTableScope,
};
use crate::core::types::{Document, PdfFeature, PdfVersion};

/// 写出后重新解析：结构树只在 `build` 的导航后处理中物化。
fn build_and_parse(b: &mut PdfBuilder) -> Document {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let mut out = Vec::new();
    b.build(&mut out).expect("build");
    let base = std::env::current_exe()
        .ok()
        .and_then(|p| p.ancestors().nth(3).map(|t| t.to_path_buf()))
        .filter(|p| p.file_name() == Some(std::ffi::OsStr::new("target")))
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join("test-tmp");
    std::fs::create_dir_all(&dir).expect("create test-tmp");
    let path = dir.join(format!(
        "pdf-engine-structure-standard-{}-{}.pdf",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&path, &out).expect("write temp pdf");
    let file = std::fs::File::open(&path).expect("open temp pdf");
    let mut parser = crate::core::reader::Parser::new(file).expect("parser");
    let doc = parser.parse_document().expect("parse");
    let _ = std::fs::remove_file(&path);
    doc
}

fn assert_invalid(b: &mut PdfBuilder, expected: &str) {
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("build should fail");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(error.to_string().contains(expected), "{error}");
    assert!(out.is_empty(), "失败时不应写出任何字节");
}

fn assert_version_error(b: &mut PdfBuilder, expected: &str) {
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("build should fail");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(_)),
        "{error}"
    );
    assert!(error.to_string().contains(expected), "{error}");
    assert!(out.is_empty(), "失败时不应写出任何字节");
}

fn one_page(version: PdfVersion) -> PdfBuilder {
    let mut b = PdfBuilder::with_version(version);
    b.page(PageSize::A4);
    b
}

fn reader_features(doc: &Document) -> Vec<PdfFeature> {
    doc.features.borrow().iter().copied().collect()
}

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

#[test]
fn print_field_attributes_require_pdf_1_7() {
    let tree = StructureTree::new().with_element(StructureElement::new("Form").with_attribute(
        StructureAttribute::print_field(PrintFieldAttributes::new(PrintFieldRole::TextValue)),
    ));

    let mut b = one_page(PdfVersion::V1_6);
    b.set_structure_tree(tree.clone());
    assert_version_error(&mut b, "PrintField attributes require PDF 1.7");

    let mut b = one_page(PdfVersion::V1_7);
    b.set_structure_tree(tree);
    let mut out = Vec::new();
    b.build(&mut out).expect("PDF 1.7 supports PrintField");
    assert!(!out.is_empty());
}

#[test]
fn table_header_and_summary_attributes_require_pdf_1_5_and_1_7() {
    let scoped = StructureTree::new().with_element(
        StructureElement::new("Table").with_element(
            StructureElement::new("TR").with_element(
                StructureElement::new("TH")
                    .with_id("header")
                    .with_attribute(StructureAttribute::table(
                        TableAttributes::new().with_scope(TableScope::Row),
                    )),
            ),
        ),
    );
    let mut b = one_page(PdfVersion::V1_4);
    b.set_structure_tree(scoped);
    assert_version_error(&mut b, "table header attributes require PDF 1.5");

    let summarized =
        StructureTree::new().with_element(StructureElement::new("Table").with_attribute(
            StructureAttribute::table(TableAttributes::new().with_summary("Revenue")),
        ));
    let mut b = one_page(PdfVersion::V1_6);
    b.set_structure_tree(summarized);
    assert_version_error(&mut b, "table summary requires PDF 1.7");

    // 仅跨度（RowSpan/ColSpan）自 PDF 1.4 起可用。
    let spanned = StructureTree::new().with_element(StructureElement::new("Table").with_element(
        StructureElement::new("TR").with_element(StructureElement::new("TH").with_attribute(
            StructureAttribute::table(TableAttributes::new().with_row_span(2)),
        )),
    ));
    let mut b = one_page(PdfVersion::V1_4);
    b.set_structure_tree(spanned);
    let doc = build_and_parse(&mut b);
    let tree = doc
        .structure_tree()
        .expect("structure tree")
        .expect("tree present");
    let header = tree
        .standard_attributes(2)
        .expect("header standard attributes")
        .table
        .expect("TH Table attributes");
    assert_eq!(header.row_span, 2);
    let features = reader_features(&doc);
    assert!(
        features.contains(&PdfFeature::StandardStructureAttributes),
        "{features:?}"
    );
    assert!(!features.contains(&PdfFeature::TableHeaderAttributes));
    assert!(!features.contains(&PdfFeature::TableSummary));
}
