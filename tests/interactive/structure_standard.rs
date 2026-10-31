//! 标准结构属性写出 → 解析 → 回读往返（ISO 32000-1 14.8.5.2、14.8.5.5–14.8.5.7）。
//!
//! 两页文档：`Document` 下含列表（L/Lbl/LI，`/ListNumbering`）、表单域
//! （Form，PrintField `/Role` `/checked` `/Desc`）、表格（Table/TR/TH/TD，
//! TH 带 `/ID` `/RowSpan` `/Scope`，TD 带 `/ColSpan` 与指向 TH 的
//! `/Headers`，Table 带 `/Summary`）。解析后逐元素校验
//! `standard_attributes` 与特性画像。

use super::*;

#[test]
fn builder_standard_structure_attributes_roundtrip_through_parser() {
    use pdf_engine::core::builder::{
        ListAttributes, ListNumbering, PageSize, PdfBuilder, PrintFieldAttributes,
        PrintFieldChecked, PrintFieldRole, StructureAttribute, StructureElement, StructureMarkInfo,
        StructureTree, TableAttributes, TableScope,
    };
    use pdf_engine::core::reader::structure::{
        ListNumbering as ReaderListNumbering, PrintFieldChecked as ReaderChecked,
        PrintFieldRole as ReaderRole, TableScope as ReaderTableScope,
    };
    use pdf_engine::core::types::{PdfFeature, PdfVersion};

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    builder.page(PageSize::A4);
    builder.page(PageSize::A4);

    let tree = StructureTree::new().with_element(
        StructureElement::new("Document")
            .with_id("doc")
            .with_element(
                StructureElement::new("L")
                    .with_id("list")
                    .with_attribute(StructureAttribute::list(ListAttributes::new(
                        ListNumbering::LowerAlpha,
                    )))
                    .with_element(StructureElement::new("Lbl").with_marked_content(0, 0))
                    .with_element(StructureElement::new("LI").with_marked_content(0, 1)),
            )
            .with_element(
                StructureElement::new("Form")
                    .with_id("field")
                    .with_attribute(StructureAttribute::print_field(
                        PrintFieldAttributes::new(PrintFieldRole::RadioButton)
                            .with_checked(PrintFieldChecked::On)
                            .with_description("Choice"),
                    ))
                    .with_marked_content(0, 2),
            )
            .with_element(
                StructureElement::new("Table")
                    .with_id("table")
                    .with_attribute(StructureAttribute::table(
                        TableAttributes::new().with_summary("Revenue by quarter"),
                    ))
                    .with_element(
                        StructureElement::new("TR").with_element(
                            StructureElement::new("TH")
                                .with_id("head")
                                .with_attribute(StructureAttribute::table(
                                    TableAttributes::new()
                                        .with_row_span(2)
                                        .with_scope(TableScope::Column),
                                ))
                                .with_marked_content(1, 0),
                        ),
                    )
                    .with_element(
                        StructureElement::new("TR").with_element(
                            StructureElement::new("TD")
                                .with_attribute(StructureAttribute::table(
                                    TableAttributes::new().with_col_span(2).with_header("head"),
                                ))
                                .with_marked_content(1, 1),
                        ),
                    ),
            ),
    );
    builder.set_structure_tree(tree);
    builder.set_mark_info(StructureMarkInfo::new().with_marked(true));

    let mut buf = Vec::new();
    builder
        .build(&mut buf)
        .expect("build standard structure attributes");
    let doc = parse_buf(&buf);

    let tree = doc
        .structure_tree()
        .expect("parse structure tree")
        .expect("structure tree present");
    let types: Vec<&str> = tree
        .elements()
        .iter()
        .map(|element| element.structure_type.as_str())
        .collect();
    assert_eq!(
        types,
        [
            "Document", "L", "Lbl", "LI", "Form", "Table", "TR", "TH", "TR", "TD"
        ]
    );

    let document = tree
        .standard_attributes(0)
        .expect("document standard attributes");
    assert!(document.list.is_none() && document.print_field.is_none() && document.table.is_none());
    assert!(document.other.is_empty());

    let list = tree
        .standard_attributes(1)
        .expect("list standard attributes")
        .list
        .expect("List attributes");
    assert_eq!(list.numbering, ReaderListNumbering::LowerAlpha);

    // 14.8.5.3 的继承解析不在本切片内：子元素自身不报告 List 属性。
    let label = tree
        .standard_attributes(2)
        .expect("label standard attributes");
    assert!(label.list.is_none());

    let field = tree
        .standard_attributes(4)
        .expect("form standard attributes")
        .print_field
        .expect("PrintField attributes");
    assert_eq!(field.role, Some(ReaderRole::RadioButton));
    assert_eq!(field.checked, ReaderChecked::On);
    assert_eq!(field.description.as_deref(), Some("Choice"));

    let table = tree
        .standard_attributes(5)
        .expect("table standard attributes")
        .table
        .expect("Table attributes");
    assert_eq!(table.row_span, 1);
    assert_eq!(table.col_span, 1);
    assert!(table.headers.is_empty());
    assert_eq!(table.scope, None);
    assert_eq!(table.summary.as_deref(), Some("Revenue by quarter"));

    let header = tree
        .standard_attributes(7)
        .expect("header standard attributes")
        .table
        .expect("TH Table attributes");
    assert_eq!(header.row_span, 2);
    assert_eq!(header.col_span, 1);
    assert_eq!(header.scope, Some(ReaderTableScope::Column));

    let cell = tree
        .standard_attributes(9)
        .expect("cell standard attributes")
        .table
        .expect("TD Table attributes");
    assert_eq!(cell.row_span, 1);
    assert_eq!(cell.col_span, 2);
    assert_eq!(cell.headers, vec![b"head".to_vec()]);
    assert_eq!(cell.scope, None);

    let features: Vec<PdfFeature> = doc.features.borrow().iter().copied().collect();
    for feature in [
        PdfFeature::StandardStructureAttributes,
        PdfFeature::PrintFieldAttributes,
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
fn builder_layout_structure_attributes_roundtrip_through_parser() {
    use pdf_engine::core::builder::{
        BorderStyle, BorderStyles, LayoutAttributes, NumberOrAuto, NumberOrNumbers, PageSize,
        PdfBuilder, StructureAttribute, StructureElement, StructureMarkInfo, StructureTree,
        TextAlign,
    };
    use pdf_engine::core::reader::structure::{
        BorderStyle as ReaderBorderStyle, BorderStyles as ReaderBorderStyles,
        NumberOrAuto as ReaderNumberOrAuto, NumberOrNumbers as ReaderNumberOrNumbers,
        TextAlign as ReaderTextAlign,
    };
    use pdf_engine::core::types::{PdfFeature, PdfVersion};

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    builder.page(PageSize::A4);

    let tree = StructureTree::new().with_element(
        StructureElement::new("Document")
            .with_id("layout-doc")
            .with_element(
                StructureElement::new("P")
                    .with_id("body")
                    .with_attribute(StructureAttribute::layout(
                        LayoutAttributes::new()
                            .with_space_before(6.0)
                            .with_start_indent(12.5)
                            .with_text_align(TextAlign::Justify),
                    ))
                    .with_marked_content(0, 0),
            )
            .with_element(
                StructureElement::new("Div")
                    .with_id("columns")
                    .with_attribute(StructureAttribute::layout(
                        LayoutAttributes::new()
                            .with_column_count(2)
                            .with_column_gap(NumberOrNumbers::Number(12.0))
                            .with_column_widths(NumberOrNumbers::Array(vec![120.0, 120.0])),
                    ))
                    .with_element(StructureElement::new("P").with_marked_content(0, 1)),
            )
            .with_element(
                StructureElement::new("Table")
                    .with_id("layout-table")
                    .with_element(
                        StructureElement::new("TR").with_element(
                            StructureElement::new("TH")
                                .with_id("layout-head")
                                .with_attribute(StructureAttribute::layout(
                                    LayoutAttributes::new()
                                        .with_width(NumberOrAuto::Number(48.0))
                                        .with_border_style(BorderStyles::Edges([
                                            Some(BorderStyle::Solid),
                                            None,
                                            Some(BorderStyle::Dashed),
                                            Some(BorderStyle::Double),
                                        ])),
                                ))
                                .with_marked_content(0, 2),
                        ),
                    )
                    .with_element(
                        StructureElement::new("TR").with_element(
                            StructureElement::new("TD")
                                .with_id("layout-cell")
                                .with_attribute(StructureAttribute::layout(
                                    LayoutAttributes::new()
                                        .with_width(NumberOrAuto::Number(72.0))
                                        .with_border_style(BorderStyles::Edges([
                                            Some(BorderStyle::Dotted),
                                            Some(BorderStyle::Dotted),
                                            None,
                                            Some(BorderStyle::Solid),
                                        ])),
                                ))
                                .with_marked_content(0, 3),
                        ),
                    ),
            ),
    );
    builder.set_structure_tree(tree);
    builder.set_mark_info(StructureMarkInfo::new().with_marked(true));

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build layout attributes");
    let doc = parse_buf(&buf);

    let tree = doc
        .structure_tree()
        .expect("parse structure tree")
        .expect("structure tree present");
    let types: Vec<&str> = tree
        .elements()
        .iter()
        .map(|element| element.structure_type.as_str())
        .collect();
    assert_eq!(
        types,
        ["Document", "P", "Div", "P", "Table", "TR", "TH", "TR", "TD"]
    );

    let document = tree
        .standard_attributes(0)
        .expect("document standard attributes");
    assert!(document.layout.is_none());
    assert!(document.list.is_none() && document.print_field.is_none() && document.table.is_none());

    let body = tree
        .standard_attributes(1)
        .expect("body standard attributes")
        .layout
        .expect("P Layout attributes");
    assert_eq!(body.space_before, Some(6.0));
    assert_eq!(body.start_indent, Some(12.5));
    assert_eq!(body.text_align, Some(ReaderTextAlign::Justify));
    assert_eq!(body.width, None);
    assert_eq!(body.border_style, None);

    let columns = tree
        .standard_attributes(2)
        .expect("div standard attributes")
        .layout
        .expect("Div Layout attributes");
    assert_eq!(columns.column_count, Some(2));
    assert_eq!(
        columns.column_gap,
        Some(ReaderNumberOrNumbers::Number(12.0))
    );
    assert_eq!(
        columns.column_widths,
        Some(ReaderNumberOrNumbers::Array(vec![120.0, 120.0]))
    );

    let header = tree
        .standard_attributes(6)
        .expect("header standard attributes")
        .layout
        .expect("TH Layout attributes");
    assert_eq!(header.width, Some(ReaderNumberOrAuto::Number(48.0)));
    assert_eq!(
        header.border_style,
        Some(ReaderBorderStyles::Edges([
            Some(ReaderBorderStyle::Solid),
            None,
            Some(ReaderBorderStyle::Dashed),
            Some(ReaderBorderStyle::Double),
        ]))
    );
    assert_eq!(header.space_before, None);

    let cell = tree
        .standard_attributes(8)
        .expect("cell standard attributes")
        .layout
        .expect("TD Layout attributes");
    assert_eq!(cell.width, Some(ReaderNumberOrAuto::Number(72.0)));
    assert_eq!(
        cell.border_style,
        Some(ReaderBorderStyles::Edges([
            Some(ReaderBorderStyle::Dotted),
            Some(ReaderBorderStyle::Dotted),
            None,
            Some(ReaderBorderStyle::Solid),
        ]))
    );

    let features: Vec<PdfFeature> = doc.features.borrow().iter().copied().collect();
    for feature in [
        PdfFeature::StandardStructureAttributes,
        PdfFeature::LayoutExtendedAttributes,
        PdfFeature::LayoutColumnAttributes,
    ] {
        assert!(
            features.contains(&feature),
            "缺少特性 {feature}: {features:?}"
        );
    }
}
