//! 流形态 MCR（ISO 32000-1 14.7.4.2 Table 324 `/Stm` `/StmOwn`）的
//! 写端产物 → 解析 → 读端父树定位往返（14.7.4.4、Table 326）。
//!
//! 写端两条通道各出一个序列：一个 `/Stm` 直指页资源 `/Fm1` 的表单流（父树键
//! 写在**该流字典**上），另一个带 `/StmOwn` 指向以同一表单流为 `/AP /N` 常态
//! 外观的 Text 注解（父树键写在**注解字典**上，外观流自身不带键）。读端
//! [`Document::marked_content_parent`] 按规范顺序取回容器、`/StructParents`
//! 键与父元素，并与 `/ParentTree` 数组、页面层既有 `page_struct_parents`
//! 逐条对齐；[`Document::stream_mcr_diagnostic`] 在写端产物上应为空，
//! 证明读端诊断与写端 `validate_stream_mcr_targets` 的口径一致。

use super::*;
use pdf_engine::core::builder::{
    AnnotationKind, Font, FormXObject, PageSize, PdfAnnotation, PdfBuilder, StructureElement,
    StructureKid, StructureMarkInfo, StructureTree,
};
use pdf_engine::core::content::{ContentStream, MarkedContentProperties};
use pdf_engine::core::reader::structure::{
    MarkedContentContainer, ParentTreeEntry, StructureKid as ReaderKid,
    StructureTree as ReaderStructureTree,
};
use pdf_engine::core::types::PdfVersion;

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

/// 对象的字典（直接字典或流字典）：容器 `/StructParents` 的落点。
fn object_dict(doc: &pdf_engine::core::types::Document, num: i64) -> Dict {
    let object = doc
        .get_object(num)
        .expect("object lookup")
        .expect("object exists");
    match &object.value {
        Value::Dict(dict) => dict.clone(),
        Value::Stream(dict, _) => dict.clone(),
        other => panic!("对象 {num} 既不是字典也不是流：{}", other.type_name()),
    }
}

fn element_of(tree: &ReaderStructureTree, structure_type: &str) -> usize {
    tree.elements()
        .iter()
        .position(|item| item.structure_type == structure_type)
        .unwrap_or_else(|| panic!("结构树应有 {structure_type} 元素"))
}

