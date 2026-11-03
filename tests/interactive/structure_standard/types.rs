//! 14.8.4 标准结构类型（`/RoleMap` 解析与 Ruby 扩展）的写出→解析往返。

use super::*;

#[test]
fn builder_standard_structure_types_roundtrip_through_parser() {
    use pdf_engine::core::builder::{PageSize, PdfBuilder, StructureElement, StructureTree};
    use pdf_engine::core::reader::structure::{StandardStructureType, StructureTypeCategory};
    use pdf_engine::core::types::{PdfFeature, PdfVersion};

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    builder.page(PageSize::A4);

    // 14.8.4：/RoleMap 把自定义 "MySpan" 映射为标准 "Span"；同一棵树还
    // 覆盖分组（Document）、BLSE（P）、插图（Figure）、表格元素（TR/TH）
    // 与 Ruby 家族（Ruby/RB/RT/RP，PDF 1.5 扩展类型）。
    let tree = StructureTree::new()
        .with_role("MySpan", "Span")
        .with_element(
            StructureElement::new("Document")
                .with_id("doc")
                .with_element(
                    StructureElement::new("P")
                        .with_id("p")
                        .with_element(
                            StructureElement::new("MySpan")
                                .with_id("span")
                                .with_marked_content(0, 0),
                        )
                        .with_element(
                            StructureElement::new("Figure")
                                .with_id("figure")
                                .with_marked_content(0, 1),
                        ),
                )
                .with_element(
                    StructureElement::new("Table")
                        .with_id("table")
                        .with_element(
                            StructureElement::new("TR").with_element(
                                StructureElement::new("TH")
                                    .with_id("th")
                                    .with_marked_content(0, 2),
                            ),
                        ),
                )
                .with_element(
                    StructureElement::new("Ruby")
                        .with_id("ruby")
                        .with_element(
                            StructureElement::new("RB")
                                .with_id("rb")
                                .with_marked_content(0, 3),
                        )
                        .with_element(
                            StructureElement::new("RT")
                                .with_id("rt")
                                .with_marked_content(0, 4),
                        )
                        .with_element(
                            StructureElement::new("RP")
                                .with_id("rp")
                                .with_marked_content(0, 5),
                        ),
                ),
        );
    builder.set_structure_tree(tree);

    let mut buf = Vec::new();
    builder
        .build(&mut buf)
        .expect("build standard structure types");
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

    let expected = [
        (
            "doc",
            StandardStructureType::Document,
            StructureTypeCategory::Grouping,
        ),
        (
            "p",
            StandardStructureType::Paragraph,
            StructureTypeCategory::BlockLevel,
        ),
        // /S 是自定义名，经 /RoleMap 解析为标准 Span。
        (
            "span",
            StandardStructureType::Span,
            StructureTypeCategory::InlineLevel,
        ),
        (
            "figure",
            StandardStructureType::Figure,
            StructureTypeCategory::Illustration,
        ),
        (
            "table",
            StandardStructureType::Table,
            StructureTypeCategory::BlockLevel,
        ),
        (
            "th",
            StandardStructureType::TableHeader,
            StructureTypeCategory::TableElement,
        ),
        (
            "ruby",
            StandardStructureType::Ruby,
            StructureTypeCategory::InlineLevel,
        ),
        (
            "rb",
            StandardStructureType::RubyBase,
            StructureTypeCategory::InlineLevel,
        ),
        (
            "rt",
            StandardStructureType::RubyText,
            StructureTypeCategory::InlineLevel,
        ),
        (
            "rp",
            StandardStructureType::RubyPunctuation,
            StructureTypeCategory::InlineLevel,
        ),
    ];
    for (id, standard, category) in expected {
        let parsed = tree
            .standard_type(index_of(id))
            .unwrap_or_else(|| panic!("missing standard type for {id}"));
        assert_eq!(parsed, standard, "{id} 的标准类型");
        assert_eq!(parsed.category(), Some(category), "{id} 的分组");
    }

    let features: Vec<PdfFeature> = doc.features.borrow().iter().copied().collect();
    assert!(
        features.contains(&PdfFeature::StructureTypeExtensions),
        "Ruby 家族应登记 PDF 1.5 结构类型扩展: {features:?}"
    );
}
