//! 14.7.4.2 Table 324 流形态 MCR（/Stm /StmOwn）的写出 → 解析 → 回读往返。

use super::*;

/// 流形态 MCR（14.7.4.2 Table 324 `/Stm` `/StmOwn`）的整文档往返：
/// 无所有者时 `/Stm` 与页资源 `/Fm1` 引用一致、父树键写在该表单流
/// 字典上；带 `/StmOwn` 时 `/Stm` 与注解 `/AP /N` 外观流引用一致、
/// 键写在注解字典上、`/Pg` 回落注解绑定页。两容器键按容器对象号
/// 升序接在页面键 0 之后，父树数组条目指向声称元素，页面 MCID
/// 归属与文本提取不受流形态序列影响。
#[test]
fn builder_stream_marked_content_roundtrip_links_stm_stmown_and_container_keys() {
    use pdf_engine::core::builder::{
        AnnotationKind, Font, FormXObject, PageSize, PdfAnnotation, PdfBuilder, StructureElement,
        StructureKid, StructureMarkInfo, StructureTree,
    };
    use pdf_engine::core::content::{ContentStream, MarkedContentProperties};
    use pdf_engine::core::reader::structure::{ParentTreeEntry, StructureKid as ReaderKid};
    use pdf_engine::core::types::{Dict, PdfVersion, Value};

    /// 内含一段 `/P << /MCID 0 >> BDC … EMC` 标记内容序列的表单。
    fn marked_content_form() -> FormXObject {
        let mut content = ContentStream::new();
        content.begin_marked_content_properties(
            "P",
            MarkedContentProperties::Dict(Dict::new().insert("MCID", Value::Integer(0))),
        );
        content.end_marked_content();
        FormXObject::new([0.0, 0.0, 20.0, 20.0], content)
    }

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = builder.page(PageSize::A4);
    builder.begin_structure_content(page, "P", 0);
    builder.text(page, &Font::helvetica(12.0), 72.0, 700.0, "Tagged line");
    builder.end_structure_content(page);
    let inline_form = builder.form_xobject(&marked_content_form());
    builder.draw_form(page, inline_form, [1.0, 0.0, 0.0, 1.0, 72.0, 560.0]);
    let owner_form = builder.form_xobject(&marked_content_form());
    let annotation = builder.add_annotation(
        page,
        PdfAnnotation::new(AnnotationKind::Text, [72.0, 660.0, 92.0, 680.0])
            .with_contents("Stream owner")
            .with_appearance(owner_form),
    );
    builder.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document")
                .with_element(StructureElement::new("P").with_marked_content(page, 0))
                .with_element(StructureElement::new("Figure").with_kid(
                    StructureKid::StreamMarkedContent {
                        stream: inline_form,
                        owner: None,
                        page: Some(page),
                        mcid: 0,
                    },
                ))
                .with_element(StructureElement::new("Sect").with_kid(
                    StructureKid::StreamMarkedContent {
                        stream: owner_form,
                        owner: Some(annotation),
                        page: None,
                        mcid: 0,
                    },
                )),
        ),
    );
    builder.set_mark_info(StructureMarkInfo::new().with_marked(true));

    let mut buf = Vec::new();
    builder
        .build(&mut buf)
        .expect("build document with stream-form MCR content items");
    let raw = String::from_utf8_lossy(&buf).into_owned();
    for entry in [
        "/Type /MCR",
        "/Stm ",
        "/StmOwn ",
        "/P << /MCID 0 >> BDC",
        "/StructParents 1",
        "/StructParents 2",
        "/ParentTreeNextKey 3",
    ] {
        assert!(raw.contains(entry), "缺少 {entry}: {raw}");
    }
    let doc = parse_buf(&buf);

    let tree = doc
        .structure_tree()
        .expect("structure tree")
        .expect("structure tree present");
    // arena 前序：0 = Document、1 = P、2 = Figure（无所有者）、3 = Sect（/StmOwn）。
    let stream_mcr = |index: usize| match &tree.elements()[index].kids[..] {
        [ReaderKid::MarkedContent(mcr)] => mcr.clone(),
        other => panic!("元素 {index} 应只有一个 MCR 子项，实际 {other:?}"),
    };
    let figure = stream_mcr(2);
    let sect = stream_mcr(3);
    for mcr in [&figure, &sect] {
        assert_eq!(mcr.dict.get("Type"), Some(&Value::Name("MCR".into())));
        assert_eq!(mcr.mcid, 0, "Table 324 的 MCID Required");
    }
    assert_eq!(figure.stream_own, None, "Figure 不写 /StmOwn");

    let page_num = figure.page.expect("Figure 写出显式 /Pg").0;
    assert_eq!(
        objr_object_dict(&doc, page_num)
            .get("Type")
            .and_then(Value::as_name),
        Some("Page"),
        "/Pg 指向页对象"
    );
    assert_eq!(
        sect.page.expect("/StmOwn 形态回落注解绑定页并写出 /Pg").0,
        page_num
    );

    // 无所有者：/Stm 与页资源 /Fm1 引用一致，键写在该流字典上。
    let stm_form = objr_page_xobject_ref(&doc, page_num, "Fm1");
    assert_eq!(
        figure.stream,
        Some(stm_form),
        "Figure 的 /Stm 即页资源表单流"
    );
    let figure_dict = objr_object_dict(&doc, stm_form.0);
    assert_eq!(
        figure_dict.get("Subtype").and_then(Value::as_name),
        Some("Form"),
        "/Stm 指向 /Subtype /Form 流（Table 324）"
    );

    // 带所有者：/StmOwn 指向注解，/Stm 即注解 /AP /N 外观流，键在注解字典。
    let annotations = doc.page_annotations(page).expect("page annotations");
    assert_eq!(annotations.len(), 1, "页上唯一注解");
    let annotation_ref = annotations[0].reference.expect("indirect annotation");
    assert_eq!(
        sect.stream_own,
        Some(annotation_ref),
        "/StmOwn 指向注解对象"
    );
    let owner_dict = objr_object_dict(&doc, annotation_ref.0);
    let appearance_ref = owner_dict
        .get("AP")
        .and_then(Value::as_dict)
        .expect("/AP")
        .get("N")
        .and_then(Value::as_ref)
        .expect("/AP /N 是单条流引用");
    assert_eq!(
        sect.stream,
        Some(appearance_ref),
        "所有者注解的 /AP /N 正是 /Stm（Table 324）"
    );
    assert_eq!(
        objr_object_dict(&doc, appearance_ref.0).get("StructParents"),
        None,
        "所有者给定时键不在流上"
    );

    // 容器键：页面键 0 之后按容器对象号升序取 1、2（14.7.4.4 键序）。
    let key_of = |dict: &Dict| {
        dict.get("StructParents")
            .and_then(Value::as_integer)
            .expect("容器字典带 /StructParents")
    };
    let figure_key = key_of(&figure_dict);
    let sect_key = key_of(&owner_dict);
    let mut keys = [figure_key, sect_key];
    keys.sort_unstable();
    assert_eq!(keys, [1, 2], "容器键接在页面键 0 之后连续分配");
    for (key, element) in [(figure_key, 2), (sect_key, 3)] {
        match tree.parent_tree_entry(key) {
            Some(ParentTreeEntry::Page(slots)) => {
                assert_eq!(*slots, vec![Some(element)], "键 {key} 的 MCID 0 槽位")
            }
            other => panic!("父树键 {key} 应是数组条目，实际 {other:?}"),
        }
        assert_eq!(tree.mcid_parent(key, 0), Some(element));
        assert_eq!(tree.mcid_parent(key, 1), None, "键 {key} 数组只覆盖 MCID 0");
    }

    let parents = doc
        .page_struct_parents(page)
        .expect("page parents")
        .expect("/StructParents");
    assert_eq!(parents.key, 0, "唯一带标记内容的页面仍是父树键 0");
    assert_eq!(parents.parents, vec![Some(1)]);
    assert_eq!(
        tree.mcid_parent(0, 0),
        Some(1),
        "页面 MCID 归属不受流形态序列影响"
    );
    assert_eq!(
        tree.root().parent_tree_next_key,
        Some(3),
        "/ParentTreeNextKey 覆盖页面键与两个容器键"
    );

    assert_eq!(
        doc.extract_page_text(page).as_deref(),
        Some("Tagged line"),
        "流形态 MCR 与外观表单不影响页文本提取"
    );
}
