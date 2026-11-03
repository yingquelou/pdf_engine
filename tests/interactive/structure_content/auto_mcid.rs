//! 14.7.4.4 NOTE 自动 MCID 编号与 14.7.4.2 整数 MCID 简写的往返。

use super::*;

/// 自动编号通道往返（14.7.4.4 NOTE：标识符应尽可能小）：两页用
/// `begin_auto_structure_content` 取回自动分配的 MCID 写进结构树 MCR，
/// 页 1 混用一次显式 MCID 0 后自动分配跳过取 1。解析后每页父树数组
/// `0..n-1` 全部 Some 无 `null` 空洞、与结构树 MCR 逐项对齐，
/// `/ParentTreeNextKey` 严格大于全部在用键，文本提取不受标记影响。
#[test]
fn builder_auto_structure_content_roundtrip_keeps_parent_tree_dense_and_next_key_ahead() {
    use pdf_engine::core::builder::{
        Font, PageSize, PdfBuilder, StructureElement, StructureMarkInfo, StructureTree,
    };
    use pdf_engine::core::types::PdfVersion;

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    let first = builder.page(PageSize::A4);
    let second = builder.page(PageSize::A4);
    let a0 = builder
        .begin_auto_structure_content(first, "P")
        .expect("first page auto MCID");
    builder.text(first, &Font::helvetica(12.0), 72.0, 700.0, "First line");
    builder.end_structure_content(first);
    let a1 = builder
        .begin_auto_structure_content(first, "Span")
        .expect("second auto MCID on first page");
    builder.text(first, &Font::helvetica(12.0), 72.0, 680.0, "Second line");
    builder.end_structure_content(first);
    // 显式占用 0 后，自动编号取「显式 ∪ 已分配」之外的最小整数 1。
    builder.begin_structure_content(second, "P", 0);
    builder.text(second, &Font::helvetica(12.0), 72.0, 700.0, "Explicit line");
    builder.end_structure_content(second);
    let b1 = builder
        .begin_auto_structure_content(second, "Span")
        .expect("auto MCID skips the explicit 0");
    builder.text(second, &Font::helvetica(12.0), 72.0, 680.0, "Auto line");
    builder.end_structure_content(second);
    assert_eq!(
        (a0, a1, b1),
        (0, 1, 1),
        "自动编号按页从 0 稠密递增且跳过显式值"
    );

    builder.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document")
                .with_element(StructureElement::new("P").with_marked_content(first, a0))
                .with_element(StructureElement::new("Span").with_marked_content(first, a1))
                .with_element(StructureElement::new("P").with_marked_content(second, 0))
                .with_element(StructureElement::new("Span").with_marked_content(second, b1)),
        ),
    );
    builder.set_mark_info(StructureMarkInfo::new().with_marked(true));

    let mut buf = Vec::new();
    builder
        .build(&mut buf)
        .expect("build pages with auto-numbered structure content");
    let raw = String::from_utf8_lossy(&buf).into_owned();
    for entry in [
        "/P << /MCID 0 >> BDC",
        "/Span << /MCID 1 >> BDC",
        "/StructParents 0",
        "/StructParents 1",
        "/ParentTreeNextKey 2",
    ] {
        assert!(raw.contains(entry), "缺少 {entry}: {raw}");
    }
    let doc = parse_buf(&buf);

    let tree = doc
        .structure_tree()
        .expect("structure tree")
        .expect("structure tree present");
    // arena 前序：0 = Document、1 = P、2 = Span（页 0）；3 = P、4 = Span（页 1）。
    // 自动编号使两页父树数组 0..n-1 全部命中，无任何 null 空洞。
    let first_parents = doc
        .page_struct_parents(first)
        .expect("first page parents")
        .expect("/StructParents");
    assert_eq!(first_parents.key, 0, "页序首个带标记内容的页面得键 0");
    assert_eq!(first_parents.parents, vec![Some(1), Some(2)]);
    assert!(
        first_parents.parents.iter().all(|slot| slot.is_some()),
        "自动编号页面数组不应有 null 空洞: {:?}",
        first_parents.parents
    );
    let second_parents = doc
        .page_struct_parents(second)
        .expect("second page parents")
        .expect("/StructParents");
    assert_eq!(second_parents.key, 1, "第二页得键 1");
    assert_eq!(second_parents.parents, vec![Some(3), Some(4)]);
    assert!(
        second_parents.parents.iter().all(|slot| slot.is_some()),
        "显式 0 + 自动 1 混用页同样稠密: {:?}",
        second_parents.parents
    );
    assert_eq!(tree.mcid_parent(0, 0), Some(1));
    assert_eq!(tree.mcid_parent(0, 1), Some(2));
    assert_eq!(tree.mcid_parent(1, 0), Some(3));
    assert_eq!(tree.mcid_parent(1, 1), Some(4));
    assert_eq!(tree.mcid_parent(0, 2), None, "稠密区间之外无条目");

    // shall：NextKey 严格大于全部在用父树键，且等于「新条目取当前键再自增」
    // 后的下一个可用值。
    let next_key = tree
        .root()
        .parent_tree_next_key
        .expect("/ParentTreeNextKey present");
    assert_eq!(next_key, 2);
    for key in tree.root().parent_tree.keys() {
        assert!(
            *key < next_key,
            "在用父树键 {key} 必须小于 /ParentTreeNextKey {next_key}"
        );
    }

    assert_eq!(
        doc.extract_page_text(first).as_deref(),
        Some("First line\nSecond line"),
        "自动编号标记段内的文本照常提取"
    );
    assert_eq!(
        doc.extract_page_text(second).as_deref(),
        Some("Explicit line\nAuto line"),
        "混用页文本照常提取"
    );
}

