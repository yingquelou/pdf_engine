//! Artifact 结构归属诊断的写出 → 解析 → 回读往返（ISO 32000-1 14.8.2.2.1、
//! 14.8.2.3.1）。
//!
//! 写端给出两类页面：一类只有类型化 Artifact 区段（属性表按 Table 330 写出，
//! 不含 `/MCID`），解析后 `artifact_attribution_violations` 为空；另一类在同一
//! 页上再包一段没有 `/MCID` 的 `Span` 标记内容，解析后回读到唯一的
//! `RealContentNotInStructure`（should 级）发现，且 Artifact 内文本仍不参与提取。
//! Artifact 携带 `/MCID` 的两条 shall 级规则需要页面流里出现结构身份，写端
//! 没有把 `/MCID` 写进 Artifact 属性表的通道，其正向命中由
//! `core::reader::accessibility::artifact_attribution` 的单元测试用手工夹具覆盖。

use super::*;

#[test]
fn builder_artifact_attribution_roundtrip_through_parser() {
    use pdf_engine::core::builder::{
        ArtifactProperties, ArtifactType, Font, PageSize, PdfBuilder, SpanProperties,
        StructureElement, StructureMarkInfo, StructureTree,
    };
    use pdf_engine::core::reader::accessibility::{
        ArtifactAttributionRule, ArtifactAttributionViolation,
    };
    use pdf_engine::core::types::PdfVersion;

    let artifact = ArtifactProperties::new(ArtifactType::Pagination);
    let tree = || {
        StructureTree::new().with_element(
            StructureElement::new("Document")
                .with_element(StructureElement::new("P").with_marked_content(0, 0)),
        )
    };

    // 合规页：Artifact 区段没有结构身份，页面上也没有游离的标记区段。
    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 700.0, "Body");
    builder.begin_artifact(page, &artifact);
    builder.text(page, &Font::helvetica(9.0), 72.0, 680.0, "Running head");
    builder.end_artifact(page);
    builder.set_structure_tree(tree());
    builder.set_mark_info(StructureMarkInfo::new().with_marked(true));
    let mut buf = Vec::new();
    builder
        .build(&mut buf)
        .expect("build page with artifact only");
    let doc = parse_buf(&buf);
    assert_eq!(
        doc.artifact_attribution_violations(0)
            .expect("diagnose compliant page"),
        vec![],
        "Artifact 未携带 /MCID 且无游离标记区段时不应有归属发现"
    );
    assert!(
        doc.mark_info().expect("mark info").expect("present").marked,
        "/MarkInfo /Marked 应随结构树写出"
    );
    assert_eq!(
        doc.extract_page_text(0).as_deref(),
        Some("Body"),
        "Artifact 内的文本不参与提取"
    );

    // 违规页：再包一段没有 /MCID 的 Span 标记内容——真实内容未进逻辑结构。
    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = builder.page(PageSize::A4);
    let second = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 700.0, "Body");
    builder.begin_artifact(page, &artifact);
    builder.text(page, &Font::helvetica(9.0), 72.0, 680.0, "Running head");
    builder.end_artifact(page);
    builder.begin_span(page, &SpanProperties::new().with_actual_text("star"));
    builder.text(page, &Font::helvetica(12.0), 72.0, 660.0, "*");
    builder.end_span(page);
    builder.text(second, &Font::helvetica(12.0), 72.0, 700.0, "Next page");
    builder.set_structure_tree(tree());
    builder.set_mark_info(StructureMarkInfo::new().with_marked(true));
    let mut buf = Vec::new();
    builder
        .build(&mut buf)
        .expect("build page with unstructured span");
    let doc = parse_buf(&buf);

    let sections = doc.page_marked_content(0).expect("marked content");
    let span_index = sections
        .iter()
        .position(|section| section.tag == "Span")
        .expect("Span section written");
    assert_eq!(
        doc.artifact_attribution_violations(0)
            .expect("diagnose violating page"),
        vec![ArtifactAttributionViolation {
            page: 0,
            section: Some(span_index),
            element: None,
            kid: None,
            rule: ArtifactAttributionRule::RealContentNotInStructure,
        }],
        "没有 /MCID 且祖先也不是结构内容的 Span 应被记为真实内容未入结构"
    );
    let found = doc
        .artifact_attribution_violations(0)
        .expect("diagnose violating page");
    assert!(
        !found[0].rule.is_mandatory(),
        "真实内容未入结构只是 14.8.2.2.1 的 should 级发现"
    );
    assert_eq!(
        doc.artifact_attribution_violations(1)
            .expect("diagnose unmarked page"),
        vec![],
        "完全没有标记内容的页面不应有归属发现"
    );
    assert_eq!(
        doc.extract_page_text(0).as_deref(),
        Some("Body\nstar"),
        "Span 内的可见文本由 ActualText 替换，Artifact 内文本仍被排除"
    );
}
