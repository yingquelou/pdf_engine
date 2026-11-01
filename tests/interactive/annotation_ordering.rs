//! 注解页面内容序列（ISO 32000-1 14.8.2.3.2）写出 → 解析 → 回读往返。
//!
//! 写端用 14.7.4.2 的结构标记内容与 14.7.4.3 的 OBJR 内容项造出规范承认的
//! 三种情形，读端 `Document::annotation_content_order` 在同一份字节上给出
//! 一致的位置：
//!
//! - `Annot`（PDF 1.5）元素同时挂 MCR 与 OBJR：显式关联，注解紧贴其关联
//!   序列之后（`AnnotationOrderBasis::ExplicitAssociation`）。
//! - 非关联型 `Note` 元素在带区段元素之前：邻接在前
//!   （`AnnotationOrderBasis::AdjacentPrecedes`）。
//! - 规范 NOTE 的空标记内容序列：写端只为定位相邻注解放一段空序列，读端
//!   既认出锚点（`MarkedContentPosition::positioning_anchor`）也让注解落在
//!   其后。
//! - 没有逻辑结构树的页面：返回空序列而非错误；越界页码返回类型化错误。

use super::*;

/// 把回读序列压成 `MC`/`A` 简写，便于与预期的页面内容顺序整体比对。
fn shape(items: &[pdf_engine::core::reader::accessibility::ContentOrderItem]) -> Vec<String> {
    use pdf_engine::core::reader::accessibility::ContentOrderItem;
    items
        .iter()
        .map(|item| match item {
            ContentOrderItem::MarkedContent(position) => format!("MC{}", position.section),
            ContentOrderItem::Annotation(position) => format!("A{}", position.annotation),
        })
        .collect()
}

#[test]
fn builder_annot_object_reference_roundtrip_orders_annotation_after_its_sequence() {
    use pdf_engine::core::builder::{
        AnnotationKind, Font, PageSize, PdfAnnotation, PdfBuilder, StructureElement,
        StructureMarkInfo, StructureTree,
    };
    use pdf_engine::core::reader::accessibility::{AnnotationOrderBasis, ContentOrderItem};
    use pdf_engine::core::types::PdfVersion;

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = builder.page(PageSize::A4);
    builder.begin_structure_content(page, "P", 0);
    builder.text(page, &Font::helvetica(12.0), 72.0, 700.0, "First line");
    builder.end_structure_content(page);
    builder.begin_structure_content(page, "P", 1);
    builder.text(page, &Font::helvetica(12.0), 72.0, 680.0, "Second line");
    builder.end_structure_content(page);
    let annotation = builder.add_annotation(
        page,
        PdfAnnotation::new(AnnotationKind::Text, [72.0, 660.0, 92.0, 680.0])
            .with_contents("Inline note"),
    );
    builder.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document")
                .with_element(StructureElement::new("P").with_marked_content(page, 0))
                .with_element(
                    StructureElement::new("Annot")
                        .with_marked_content(page, 1)
                        .with_object_reference(annotation),
                ),
        ),
    );
    builder.set_mark_info(StructureMarkInfo::new().with_marked(true));

    let mut buf = Vec::new();
    builder
        .build(&mut buf)
        .expect("build tagged page with Annot element and OBJR");
    let raw = String::from_utf8_lossy(&buf).into_owned();
    for entry in ["/S /Annot", "/Type /OBJR", "/StructParent 1"] {
        assert!(raw.contains(entry), "缺少 {entry}: {raw}");
    }
    let doc = parse_buf(&buf);

    let items = doc
        .annotation_content_order(page)
        .expect("annotation order on writer produced page");
    assert_eq!(
        shape(&items),
        vec!["MC0", "MC1", "A0"],
        "显式关联把注解贴在关联序列（MCID 1）之后"
    );
    let ContentOrderItem::Annotation(position) = &items[2] else {
        panic!("末条应是注解");
    };
    assert_eq!(position.basis, AnnotationOrderBasis::ExplicitAssociation);
    assert_eq!(
        position.associated_sequence,
        Some(1),
        "关联的是 MCID 1 的序列"
    );
    assert_eq!(position.annotation, 0, "本页 /Annots 下标");
    // arena 前序：0 = Document、1 = P、2 = Annot；OBJR 是 /K 的第二个子项。
    assert_eq!(position.element, Some(2));
    assert_eq!(position.kid, Some(1));
    let ContentOrderItem::MarkedContent(second) = &items[1] else {
        panic!("第二条应是 MCID 1 的区段");
    };
    assert!(!second.positioning_anchor, "两段都是实内容");
    assert_eq!(second.element, Some(2), "区段与注解由同一个 Annot 元素认领");

    assert_eq!(
        doc.extract_page_text(page).as_deref(),
        Some("First line\nSecond line"),
        "结构标注不影响文本提取"
    );
}

