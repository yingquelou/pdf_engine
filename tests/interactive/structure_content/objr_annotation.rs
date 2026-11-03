//! 14.7.4.3 OBJR 对象引用与 14.8.2.2.1 注解归属的往返（注解形态）。

use super::*;

#[test]
fn builder_object_reference_roundtrip_links_struct_parent() {
    use pdf_engine::core::builder::{
        AnnotationKind, Font, PageSize, PdfAnnotation, PdfBuilder, StructureElement,
        StructureMarkInfo, StructureTree,
    };
    use pdf_engine::core::reader::structure::{ParentTreeEntry, StructureKid};
    use pdf_engine::core::types::{PdfVersion, Value};

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = builder.page(PageSize::A4);
    builder.begin_structure_content(page, "P", 0);
    builder.text(page, &Font::helvetica(12.0), 72.0, 700.0, "Tagged line");
    builder.end_structure_content(page);
    let annotation = builder.add_annotation(
        page,
        PdfAnnotation::new(AnnotationKind::Text, [72.0, 660.0, 92.0, 680.0])
            .with_contents("Figure note"),
    );
    builder.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document").with_element(
                StructureElement::new("P")
                    .with_marked_content(page, 0)
                    .with_object_reference(annotation),
            ),
        ),
    );
    builder.set_mark_info(StructureMarkInfo::new().with_marked(true));

    let mut buf = Vec::new();
    builder
        .build(&mut buf)
        .expect("build page with OBJR structure content");
    let raw = String::from_utf8_lossy(&buf).into_owned();
    for entry in [
        "/Type /OBJR",
        "/StructParent 1",
        "/StructParents 0",
        "/ParentTreeNextKey 2",
    ] {
        assert!(raw.contains(entry), "缺少 {entry}: {raw}");
    }
    let doc = parse_buf(&buf);

    let tree = doc
        .structure_tree()
        .expect("structure tree")
        .expect("structure tree present");
    // arena 前序：0 = Document、1 = P。
    let annotations = doc.page_annotations(page).expect("page annotations");
    assert_eq!(annotations.len(), 1, "页上唯一注解");
    let element = &tree.elements()[1];
    let (mcr, objr) = match element.kids.as_slice() {
        [
            StructureKid::MarkedContent(mcr),
            StructureKid::ObjectReference(objr),
        ] => (mcr, objr),
        other => panic!("元素 1 应为 MCR + OBJR 两个内容项，实际 {other:?}"),
    };
    assert_eq!(
        objr.object,
        annotations[0].reference.expect("indirect annotation"),
        "/Obj 指向注解对象本身"
    );
    assert_eq!(
        objr.dict.get("Type"),
        Some(&Value::Name("OBJR".into())),
        "OBJR 字典带 /Type /OBJR"
    );
    let objr_page = objr.page.expect("OBJR 带显式 /Pg");
    assert_eq!(objr_page, mcr.page.expect("MCR 带显式 /Pg"));
    let page_dict = doc
        .get_object(objr_page.0)
        .expect("read /Pg object")
        .expect("/Pg object exists");
    assert_eq!(
        page_dict
            .value
            .as_dict()
            .and_then(|dict| dict.get("Type"))
            .and_then(Value::as_name),
        Some("Page"),
        "/Pg 解析到页对象"
    );
    assert_eq!(
        annotations[0]
            .dict
            .get("StructParent")
            .and_then(Value::as_integer),
        Some(1),
        "注解字典追加父树键 1（页面键 0 之后）"
    );

    assert!(
        matches!(tree.parent_tree_entry(1), Some(ParentTreeEntry::Object(_)),),
        "OBJR 键的父树条目是单元引用，实际 {:?}",
        tree.parent_tree_entry(1)
    );
    assert_eq!(tree.object_parent(1), Some(1), "单元引用指向元素 P");
    assert_eq!(tree.mcid_parent(1, 0), None, "单元条目不参与 MCID 查找");

    let parents = doc
        .page_struct_parents(page)
        .expect("page parents")
        .expect("/StructParents");
    assert_eq!(parents.key, 0, "唯一带标记内容的页面得到父树键 0");
    assert_eq!(parents.parents, vec![Some(1)]);
    assert_eq!(tree.mcid_parent(0, 0), Some(1), "页面 MCID 归属仍成立");
    assert_eq!(
        tree.root().parent_tree_next_key,
        Some(2),
        "/ParentTreeNextKey 覆盖页面键与 OBJR 键"
    );

    assert_eq!(
        doc.extract_page_text(page).as_deref(),
        Some("Tagged line"),
        "OBJR 内容项不影响页文本提取"
    );
}

