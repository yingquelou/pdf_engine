//! 结构标记内容写出 → 解析 → 回读往返（ISO 32000-1 14.8.2、14.8.2.3.1、14.7.4）。
//!
//! 页面内容流以 `/P << /MCID n >> BDC` 标记两段真实内容，结构树用 MCR 子项
//! 声明同一页的 MCID 0 与 1：解析后页面 `/StructParents`、目录 `/ParentTree`
//! 与页上标记内容三者对齐，14.8.2 的归属诊断在写端产物上为空，文本提取
//! 不受标记影响。
//!
//! 另一条往返覆盖 14.8.2.3.1 的 tag suspects：页上混入
//! `/TagSuspect << /TagSuspect /Ordering >> BDC` 且目录写出
//! `/MarkInfo << /Suspects true >>`，回读后属性表形态、`/Suspects` 声明与
//! 归属诊断的空结果同时成立。
//!
//! 第三条往返覆盖 14.7.4.3 的 OBJR 内容项（Table 325）：同一结构元素同时
//! 挂 MCR 与注解对象引用，写出 `/K` 的 `/Type /OBJR` 字典、注解字典的
//! `/StructParent`（Table 326）与父树的单元型条目；回读后 `/Obj` 指向该
//! 注解对象、`/Pg` 与 MCR 同页、`/ParentTreeNextKey` 覆盖页面键 + OBJR 键，
//! 页面 MCID 归属与文本提取不受影响。

use super::*;

#[test]
fn builder_structure_marked_content_roundtrip_links_parent_tree() {
    use pdf_engine::core::builder::{
        Font, PageSize, PdfBuilder, StructureElement, StructureMarkInfo, StructureTree,
    };
    use pdf_engine::core::reader::structure::StructureKid;
    use pdf_engine::core::types::PdfVersion;

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = builder.page(PageSize::A4);
    builder.begin_structure_content(page, "P", 0);
    builder.text(page, &Font::helvetica(12.0), 72.0, 700.0, "First line");
    builder.end_structure_content(page);
    builder.begin_structure_content(page, "P", 1);
    builder.text(page, &Font::helvetica(12.0), 72.0, 680.0, "Second line");
    builder.end_structure_content(page);
    builder.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document")
                .with_element(StructureElement::new("P").with_marked_content(page, 0))
                .with_element(StructureElement::new("Span").with_marked_content(page, 1)),
        ),
    );
    builder.set_mark_info(StructureMarkInfo::new().with_marked(true));

    let mut buf = Vec::new();
    builder
        .build(&mut buf)
        .expect("build tagged page with /MCID marked content");
    let raw = String::from_utf8_lossy(&buf).into_owned();
    for entry in [
        "/P << /MCID 0 >> BDC",
        "/P << /MCID 1 >> BDC",
        "/StructParents 0",
        "/ParentTreeNextKey 1",
    ] {
        assert!(raw.contains(entry), "缺少 {entry}: {raw}");
    }
    let doc = parse_buf(&buf);

    let tree = doc
        .structure_tree()
        .expect("structure tree")
        .expect("structure tree present");
    // arena 前序：0 = Document、1 = P、2 = Span。
    let document = &tree.elements()[0];
    assert!(
        matches!(
            document.kids[..],
            [StructureKid::Element(1), StructureKid::Element(2)]
        ),
        "{:?}",
        document.kids
    );
    for (element, mcid) in [(1, 0), (2, 1)] {
        match &tree.elements()[element].kids[..] {
            [StructureKid::MarkedContent(mcr)] => {
                assert_eq!(mcr.mcid, mcid, "元素 {element} 的 MCR MCID");
                assert!(mcr.page.is_some(), "MCR 应带显式 /Pg");
            }
            other => panic!("元素 {element} 应只有一个 MCR，实际 {other:?}"),
        }
    }
    let parents = doc
        .page_struct_parents(page)
        .expect("page parents")
        .expect("/StructParents");
    assert_eq!(parents.key, 0, "唯一带标记内容的页面得到父树键 0");
    assert_eq!(parents.parents, vec![Some(1), Some(2)]);
    assert_eq!(tree.mcid_parent(parents.key, 0), Some(1));
    assert_eq!(tree.mcid_parent(parents.key, 1), Some(2));
    assert_eq!(tree.mcid_parent(parents.key, 2), None);

    let sections = doc.page_marked_content(page).expect("marked content");
    let paragraphs: Vec<Option<i64>> = sections
        .iter()
        .filter(|section| section.tag == "P")
        .map(|section| {
            section
                .properties
                .as_ref()
                .and_then(|dict| dict.get("MCID"))
                .and_then(|value| value.as_integer())
        })
        .collect();
    assert_eq!(paragraphs, vec![Some(0), Some(1)], "两段 /P 各带 MCID");

    assert_eq!(
        doc.artifact_attribution_violations(page)
            .expect("diagnose writer produced page"),
        vec![],
        "标记内容已通过 /MCID 挂上结构树，归属诊断应为空"
    );

    assert_eq!(
        doc.extract_page_text(page).as_deref(),
        Some("First line\nSecond line"),
        "标记内容段内的文本照常提取"
    );
}