#[test]
fn builder_adjacent_annotation_element_roundtrip_orders_annotation_before_sequence() {
    use pdf_engine::core::builder::{
        AnnotationKind, Font, PageSize, PdfAnnotation, PdfBuilder, StructureElement,
        StructureMarkInfo, StructureTree,
    };
    use pdf_engine::core::reader::accessibility::AnnotationOrderBasis;
    use pdf_engine::core::types::PdfVersion;

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = builder.page(PageSize::A4);
    builder.begin_structure_content(page, "P", 0);
    builder.text(page, &Font::helvetica(12.0), 72.0, 700.0, "Only line");
    builder.end_structure_content(page);
    let annotation = builder.add_annotation(
        page,
        PdfAnnotation::new(AnnotationKind::Text, [72.0, 690.0, 200.0, 706.0]),
    );
    // Note 元素（非 Annot/Link/Form）只挂 OBJR，且逻辑结构序中紧接在带区段
    // 的 P 元素之前：14.8.2.3.2 的邻接句把注解排在序列之前。
    builder.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document")
                .with_element(StructureElement::new("Note").with_object_reference(annotation))
                .with_element(StructureElement::new("P").with_marked_content(page, 0)),
        ),
    );
    builder.set_mark_info(StructureMarkInfo::new().with_marked(true));

    let mut buf = Vec::new();
    builder
        .build(&mut buf)
        .expect("build page with annotation element preceding marked content");
    let doc = parse_buf(&buf);

    let items = doc
        .annotation_content_order(page)
        .expect("annotation order for adjacency");
    assert_eq!(shape(&items), vec!["A0", "MC0"]);
    let pdf_engine::core::reader::accessibility::ContentOrderItem::Annotation(position) = &items[0]
    else {
        panic!("首条应是注解");
    };
    assert_eq!(position.basis, AnnotationOrderBasis::AdjacentPrecedes);
    assert_eq!(position.associated_sequence, Some(0));
    assert!(
        !position.basis.is_explicit(),
        "邻接档不是 Annot/Link/Form 的显式关联"
    );
    assert_eq!(
        doc.extract_page_text(page).as_deref(),
        Some("Only line"),
        "邻接排序不改动内容流"
    );
}

#[test]
fn builder_empty_marked_content_sequence_roundtrip_anchors_adjacent_annotation() {
    use pdf_engine::core::builder::{
        AnnotationKind, Font, PageSize, PdfAnnotation, PdfBuilder, StructureElement,
        StructureMarkInfo, StructureTree,
    };
    use pdf_engine::core::reader::accessibility::{AnnotationOrderBasis, ContentOrderItem};
    use pdf_engine::core::types::PdfVersion;

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = builder.page(PageSize::A4);
    builder.begin_structure_content(page, "P", 0);
    builder.text(page, &Font::helvetica(12.0), 72.0, 700.0, "Real content");
    builder.end_structure_content(page);
    // 规范 NOTE：只为定位相邻注解而写的空标记内容序列（BDC 后紧跟 EMC）。
    builder.begin_structure_content(page, "Anchor", 1);
    builder.end_structure_content(page);
    let annotation = builder.add_annotation(
        page,
        PdfAnnotation::new(AnnotationKind::Text, [72.0, 660.0, 92.0, 680.0]),
    );
    builder.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document")
                .with_element(StructureElement::new("P").with_marked_content(page, 0))
                .with_element(StructureElement::new("P").with_marked_content(page, 1))
                .with_element(StructureElement::new("Note").with_object_reference(annotation)),
        ),
    );
    builder.set_mark_info(StructureMarkInfo::new().with_marked(true));

    let mut buf = Vec::new();
    builder
        .build(&mut buf)
        .expect("build page with empty anchor marked-content sequence");
    let raw = String::from_utf8_lossy(&buf).into_owned();
    assert!(
        raw.contains("/Anchor << /MCID 1 >> BDC"),
        "空序列应写出定界符与 MCID: {raw}"
    );
    let doc = parse_buf(&buf);
    assert!(
        doc.page_marked_content(page)
            .expect("marked content")
            .iter()
            .any(|section| section.tag == "Anchor"),
        "空标记内容序列照样出现在区段列表里"
    );

    let items = doc
        .annotation_content_order(page)
        .expect("annotation order with positioning anchor");
    assert_eq!(
        shape(&items),
        vec!["MC0", "MC1", "A0"],
        "空序列占位，注解紧接其后"
    );
    let ContentOrderItem::MarkedContent(anchor) = &items[1] else {
        panic!("第二条应是定位锚点");
    };
    assert!(
        anchor.positioning_anchor,
        "MCID 1 的区段内没有任何算子，应识别为规范 NOTE 的锚点"
    );
    let ContentOrderItem::Annotation(position) = &items[2] else {
        panic!("末条应是注解");
    };
    assert_eq!(position.basis, AnnotationOrderBasis::AdjacentFollows);
    assert_eq!(
        position.associated_sequence,
        Some(1),
        "锚点序列就是注解的定位依据"
    );

    assert_eq!(
        doc.extract_page_text(page).as_deref(),
        Some("Real content"),
        "空序列不引入文本"
    );
}

#[test]
fn builder_page_without_structure_tree_roundtrip_yields_empty_annotation_order() {
    use pdf_engine::core::builder::{AnnotationKind, Font, PageSize, PdfAnnotation, PdfBuilder};
    use pdf_engine::core::reader::accessibility::AnnotationOrderingError;
    use pdf_engine::core::types::PdfVersion;

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 700.0, "Untagged page");
    builder.add_annotation(
        page,
        PdfAnnotation::new(AnnotationKind::Text, [72.0, 660.0, 92.0, 680.0]),
    );

    let mut buf = Vec::new();
    builder
        .build(&mut buf)
        .expect("build page with annotation but no structure tree");
    let doc = parse_buf(&buf);

    assert!(
        doc.structure_tree().expect("structure tree read").is_none(),
        "该文档没有 /StructTreeRoot"
    );
    assert_eq!(
        doc.annotation_content_order(page)
            .expect("no structure tree is not an error"),
        vec![],
        "没有逻辑结构就没有可导出的位置，返回空序列"
    );
    let error = doc
        .annotation_content_order(page + 7)
        .expect_err("越界页码应是类型化错误");
    assert!(
        matches!(error, AnnotationOrderingError::Page(_)),
        "越界页码返回 Page 变体，实际 {error:?}"
    );
    assert_eq!(
        doc.page_annotations(page).expect("page annotations").len(),
        1,
        "注解仍在页 /Annots 里，只是位置无从导出"
    );
}