/// 可见注解被 OBJR 正确声称的往返：14.8.2.2.1 的三条件齐备
/// （`/AP /N`、未置 Hidden、纳入逻辑结构），回读后归属诊断整体为空，
/// 既有标记内容与文本提取不受影响。
#[test]
fn builder_visible_annotation_object_reference_roundtrip_keeps_document_conforming() {
    let buf = build_annotation_appearance_page(true, false);
    let doc = parse_buf(&buf);

    let annotations = doc.page_annotations(0).expect("page annotations");
    assert_eq!(annotations.len(), 1, "页上唯一注解");
    assert!(
        annotations[0]
            .appearance
            .as_ref()
            .is_some_and(|streams| streams.normal.is_some()),
        "回读注解应带常态 (N) 外观流"
    );
    assert!(!annotations[0].flags.hidden(), "/F 未置 Hidden 位");
    assert_eq!(
        doc.artifact_attribution_violations(0)
            .expect("diagnose writer produced page"),
        vec![],
        "可见注解经 OBJR + /StructParent + 父树单元条目正确声称，七条归属诊断应为空"
    );
    assert_eq!(
        doc.extract_page_text(0).as_deref(),
        Some("Tagged line"),
        "注解外观与 OBJR 声称不影响页文本提取"
    );
}

/// 去掉 OBJR 声称的同一文档：可见注解不满足 14.8.2.2.1 第三个条件，
/// 回读后应恰好命中一条 `VisibleAnnotationNotInStructure`（should 级），
/// 其余规则保持干净。
#[test]
fn builder_visible_annotation_without_object_reference_roundtrip_reports_missing_structure() {
    use pdf_engine::core::reader::accessibility::{
        ArtifactAttributionRule, ArtifactAttributionViolation,
    };

    let buf = build_annotation_appearance_page(false, false);
    let doc = parse_buf(&buf);

    let found = doc
        .artifact_attribution_violations(0)
        .expect("diagnose writer produced page");
    assert_eq!(
        found,
        vec![ArtifactAttributionViolation {
            page: 0,
            section: None,
            element: None,
            kid: None,
            annotation: Some(0),
            rule: ArtifactAttributionRule::VisibleAnnotationNotInStructure,
        }],
        "未被逻辑结构纳入的可见注解应只记一条规则 6 发现，annotation 为 /Annots 下标"
    );
    assert!(
        !found[0].rule.is_mandatory(),
        "14.8.2.2.1 的注解条件派生自定义式表述，应为 should 级"
    );
    assert_eq!(
        doc.extract_page_text(0).as_deref(),
        Some("Tagged line"),
        "文本提取不受归属诊断影响"
    );
}

/// Hidden 注解即使带 `/AP /N` 也不属于真实内容（14.8.2.2.1 第二条件
/// 不成立）：不声称、不置结构，回读后也没有注解归属发现。
#[test]
fn builder_hidden_annotation_appearance_roundtrip_reports_no_annotation_finding() {
    let buf = build_annotation_appearance_page(false, true);
    let doc = parse_buf(&buf);

    let annotations = doc.page_annotations(0).expect("page annotations");
    assert!(annotations[0].flags.hidden(), "回读注解应带 Hidden 位");
    assert!(
        annotations[0]
            .appearance
            .as_ref()
            .is_some_and(|streams| streams.normal.is_some()),
        "外观流仍在，只是可见性条件不成立"
    );
    assert_eq!(
        doc.artifact_attribution_violations(0)
            .expect("diagnose writer produced page"),
        vec![],
        "Hidden 注解不要求进入逻辑结构，归属诊断应为空"
    );
}
