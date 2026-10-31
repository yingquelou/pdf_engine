//! 文档默认语言、结构元素语言与 Span 属性表写出 → 解析 → 回读往返
//! （ISO 32000-1 14.9.2–14.9.5）。

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

#[test]
fn builder_span_property_lists_roundtrip_through_parser() {
    use pdf_engine::core::builder::{Font, PageSize, PdfBuilder, SpanProperties};
    use pdf_engine::core::reader::accessibility::LanguageIdentifier;
    use pdf_engine::core::types::PdfVersion;

    let language = |value: &str| {
        LanguageIdentifier::parse(value).unwrap_or_else(|error| panic!("{value:?}: {error}"))
    };

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_5);
    builder.set_document_language(language("en-US"));
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 700.0, "See ");
    let properties = SpanProperties::new()
        .with_alternate_text("six-point star")
        .with_actual_text("star")
        .with_expansion("St.")
        .with_language(language("es-MX"));
    builder.begin_span(page, &properties);
    builder.text(page, &Font::helvetica(12.0), 110.0, 700.0, "A");
    builder.end_span(page);
    builder.text(page, &Font::helvetica(12.0), 120.0, 700.0, " later");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let doc = parse_buf(&buf);

    let sections = doc.page_marked_content(0).expect("marked content");
    assert_eq!(sections.len(), 1, "{sections:?}");
    let span = &sections[0];
    assert_eq!(span.tag, "Span");
    let parsed = span
        .span_properties()
        .expect("typed span properties")
        .expect("Span property list");
    assert_eq!(parsed.alternate_text.as_deref(), Some("six-point star"));
    assert_eq!(parsed.actual_text.as_deref(), Some("star"));
    assert_eq!(parsed.expansion.as_deref(), Some("St."));
    assert_eq!(parsed.language, Some(language("es-MX")));

    assert_eq!(
        doc.page_marked_content_languages(0).expect("languages"),
        vec![Some(language("es-MX"))]
    );
    assert_eq!(
        doc.extract_page_text(0).as_deref(),
        Some("See \nstar\n later"),
        "ActualText 应替换 Span 内的可见文本；不同 BT/ET 文本块之间由提取器插入换行"
    );
}

#[test]
fn builder_artifact_property_lists_roundtrip_through_parser() {
    use pdf_engine::core::builder::{
        ArtifactAttachment, ArtifactProperties, ArtifactSubtype, ArtifactType, Font, PageSize,
        PdfBuilder,
    };
    use pdf_engine::core::reader::accessibility::{
        ArtifactAttachment as ReaderAttachment, ArtifactSubtype as ReaderSubtype,
        ArtifactType as ReaderType,
    };
    use pdf_engine::core::types::{PdfFeature, PdfVersion};

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 700.0, "Body ");
    let properties = ArtifactProperties::new(ArtifactType::Pagination)
        .with_bounding_box([0.0, 0.0, 612.0, 792.0])
        .with_attachment(ArtifactAttachment::Top)
        .with_attachment(ArtifactAttachment::Bottom)
        .with_subtype(ArtifactSubtype::Header);
    builder.begin_artifact(page, &properties);
    builder.text(page, &Font::helvetica(9.0), 72.0, 680.0, "Running head");
    builder.end_artifact(page);
    builder.text(page, &Font::helvetica(12.0), 72.0, 660.0, " later");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let doc = parse_buf(&buf);

    let sections = doc.page_marked_content(0).expect("marked content");
    let artifact = sections
        .iter()
        .find(|section| section.tag == "Artifact")
        .expect("Artifact section");
    let parsed = artifact
        .artifact_properties()
        .expect("typed artifact properties")
        .expect("Artifact property list");
    assert_eq!(parsed.artifact_type, Some(ReaderType::Pagination));
    assert_eq!(parsed.bounding_box, Some([0.0, 0.0, 612.0, 792.0]));
    assert_eq!(
        parsed.attached,
        vec![ReaderAttachment::Top, ReaderAttachment::Bottom]
    );
    assert_eq!(parsed.subtype, Some(ReaderSubtype::Header));

    assert_eq!(
        doc.extract_page_text(0).as_deref(),
        Some("Body \n later"),
        "Artifact 内的文本不参与提取；不同 BT/ET 文本块之间由提取器插入换行"
    );

    let features = doc.features.borrow();
    assert!(
        features.contains(PdfFeature::ArtifactPropertyLists),
        "特征画像应登记 Artifact 属性表"
    );
    assert!(
        features.contains(PdfFeature::ArtifactSubtype),
        "特征画像应登记 Artifact /Subtype"
    );
}