#[test]
fn builder_stream_marked_content_roundtrip_resolves_parents_through_container_keys() {
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
    for entry in ["/Type /MCR", "/Stm ", "/StmOwn ", "/ParentTreeNextKey 3"] {
        assert!(raw.contains(entry), "缺少 {entry}");
    }
    let doc = parse_buf(&buf);
    let tree = doc
        .structure_tree()
        .expect("structure tree")
        .expect("structure tree present");

    // arena 前序：0 = Document、1 = P（页面层）、2 = Figure（无所有者）、3 = Sect（/StmOwn）。
    let page_element = element_of(&tree, "P");
    let figure = element_of(&tree, "Figure");
    let sect = element_of(&tree, "Sect");
    assert_eq!(
        (page_element, figure, sect),
        (1, 2, 3),
        "写端产物应保持前序 arena"
    );

    // 1) 无所有者：容器是 `/Stm` 表单流自身，键在该流字典上。
    let parent = doc
        .marked_content_parent(figure, 0)
        .expect("figure mcr resolves")
        .expect("kid 是 MCR");
    assert_eq!(parent.level, MarkedContentContainer::Stream);
    assert_eq!(parent.mcid, 0, "Table 324 的 /MCID Required");
    let container = parent.container.expect("容器可定位");
    assert_eq!(
        container,
        parent.stream.expect("/Stm 是间接引用"),
        "无 /StmOwn 时容器即 /Stm 流"
    );
    assert_eq!(
        object_dict(&doc, container.0)
            .get("Subtype")
            .and_then(Value::as_name),
        Some("Form"),
        "/Stm 指向页资源的表单流"
    );
    let key = parent.parents_key.expect("容器带 /StructParents");
    assert_eq!(
        object_dict(&doc, container.0).get("StructParents"),
        Some(&Value::Integer(key)),
        "14.7.4.4：父树键取自序列所在的内容流"
    );
    assert_eq!(
        object_dict(&doc, container.0).get("StructParent"),
        None,
        "Table 326：容器不同时作内容项"
    );
    assert_eq!(parent.parent, Some(figure), "MCID 索引父树数组得到 Figure");
    assert_eq!(tree.mcid_parent(key, 0), Some(figure));
    match tree.parent_tree_entry(key) {
        Some(ParentTreeEntry::Page(slots)) => assert_eq!(*slots, vec![Some(figure)]),
        other => panic!("键 {key} 应是 MCID 数组条目，实际 {other:?}"),
    }
    assert!(
        parent.diagnostics.is_empty(),
        "写端产物不应命中读端诊断：{:?}",
        parent.diagnostics
    );

    // 2) 页面层序列继续按页上的键定位（既有行为不回归）。
    let page_parent = doc
        .marked_content_parent(page_element, 0)
        .expect("page mcr resolves")
        .expect("kid 是 MCR");
    assert_eq!(page_parent.level, MarkedContentContainer::Page);
    assert_eq!(page_parent.stream, None, "无 /Stm 时回落页面");
    let parents = doc
        .page_struct_parents(page)
        .expect("page parents")
        .expect("page declares /StructParents");
    assert_eq!(parents.key, 0, "页面键仍是 0，容器键接在其后");
    assert_eq!(parents.parents, vec![Some(page_element)]);
    assert_eq!(page_parent.parents_key, Some(0));
    assert_eq!(page_parent.parent, Some(page_element));
    assert!(
        page_parent.diagnostics.is_empty(),
        "{:?}",
        page_parent.diagnostics
    );

    // 3) 带 `/StmOwn`：容器是所有者注解，键在注解字典，外观流自身不带键。
    let owner_parent = doc
        .marked_content_parent(sect, 0)
        .expect("sect mcr resolves")
        .expect("kid 是 MCR");
    assert_eq!(owner_parent.level, MarkedContentContainer::Owner);
    let owner = owner_parent.container.expect("所有者容器");
    let annotations = doc.page_annotations(page).expect("page annotations");
    assert_eq!(
        owner,
        annotations[0].reference.expect("annotation is indirect"),
        "/StmOwn 指向页上的注解对象"
    );
    let owner_dict = object_dict(&doc, owner.0);
    let owner_key = owner_parent
        .parents_key
        .expect("所有者字典带 /StructParents");
    assert_eq!(
        owner_dict.get("StructParents"),
        Some(&Value::Integer(owner_key)),
        "写端把容器键标在 /StmOwn 所有者字典上"
    );
    assert_eq!(owner_dict.get("StructParent"), None, "Table 326 互斥");
    let appearance = owner_dict
        .get("AP")
        .and_then(Value::as_dict)
        .expect("/AP")
        .get("N")
        .and_then(Value::as_ref)
        .expect("/AP /N 是单条流引用");
    assert_eq!(
        owner_parent.stream,
        Some(appearance),
        "所有者核对：/AP /N 正是 /Stm"
    );
    assert_eq!(
        object_dict(&doc, appearance.0).get("StructParents"),
        None,
        "所有者给定时键不在外观流上"
    );
    assert_eq!(owner_parent.parent, Some(sect), "MCID 0 索引到 Sect");
    assert_eq!(
        owner_parent.page, page_parent.page,
        "/Pg 回落注解绑定页：与页面层序列同页"
    );
    assert!(
        owner_parent.diagnostics.is_empty(),
        "{:?}",
        owner_parent.diagnostics
    );
    assert!(
        key > 0 && owner_key > key,
        "容器键按容器对象号升序接在页面键 0 之后：{key} / {owner_key}"
    );

    // 4) 全树扫描在写端产物上为空：读端诊断与写端校验同口径。
    assert_eq!(
        doc.stream_mcr_diagnostic().expect("sweep runs"),
        vec![],
        "写端产物不应命中任何流形态/Table 326 发现"
    );
    for index in [figure, sect] {
        assert!(
            matches!(
                tree.elements()[index].kids[..],
                [ReaderKid::MarkedContent(_)]
            ),
            "元素 {index} 应只有一个 MCR 子项：{:?}",
            tree.elements()[index].kids
        );
    }
    assert_eq!(tree.root().parent_tree_next_key, Some(3));
}
