//! 流形态 MCR 写出与正向形态：`/Type /MCR` + `/Stm` 间接引用 + `/MCID`
//! 与容器 `/StructParents`/父树条目（Table 324、14.7.4.4，EXAMPLE 5 形态）、
//! 元素 `/Pg` 继承时省略显式条目、`/StmOwn` 取注解外观与绑定页回落、
//! 页面键 → OBJR 键 → 容器键的连续键序、多状态 `/AP` 被接受。

use super::*;
use crate::core::reader::structure::ParentTreeEntry;
use crate::core::types::{PdfVersion, Value};

#[test]
fn stream_mcr_writes_stm_ref_mcid_and_container_struct_parents() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let form = b.form_xobject(&marked_form());
    b.draw_form(page, form, IDENTITY_AT_72);
    b.set_structure_tree(stream_tree(StructureKid::StreamMarkedContent {
        stream: form,
        owner: None,
        page: Some(page),
        mcid: 0,
    }));

    let doc = build_and_parse(&mut b);
    let tree = tree_of(&doc);
    let mcr = only_stream_mcr(&tree);
    // `/Type /MCR` + `/Stm` 间接引用 + `/MCID`（Table 324，EXAMPLE 5 形态）。
    assert_eq!(mcr.dict.get("Type"), Some(&Value::Name("MCR".into())));
    assert_eq!(mcr.mcid, 0);
    assert_eq!(mcr.stream_own, None, "无 /StmOwn 时不写出");
    let form_num = mcr.stream.expect("/Stm 是间接引用").0;
    assert_eq!(
        object_dict(&doc, form_num)
            .get("Subtype")
            .and_then(Value::as_name),
        Some("Form"),
        "/Stm 指向表单流对象"
    );
    let page_ref = mcr.page.expect("显式 /Pg 是间接引用");
    assert_eq!(
        object_dict(&doc, page_ref.0)
            .get("Type")
            .and_then(Value::as_name),
        Some("Page"),
        "/Pg 指向页对象"
    );
    // 14.7.4.4：无所有者时序列所在流自身带 /StructParents。
    let form_dict = object_dict(&doc, form_num);
    let key = match form_dict.get("StructParents") {
        Some(Value::Integer(key)) => *key,
        other => panic!("表单流字典应带 /StructParents，实际 {other:?}"),
    };
    assert_eq!(key, 0, "无页面/OBJR 键时容器键为 0");
    let figure_index = element_of(&tree, "Figure");
    match tree.parent_tree_entry(key) {
        Some(ParentTreeEntry::Page(slots)) => {
            assert_eq!(slots.len(), 1);
            assert_eq!(slots[0], Some(figure_index), "MCID 0 槽位指向 Figure");
        }
        other => panic!("父树键 {key} 应为数组条目，实际 {other:?}"),
    }
    assert_eq!(tree.mcid_parent(key, 0), Some(figure_index));
    assert_eq!(tree.root().parent_tree_next_key, Some(1));
}

#[test]
fn stream_mcr_pg_inherits_element_pg_and_omits_explicit_entry() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let form = b.form_xobject(&marked_form());
    b.draw_form(page, form, IDENTITY_AT_72);
    b.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document").with_element(
                StructureElement::new("Figure")
                    .with_page(page)
                    .with_stream_marked_content(form, 0),
            ),
        ),
    );

    let doc = build_and_parse(&mut b);
    let tree = tree_of(&doc);
    let mcr = only_stream_mcr(&tree);
    // 继承元素 `/Pg` 时 MCR 字典不重复写 `/Pg`（Table 324 的覆盖语义）。
    assert_eq!(mcr.dict.get("Pg"), None, "继承元素 /Pg 时省略显式条目");
    assert_eq!(mcr.stream_own, None);
    assert_eq!(mcr.mcid, 0);
    // 元素 `/Pg` 写在元素字典上并指向页对象（Table 323）。
    let figure_index = element_of(&tree, "Figure");
    let element_pg = tree.elements()[figure_index]
        .page
        .expect("元素 /Pg 应解析为页引用");
    assert_eq!(
        object_dict(&doc, element_pg.0)
            .get("Type")
            .and_then(Value::as_name),
        Some("Page"),
        "元素 /Pg 指向页对象"
    );
    // 父树键仍在无所有者时的 `/Stm` 流自身上。
    let form_num = mcr.stream.expect("/Stm 是间接引用").0;
    let key = match object_dict(&doc, form_num).get("StructParents") {
        Some(Value::Integer(key)) => *key,
        other => panic!("表单流字典应带 /StructParents，实际 {other:?}"),
    };
    assert_eq!(key, 0, "无页面/OBJR 键时容器键为 0");
    assert_eq!(tree.mcid_parent(key, 0), Some(figure_index));
    assert_eq!(tree.root().parent_tree_next_key, Some(1));
}

