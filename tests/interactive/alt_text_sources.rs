//! 14.9.3–14.9.4 替代文本来源解析的端到端往返：builder 写出 Span
//! 属性表、注解 `/Contents`、字段 `/TU` 与结构元素 `/Alt`/`/ActualText`，
//! 重新解析后经 `Document::page_marked_content_alternatives`、
//! `Annotation::resolved_alternative_text`、`Field::resolved_alternative_text`
//! 同时断言生效文本与来源出处（provenance）。

use super::*;

/// 写出 Span `/Alt` + `/ActualText`、纯空白 `/Alt`、Text 注解 `/Contents`
/// （带 ja 转义）与字段 `/TU` → 重新解析 → 各载体的文本、出处与语言
/// 逐一对号；空白 `/Alt` 降级为类型化问题而不是胜出文本。
#[test]
fn builder_span_annotation_and_field_alternatives_roundtrip_with_provenance() {
    use pdf_engine::core::builder::{
        AcroForm, AnnotationKind, Font, FormField, PageSize, PdfAnnotation, PdfBuilder,
        SpanProperties,
    };
    use pdf_engine::core::reader::accessibility::{
        AlternativeTextProblem, AlternativeTextSource, LanguageIdentifier,
    };
    use pdf_engine::core::reader::annotation::AnnotationSubtype;
    use pdf_engine::core::types::PdfVersion;

    let language = |value: &str| {
        LanguageIdentifier::parse(value).unwrap_or_else(|error| panic!("{value:?}: {error}"))
    };

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_5);
    builder.set_document_language(language("en-US"));
    let page = builder.page(PageSize::A4);
    builder.begin_span(
        page,
        &SpanProperties::new()
            .with_alternate_text("six-point star")
            .with_actual_text("Druk"),
    );
    builder.text(page, &Font::helvetica(12.0), 72.0, 700.0, "A");
    builder.end_span(page);
    builder.begin_span(page, &SpanProperties::new().with_alternate_text("   "));
    builder.text(page, &Font::helvetica(12.0), 72.0, 680.0, "B");
    builder.end_span(page);
    builder.add_annotation(
        page,
        PdfAnnotation::new(AnnotationKind::Text, [72.0, 600.0, 92.0, 620.0])
            .with_contents("\u{1b}ja\u{1b}Review note"),
    );
    let mut form = AcroForm::new();
    form.add_field(
        page,
        FormField::text("f1", [72.0, 500.0, 240.0, 520.0])
            .with_value("x")
            .with_alternate_name("Your name"),
    );
    builder.set_acro_form(form);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let doc = parse_buf(&buf);
    let prevailing = doc.document_language().expect("catalog /Lang");

    let resolutions = doc
        .page_marked_content_alternatives(0)
        .expect("page alternatives");
    assert_eq!(resolutions.len(), 2, "{resolutions:?}");

    let star = &resolutions[0];
    assert_eq!(
        star.description
            .as_ref()
            .map(|a| (a.source, a.text.as_str())),
        Some((AlternativeTextSource::MarkedContentAlt, "six-point star",)),
        "14.9.3：Span 属性表 /Alt 是替代描述"
    );
    assert_eq!(
        star.replacement
            .as_ref()
            .map(|a| (a.source, a.text.as_str())),
        Some((AlternativeTextSource::MarkedContentActualText, "Druk")),
        "14.9.4：/ActualText 独立占替换槽，不与 /Alt 混同"
    );
    assert_eq!(
        star.description.as_ref().and_then(|a| a.language.as_ref()),
        Some(&language("en-US")),
        "无转义时沿用目录 /Lang"
    );
    assert!(star.problems.is_empty(), "{:?}", star.problems);

    let blank = &resolutions[1];
    assert_eq!(blank.description, None, "纯空白 /Alt 不得胜出");
    assert_eq!(
        blank.problems,
        vec![AlternativeTextProblem::WhitespaceOnly {
            source: AlternativeTextSource::MarkedContentAlt,
        }],
        "空白值必须是类型化问题"
    );

    let annotation = doc
        .page_annotations(0)
        .expect("page annotations")
        .into_iter()
        .find(|annotation| annotation.subtype == AnnotationSubtype::Text)
        .expect("Text annotation");
    let resolved = annotation.resolved_alternative_text(prevailing.as_ref());
    assert_eq!(
        resolved
            .description
            .as_ref()
            .map(|a| (a.source, a.text.as_str())),
        Some((AlternativeTextSource::AnnotationContents, "Review note")),
        "14.9.3：注解 /Contents 是描述来源，转义已剥离"
    );
    assert_eq!(
        resolved
            .description
            .as_ref()
            .and_then(|a| a.language.as_ref()),
        Some(&language("ja")),
        "14.9.3 shall：/Contents 内转义覆盖 prevailing Lang"
    );
    assert!(resolved.problems.is_empty(), "{:?}", resolved.problems);

    let form = doc.acro_form().expect("acro form").expect("present");
    let field = form.fields.first().expect("one field");
    let field_resolution = field.resolved_alternative_text(prevailing.as_ref());
    assert_eq!(
        field_resolution
            .description
            .as_ref()
            .map(|a| (a.source, a.text.as_str())),
        Some((AlternativeTextSource::FieldAlternateName, "Your name")),
        "Table 220/14.9.3：/TU 替代字段名是字段一侧的出处"
    );
    assert_eq!(
        field_resolution
            .description
            .as_ref()
            .and_then(|a| a.language.as_ref()),
        Some(&language("en-US"))
    );
}

