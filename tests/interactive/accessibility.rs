//! 文档默认语言与结构元素语言写出 → 解析 → 回读往返（ISO 32000-1 14.9.2）。

use super::*;

#[test]
fn builder_document_and_structure_language_roundtrip_through_parser() {
    use pdf_engine::core::builder::{PageSize, PdfBuilder, StructureElement, StructureTree};
    use pdf_engine::core::reader::accessibility::LanguageIdentifier;
    use pdf_engine::core::types::{PdfFeature, PdfVersion};

    let language = |value: &str| {
        LanguageIdentifier::parse(value).unwrap_or_else(|error| panic!("{value:?}: {error}"))
    };

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    builder.page(PageSize::A4);
    let tree = StructureTree::new().with_element(
        StructureElement::new("Document")
            .with_lang("en-US")
            .with_element(
                StructureElement::new("P")
                    .with_lang("es-MX")
                    .with_element(StructureElement::new("Span").with_marked_content(0, 0)),
            ),
    );
    builder.set_structure_tree(tree);
    builder.set_document_language(language("en-US"));

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let doc = parse_buf(&buf);

    assert_eq!(
        doc.document_language().expect("catalog /Lang"),
        Some(language("en-US"))
    );

    let tree = doc
        .structure_tree()
        .expect("structure tree")
        .expect("tree present");
    assert_eq!(tree.elements().len(), 3);
    assert_eq!(
        tree.element_language(0).expect("document /Lang"),
        Some(language("en-US"))
    );
    assert_eq!(
        tree.element_language(1).expect("paragraph /Lang"),
        Some(language("es-MX"))
    );
    assert_eq!(tree.element_language(2).expect("span /Lang"), None);
    assert_eq!(
        tree.effective_language(2).expect("span effective language"),
        Some(language("es-MX")),
        "Span 应继承最近的 /Lang 声明"
    );
    assert_eq!(
        tree.effective_language(0).expect("root effective language"),
        Some(language("en-US"))
    );

    let features = doc.features.borrow();
    assert!(features.contains(PdfFeature::DocumentLanguage));
    assert!(features.contains(PdfFeature::StructureLanguageText));
}