#[test]
fn stream_mcr_with_stmown_uses_annotation_appearance_and_page() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let form = b.form_xobject(&marked_form());
    // 注解以该表单为常态外观：/StmOwn 一致性核对的合法形态。
    let annotation = b.add_annotation(page, marker_annotation().with_appearance(form));
    b.set_structure_tree(stream_tree(StructureKid::StreamMarkedContent {
        stream: form,
        owner: Some(annotation),
        page: None,
        mcid: 0,
    }));

    let doc = build_and_parse(&mut b);
    let tree = tree_of(&doc);
    let mcr = only_stream_mcr(&tree);
    assert_eq!(mcr.dict.get("Type"), Some(&Value::Name("MCR".into())));
    assert_eq!(mcr.mcid, 0, "Table 324 的 MCID Required");
    let reference = doc.page_annotations(page).expect("annotations")[0]
        .reference
        .expect("annotation is indirect");
    assert_eq!(mcr.stream_own, Some(reference), "/StmOwn 指向注解对象");
    // 页三路回落的第二路：引用与元素都没给 /Pg，回落注解绑定页并写出显式条目。
    let Some(pg_value) = mcr.dict.get("Pg") else {
        panic!("回落注解绑定页时应写出显式 /Pg");
    };
    let Value::Ref(pg_num, pg_generation) = pg_value else {
        panic!("/Pg 应为间接引用，实际 {pg_value:?}");
    };
    assert_eq!(mcr.page, Some((*pg_num, *pg_generation)));
    assert_eq!(
        object_dict(&doc, *pg_num)
            .get("Type")
            .and_then(Value::as_name),
        Some("Page"),
        "/Pg 指向页对象"
    );
    // `/Stm` 在 `/StmOwn` 形态下不带父树键；键在所有者（注解字典）上。
    let form_dict = object_dict(&doc, mcr.stream.expect("/Stm").0);
    assert_eq!(
        form_dict.get("StructParents"),
        None,
        "所有者给定时键不在流上"
    );
    let annotation_dict = object_dict(&doc, reference.0);
    let key = match annotation_dict.get("StructParents") {
        Some(Value::Integer(key)) => *key,
        other => panic!("注解字典应带 /StructParents，实际 {other:?}"),
    };
    assert_eq!(key, 0, "无页面/OBJR 键时容器键为 0");
    let figure_index = element_of(&tree, "Figure");
    assert_eq!(tree.mcid_parent(key, 0), Some(figure_index));
    assert_eq!(tree.root().parent_tree_next_key, Some(1));
}

#[test]
fn stream_mcr_parent_keys_follow_page_and_objr_keys() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    b.begin_structure_content(page, "P", 0);
    b.end_structure_content(page);
    let form = b.form_xobject(&marked_form());
    b.draw_form(page, form, IDENTITY_AT_72);
    let annotation = b.add_annotation(page, marker_annotation());
    b.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document")
                .with_element(StructureElement::new("P").with_marked_content(page, 0))
                .with_element(StructureElement::new("Figure").with_object_reference(annotation))
                .with_element(StructureElement::new("Sect").with_kid(
                    StructureKid::StreamMarkedContent {
                        stream: form,
                        owner: None,
                        page: Some(page),
                        mcid: 0,
                    },
                )),
        ),
    );

    let doc = build_and_parse(&mut b);
    let tree = tree_of(&doc);
    // 键序：页面键 0 → OBJR 键 1 → /Stm 容器键 2；NextKey = 3。
    assert_eq!(tree.root().parent_tree_next_key, Some(3));
    let parents = doc
        .page_struct_parents(page)
        .expect("page parents")
        .expect("/StructParents");
    assert_eq!(parents.key, 0);
    let annotation_dict = &doc.page_annotations(page).expect("annotations")[0].dict;
    let objr_key = match annotation_dict.get("StructParent") {
        Some(Value::Integer(key)) => *key,
        other => panic!("OBJR 注解应带 /StructParent，实际 {other:?}"),
    };
    assert_eq!(objr_key, 1);
    let sect_index = element_of(&tree, "Sect");
    let ReaderKid::MarkedContent(mcr) = &tree.elements()[sect_index].kids[0] else {
        panic!("Sect 子项应为 MCR");
    };
    let form_dict = object_dict(&doc, mcr.stream.expect("/Stm").0);
    match form_dict.get("StructParents") {
        Some(Value::Integer(key)) => assert_eq!(*key, 2, "/Stm 容器键接在 OBJR 键之后"),
        other => panic!("表单流字典应带 /StructParents，实际 {other:?}"),
    }
    assert_eq!(tree.mcid_parent(2, 0), Some(sect_index));
}

#[test]
fn stream_mcr_stmown_multistate_appearance_is_accepted() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let form = b.form_xobject(&marked_form());
    // /AP /N 为「状态名 → 流引用」子字典（Table 168 多状态 Widget 形态）。
    let annotation = b.add_annotation(page, marker_annotation().with_appearance_state("On", form));
    b.set_structure_tree(stream_tree(StructureKid::StreamMarkedContent {
        stream: form,
        owner: Some(annotation),
        page: None,
        mcid: 0,
    }));

    let doc = build_and_parse(&mut b);
    let tree = tree_of(&doc);
    let mcr = only_stream_mcr(&tree);
    assert_eq!(
        mcr.stream_own,
        Some(
            doc.page_annotations(page).expect("annotations")[0]
                .reference
                .expect("annotation is indirect")
        )
    );
    assert_eq!(tree.root().parent_tree_next_key, Some(1));
}
