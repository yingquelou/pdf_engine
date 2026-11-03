//! 14.8.5.4 Layout 属性（间距、对齐、列、宽高与边框）的写出→解析往返。

use super::*;

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
