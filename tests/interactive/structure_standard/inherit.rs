//! 14.8.5.3 属性继承（`effective_standard_attributes`）的写出→解析往返。

use super::*;

#[test]
fn builder_effective_structure_attributes_inherit_through_parser() {
    use pdf_engine::core::builder::{
        BorderColor, BorderStyle, BorderStyles, LayoutAttributes, ListAttributes, ListNumbering,
        PageSize, PdfBuilder, Placement, StructureAttribute, StructureElement, StructureMarkInfo,
        StructureTree, TextAlign,
    };
    use pdf_engine::core::reader::structure::{
        BorderColor as ReaderBorderColor, ListNumbering as ReaderListNumbering,
        TextAlign as ReaderTextAlign,
    };
    use pdf_engine::core::types::{PdfFeature, PdfVersion};

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    builder.page(PageSize::A4);

    let tree = StructureTree::new().with_element(
        StructureElement::new("Document")
            .with_id("inherit-doc")
            .with_element(
                StructureElement::new("Div")
                    .with_id("inherit-div")
                    .with_attribute(StructureAttribute::layout(
                        LayoutAttributes::new()
                            .with_text_align(TextAlign::Justify)
                            .with_border_color(BorderColor::Uniform([0.1, 0.2, 0.3]))
                            .with_placement(Placement::Block)
                            .with_border_style(BorderStyles::Uniform(BorderStyle::Solid)),
                    ))
                    .with_element(
                        StructureElement::new("P")
                            .with_id("inherit-p")
                            .with_attribute(StructureAttribute::layout(
                                LayoutAttributes::new().with_space_before(6.0),
                            ))
                            .with_marked_content(0, 0),
                    ),
            )
            .with_element(
                StructureElement::new("L")
                    .with_id("inherit-list")
                    .with_attribute(StructureAttribute::list(ListAttributes::new(
                        ListNumbering::Disc,
                    )))
                    .with_element(
                        StructureElement::new("LI")
                            .with_id("inherit-li")
                            .with_element(
                                StructureElement::new("Lbl")
                                    .with_id("inherit-lbl")
                                    .with_marked_content(0, 1),
                            ),
                    ),
            ),
    );
    builder.set_structure_tree(tree);
    builder.set_mark_info(StructureMarkInfo::new().with_marked(true));

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build inherited attributes");
    let doc = parse_buf(&buf);

    let tree = doc
        .structure_tree()
        .expect("parse structure tree")
        .expect("structure tree present");
    let index_of = |id: &str| {
        tree.elements()
            .iter()
            .position(|element| element.id.as_deref() == Some(id.as_bytes()))
            .unwrap_or_else(|| panic!("missing structure element {id}"))
    };

    let paragraph = tree
        .effective_standard_attributes(index_of("inherit-p"))
        .expect("P effective standard attributes")
        .layout
        .expect("inherited Layout attributes");
    assert_eq!(paragraph.text_align, Some(ReaderTextAlign::Justify));
    assert_eq!(
        paragraph.border_color,
        Some(ReaderBorderColor::Uniform([0.1, 0.2, 0.3]))
    );
    assert_eq!(paragraph.space_before, Some(6.0));
    assert_eq!(paragraph.placement, None, "/Placement 不可继承");
    assert_eq!(paragraph.border_style, None, "/BorderStyle 不可继承");

    let own = tree
        .standard_attributes(index_of("inherit-p"))
        .expect("P own standard attributes")
        .layout
        .expect("P own Layout attributes");
    assert_eq!(own.text_align, None, "standard_attributes 仍只看自身");
    assert_eq!(own.border_color, None);
    assert_eq!(own.space_before, Some(6.0));

    let label = tree
        .effective_standard_attributes(index_of("inherit-lbl"))
        .expect("Lbl effective standard attributes")
        .list
        .expect("inherited List attributes");
    assert_eq!(label.numbering, ReaderListNumbering::Disc);
    assert!(
        tree.standard_attributes(index_of("inherit-lbl"))
            .expect("Lbl own standard attributes")
            .list
            .is_none(),
        "Lbl 自身没有 /List 属性"
    );

    let item = tree
        .effective_standard_attributes(index_of("inherit-li"))
        .expect("LI effective standard attributes")
        .list
        .expect("inherited List attributes");
    assert_eq!(item.numbering, ReaderListNumbering::Disc);

    let features: Vec<PdfFeature> = doc.features.borrow().iter().copied().collect();
    for feature in [
        PdfFeature::StandardStructureAttributes,
        PdfFeature::LayoutExtendedAttributes,
    ] {
        assert!(
            features.contains(&feature),
            "缺少特性 {feature}: {features:?}"
        );
    }
    let divider = tree
        .effective_standard_attributes(index_of("inherit-div"))
        .expect("Div effective standard attributes")
        .layout
        .expect("Div Layout attributes");
    assert_eq!(
        divider
            .dict
            .get("Placement")
            .and_then(|value| value.as_name()),
        Some("Block"),
        "父元素自身的非可继承条目仍在结果中"
    );
}
