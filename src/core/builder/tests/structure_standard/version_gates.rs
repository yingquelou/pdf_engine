//! 标准结构属性的版本门禁：格式专用属性（Table 341）需 PDF 1.4、
//! PrintField 属性（Table 348）需 PDF 1.7、表头属性（`/Scope` 等）需
//! PDF 1.5 且 `/Summary` 需 PDF 1.7；不满足时报 `UnsupportedVersion`
//! 且零字节输出，满足时正常写出并由读端登记（或明确不登记）对应画像。

use super::*;

#[test]
fn format_specific_attributes_require_pdf_1_4() {
    let tree = StructureTree::new().with_element(
        StructureElement::new("Div").with_attribute(StructureAttribute::format_specific(
            FormatSpecificAttributes::new(FormatSpecificOwner::Xml1_00)
                .with_entry("role", Value::String("note".into())),
        )),
    );

    let mut b = one_page(PdfVersion::V1_3);
    b.set_structure_tree(tree.clone());
    assert_version_error(&mut b, "standard structure attributes require PDF 1.4");

    let mut b = one_page(PdfVersion::V1_4);
    b.set_structure_tree(tree);
    let mut out = Vec::new();
    b.build(&mut out)
        .expect("PDF 1.4 supports format-specific attributes");
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