#[test]
fn builder_tag_suspect_roundtrip_keeps_document_conforming() {
    use pdf_engine::core::builder::{
        Font, PageSize, PdfBuilder, StructureElement, StructureMarkInfo, StructureTree,
    };
    use pdf_engine::core::types::{PdfVersion, Value};

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = builder.page(PageSize::A4);
    builder.begin_structure_content(page, "P", 0);
    builder.text(page, &Font::helvetica(12.0), 72.0, 700.0, "First line");
    builder.end_structure_content(page);
    // 页序无法判定的一段内容：14.8.2.3.1 的 tag suspect。
    builder.begin_tag_suspect(page);
    builder.text(page, &Font::helvetica(12.0), 72.0, 680.0, "Second line");
    builder.end_tag_suspect(page);
    builder.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document")
                .with_element(StructureElement::new("P").with_marked_content(page, 0)),
        ),
    );
    builder.set_mark_info(
        StructureMarkInfo::new()
            .with_marked(true)
            .with_suspects(true),
    );

    let mut buf = Vec::new();
    builder
        .build(&mut buf)
        .expect("build page with structure content and tag suspect");
    let raw = String::from_utf8_lossy(&buf).into_owned();
    for entry in [
        "/P << /MCID 0 >> BDC",
        "/TagSuspect << /TagSuspect /Ordering >> BDC",
        "/StructParents 0",
        "/MarkInfo <<",
        "/Suspects true",
    ] {
        assert!(raw.contains(entry), "缺少 {entry}: {raw}");
    }
    let doc = parse_buf(&buf);

    let info = doc
        .mark_info()
        .expect("mark info")
        .expect("/MarkInfo present");
    assert!(info.marked, "文档应声明符合标记内容约定");
    assert!(
        info.suspects,
        "含 tag suspects 的文档必须声明 /Suspects true"
    );

    let sections = doc.page_marked_content(page).expect("marked content");
    let suspects: Vec<Option<&str>> = sections
        .iter()
        .filter(|section| section.tag == "TagSuspect")
        .map(|section| {
            section
                .properties
                .as_ref()
                .and_then(|dict| dict.get("TagSuspect"))
                .and_then(Value::as_name)
        })
        .collect();
    assert_eq!(suspects, vec![Some("Ordering")], "唯一一段 tag suspect");

    assert_eq!(
        doc.artifact_attribution_violations(page)
            .expect("diagnose writer produced page"),
        vec![],
        "写端产出的 tag suspect 应满足 14.8.2.3.1 的两条 shall 与真实内容豁免"
    );

    assert_eq!(
        doc.extract_page_text(page).as_deref(),
        Some("First line\nSecond line"),
        "tag suspect 段内的文本照常提取"
    );
}

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