/// 第六条往返覆盖 14.7.4.2 的整数 MCID 简写：两页各标记一个 MCID 0 的
/// 真实文本，结构元素以 `/Pg` + 裸整数子项声称同一标记内容。写出零个
/// `/Type /MCR` 字典、回读为 `StructureKid::Mcid`，元素 `/Pg` 与页面
/// `/StructParents` 父树数组对齐，14.8.2.2.1 归属诊断在简写形态下仍为空，
/// 两页文本提取不受影响。
#[test]
fn builder_inline_mcid_shorthand_roundtrip_links_element_page_and_parent_tree() {
    use pdf_engine::core::builder::{Font, PageSize, PdfBuilder, StructureElement, StructureTree};
    use pdf_engine::core::reader::structure::StructureKid;
    use pdf_engine::core::types::PdfVersion;

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    let first = builder.page(PageSize::A4);
    let second = builder.page(PageSize::A4);
    builder.begin_structure_content(first, "P", 0);
    builder.text(first, &Font::helvetica(12.0), 72.0, 700.0, "Inline first");
    builder.end_structure_content(first);
    builder.begin_structure_content(second, "P", 0);
    builder.text(second, &Font::helvetica(12.0), 72.0, 700.0, "Inline second");
    builder.end_structure_content(second);
    builder.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document")
                .with_element(
                    StructureElement::new("P")
                        .with_page(first)
                        .with_inline_marked_content(first, 0),
                )
                .with_element(
                    StructureElement::new("Span")
                        .with_page(second)
                        .with_inline_marked_content(second, 0),
                ),
        ),
    );

    let mut buf = Vec::new();
    builder
        .build(&mut buf)
        .expect("build element /Pg with integer MCID kids");
    let raw = String::from_utf8_lossy(&buf).into_owned();
    assert!(
        raw.contains("/P << /MCID 0 >> BDC"),
        "页上仍是带 /MCID 属性表的标记内容: {raw}"
    );
    assert!(
        !raw.contains("/Type /MCR"),
        "整数简写不应写出任何 MCR 字典: {raw}"
    );
    assert!(raw.contains("/StructParents 0"), "{raw}");
    assert!(raw.contains("/StructParents 1"), "{raw}");
    let doc = parse_buf(&buf);

    let tree = doc
        .structure_tree()
        .expect("structure tree")
        .expect("structure tree present");
    for (index, structure_type) in [(1, "P"), (2, "Span")] {
        let element = &tree.elements()[index];
        assert_eq!(element.structure_type, structure_type);
        assert!(
            matches!(element.kids[..], [StructureKid::Mcid(0)]),
            "元素 {index} 应只有一个整数简写子项，实际 {:?}",
            element.kids
        );
        let page_ref = element.page.expect("元素 /Pg 已写出并可回读");
        assert!(
            doc.get_object(page_ref.0)
                .expect("page lookup")
                .expect("page exists")
                .value
                .as_dict()
                .is_some_and(
                    |dict| dict.get("Type").and_then(|value| value.as_name()) == Some("Page")
                ),
            "元素 /Pg 必须指向页对象"
        );
        assert_eq!(tree.mcid_parent(index as i64 - 1, 0), Some(index));
    }
    for page in [first, second] {
        let parents = doc
            .page_struct_parents(page)
            .expect("page parents")
            .expect("/StructParents");
        assert_eq!(parents.key, page as i64);
        assert_eq!(parents.parents, vec![Some(page as usize + 1)]);
    }
    assert_eq!(
        doc.artifact_attribution_violations(first)
            .expect("diagnostics"),
        Vec::new(),
        "整数简写形态下 14.8.2.2.1 与 14.8.2.3.1 诊断应为空"
    );
    assert_eq!(
        doc.extract_page_text(first).as_deref(),
        Some("Inline first"),
        "简写标记段内的文本照常提取"
    );
    assert_eq!(
        doc.extract_page_text(second).as_deref(),
        Some("Inline second")
    );
}
