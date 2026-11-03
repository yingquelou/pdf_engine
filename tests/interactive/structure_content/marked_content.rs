//! 14.7.4.2、14.7.4.4 MCID 通道：结构标记内容写出 → 解析 → 回读往返。

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