/// 结构内容的 `/MCID` 经父树定位结构元素：`/Alt` 与 `/ActualText` 都以
/// `StructureElement*` 出处胜出，元素 `/Lang` 作为 prevailing 随条目返回。
#[test]
fn builder_structure_element_alternatives_resolve_via_parent_tree_roundtrip() {
    use pdf_engine::core::builder::{
        Font, PageSize, PdfBuilder, StructureElement, StructureMarkInfo, StructureTree,
    };
    use pdf_engine::core::reader::accessibility::{AlternativeTextSource, LanguageIdentifier};
    use pdf_engine::core::types::PdfVersion;

    let language = |value: &str| {
        LanguageIdentifier::parse(value).unwrap_or_else(|error| panic!("{value:?}: {error}"))
    };

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_5);
    builder.set_document_language(language("en-US"));
    let page = builder.page(PageSize::A4);
    builder.begin_structure_content(page, "P", 0);
    builder.text(page, &Font::helvetica(12.0), 72.0, 700.0, "Tagged line");
    builder.end_structure_content(page);
    builder.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document").with_element(
                StructureElement::new("P")
                    .with_marked_content(page, 0)
                    .with_lang("de-DE")
                    .with_alt("structured description")
                    .with_actual_text("structured replacement"),
            ),
        ),
    );
    builder.set_mark_info(StructureMarkInfo::new().with_marked(true));

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build tagged page");
    let doc = parse_buf(&buf);

    let resolutions = doc
        .page_marked_content_alternatives(0)
        .expect("page alternatives");
    assert_eq!(resolutions.len(), 1, "{resolutions:?}");
    let resolution = &resolutions[0];
    assert_eq!(
        resolution
            .description
            .as_ref()
            .map(|a| (a.source, a.text.as_str())),
        Some((
            AlternativeTextSource::StructureElementAlt,
            "structured description",
        ))
    );
    assert_eq!(
        resolution
            .replacement
            .as_ref()
            .map(|a| (a.source, a.text.as_str())),
        Some((
            AlternativeTextSource::StructureElementActualText,
            "structured replacement",
        )),
        "两槽并存：描述与替换各按 14.9.3/14.9.4 归位"
    );
    assert_eq!(
        resolution
            .description
            .as_ref()
            .and_then(|a| a.language.as_ref()),
        Some(&language("de-DE")),
        "结构内容的 prevailing 语言取元素 /Lang（14.9.2.3）"
    );
    assert!(resolution.problems.is_empty(), "{:?}", resolution.problems);

    // 越界页码是类型化错误，不是 panic。
    assert!(doc.page_marked_content_alternatives(7).is_err());
}
