//! 14.8.5.2 格式专用属性所有者（XML-1.00 与 `/ClassMap` 的 CSS-2.00）的写出→解析往返。

use super::*;

#[test]
fn builder_format_specific_structure_attributes_roundtrip_through_parser() {
    use pdf_engine::core::builder::{
        FormatSpecificAttributes, FormatSpecificOwner, PageSize, PdfBuilder, StructureAttribute,
        StructureElement, StructureTree,
    };
    use pdf_engine::core::reader::structure::FormatSpecificOwner as ReaderFormatSpecificOwner;
    use pdf_engine::core::types::{Dict, PdfFeature, PdfVersion, Value};

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    builder.page(PageSize::A4);

    // Document(0) > Div(1) > P(2)：Div 挂 XML-1.00 格式专用属性，P 通过
    // `/C "css"` 复用 `/ClassMap` 里的 CSS-2.00 属性对象。
    let tree = StructureTree::new()
        .with_class(
            "css",
            Value::Dict(
                Dict::new()
                    .insert("O", Value::Name("CSS-2.00".into()))
                    .insert("display", Value::String("block".into())),
            ),
        )
        .with_element(
            StructureElement::new("Document")
                .with_id("doc")
                .with_element(
                    StructureElement::new("Div")
                        .with_id("box")
                        .with_attribute(StructureAttribute::format_specific(
                            FormatSpecificAttributes::new(FormatSpecificOwner::Xml1_00)
                                .with_entry("role", Value::String("note".into()))
                                .with_entry("level", Value::Integer(2)),
                        ))
                        .with_element(
                            StructureElement::new("P")
                                .with_class("css")
                                .with_marked_content(0, 0),
                        ),
                ),
        );
    builder.set_structure_tree(tree);

    let mut buf = Vec::new();
    builder
        .build(&mut buf)
        .expect("build format-specific attributes");
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
    assert_eq!(types, ["Document", "Div", "P"]);

    let divider = tree
        .standard_attributes(1)
        .expect("div standard attributes");
    assert_eq!(divider.format_specific.len(), 1);
    let xml = &divider.format_specific[0];
    assert_eq!(xml.owner, ReaderFormatSpecificOwner::Xml1_00);
    let xml_dict = xml.dict().expect("XML-1.00 attribute dictionary");
    assert_eq!(
        xml_dict.get("role"),
        Some(&Value::String("note".into())),
        "{xml_dict:?}"
    );
    assert_eq!(xml_dict.get("level"), Some(&Value::Integer(2)));

    let paragraph = tree
        .standard_attributes(2)
        .expect("paragraph standard attributes");
    assert_eq!(paragraph.format_specific.len(), 1);
    let css = &paragraph.format_specific[0];
    assert_eq!(css.owner, ReaderFormatSpecificOwner::Css2_00);
    assert_eq!(
        css.dict().and_then(|dict| dict.get("display")),
        Some(&Value::String("block".into())),
        "类属性对象应按元素顺序合并"
    );

    // 格式专用属性不参与 14.8.5.3 继承：父元素的 XML-1.00 不传播到 P。
    let effective = tree
        .effective_standard_attributes(2)
        .expect("paragraph effective standard attributes");
    assert_eq!(effective.format_specific.len(), 1);
    assert_eq!(
        effective.format_specific[0].owner,
        ReaderFormatSpecificOwner::Css2_00
    );

    let features: Vec<PdfFeature> = doc.features.borrow().iter().copied().collect();
    assert!(
        features.contains(&PdfFeature::StandardStructureAttributes),
        "{features:?}"
    );
}
