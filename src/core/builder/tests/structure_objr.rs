//! 结构元素 OBJR 内容项（ISO 32000-1 14.7.4.3 Table 325、14.7.4.4 与
//! Table 326 `/StructParent`）的写端测试：`/K` 对象引用字典（`/Type`
//! `/OBJR`、间接 `/Pg`、间接 `/Obj`）、父树单元引用条目与页面数组条目
//! 的区分、父树键先页面后 OBJR 的连续分配、注解字典 `/StructParent`
//! 追加，以及句柄越界、重复声称与所在页越界校验的零字节输出契约。

use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::core::reader::structure::{ParentTreeEntry, StructureKid as ReaderKid};
use crate::core::types::{PdfVersion, Value};

/// 写出后重新解析：OBJR 子项只在 `build` 的导航后处理中物化。
fn build_and_parse(b: &mut PdfBuilder) -> Document {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let mut out = Vec::new();
    b.build(&mut out).expect("build");
    let base = std::env::current_exe()
        .ok()
        .and_then(|p| p.ancestors().nth(3).map(|t| t.to_path_buf()))
        .filter(|p| p.file_name() == Some(std::ffi::OsStr::new("target")))
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join("test-tmp");
    std::fs::create_dir_all(&dir).expect("create test-tmp");
    let path = dir.join(format!(
        "pdf-engine-structure-objr-{}-{}.pdf",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&path, &out).expect("write temp pdf");
    let file = std::fs::File::open(&path).expect("open temp pdf");
    let mut parser = crate::core::reader::Parser::new(file).expect("parser");
    let doc = parser.parse_document().expect("parse");
    let _ = std::fs::remove_file(&path);
    doc
}

fn assert_invalid(b: &mut PdfBuilder, expected: &str) {
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("build should fail");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(error.to_string().contains(expected), "{error}");
    assert!(out.is_empty(), "失败时不应写出任何字节");
}

/// 一条 Text 注解：OBJR 引用的目标对象。
fn marker_annotation() -> PdfAnnotation {
    PdfAnnotation::new(AnnotationKind::Text, [72.0, 700.0, 92.0, 720.0]).with_contents("Note")
}

/// Document → Figure（OBJR 单子项）的最小结构树。
fn objr_tree(annotation: usize) -> StructureTree {
    StructureTree::new().with_element(
        StructureElement::new("Document")
            .with_element(StructureElement::new("Figure").with_object_reference(annotation)),
    )
}

/// 按对象号升序收集全部 `/Type /Page` 字典的间接引用。
fn page_references(doc: &Document) -> Vec<(i64, i64)> {
    (1..=64)
        .filter_map(|num| {
            let object = doc.get_object(num).ok()??;
            let dict = object.value.as_dict()?;
            (dict.get("Type").and_then(Value::as_name) == Some("Page"))
                .then_some((object.num, object.generation))
        })
        .collect()
}

/// 取解析后文档的 StructureTree。
fn tree_of(doc: &Document) -> crate::core::reader::structure::StructureTree {
    doc.structure_tree()
        .expect("structure tree")
        .expect("structure tree present")
}

#[test]
fn object_reference_writes_objr_dict_and_struct_parent() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let annotation = b.add_annotation(page, marker_annotation());
    b.set_structure_tree(objr_tree(annotation));

    let doc = build_and_parse(&mut b);
    let tree = tree_of(&doc);
    let annotation_ref = doc.page_annotations(0).expect("annotations")[0]
        .reference
        .expect("annotation is an indirect object");
    let figure = tree
        .elements()
        .iter()
        .find(|element| element.structure_type == "Figure")
        .expect("Figure element");
    match &figure.kids[..] {
        [ReaderKid::ObjectReference(objr)] => {
            assert_eq!(
                objr.object, annotation_ref,
                "/Obj 引用注解对象（Table 325）"
            );
            assert_eq!(
                objr.dict.get("Type"),
                Some(&Value::Name("OBJR".into())),
                "{:?}",
                objr.dict
            );
            let page_ref = objr.page.expect("/Pg 必须是间接引用");
            let page_dict = doc
                .get_object(page_ref.0)
                .expect("page lookup")
                .expect("page exists")
                .value
                .as_dict()
                .expect("page dict")
                .clone();
            assert_eq!(page_dict.get("Type"), Some(&Value::Name("Page".into())));
        }
        other => panic!("Figure 应只有一个 OBJR 子项，实际 {other:?}"),
    }
    let annotation_dict = &doc.page_annotations(0).expect("annotations")[0].dict;
    assert_eq!(
        annotation_dict.get("StructParent"),
        Some(&Value::Integer(0)),
        "无页面键时 OBJR 父树键为 0（Table 326）"
    );
    assert_eq!(tree.root().parent_tree_next_key, Some(1));
}

#[test]
fn object_reference_parent_tree_holds_single_reference_not_array() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let annotation = b.add_annotation(page, marker_annotation());
    b.set_structure_tree(objr_tree(annotation));

    let doc = build_and_parse(&mut b);
    let tree = tree_of(&doc);
    let figure_index = tree
        .elements()
        .iter()
        .position(|element| element.structure_type == "Figure")
        .expect("Figure element");
    // 14.7.4.4：对象引用内容项的父树条目是裸间接引用，不是数组。
    match tree.parent_tree_entry(0) {
        Some(ParentTreeEntry::Object(index)) => {
            assert_eq!(*index, figure_index);
            assert_eq!(tree.object_parent(0), Some(figure_index));
        }
        other => panic!("父树键 0 应为单元条目，实际 {other:?}"),
    }
    assert!(tree.mcid_parent(0, 0).is_none(), "单元条目无 MCID 下标");
    assert_eq!(tree.root().parent_tree_next_key, Some(1));
}

#[test]
fn object_reference_key_follows_page_keys_and_advances_next_key() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let first = b.page(PageSize::A4);
    let second = b.page(PageSize::A4);
    b.begin_structure_content(first, "P", 0);
    b.end_structure_content(first);
    b.begin_structure_content(second, "P", 0);
    b.end_structure_content(second);
    let annotation = b.add_annotation(second, marker_annotation());
    b.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document")
                .with_element(StructureElement::new("P").with_marked_content(first, 0))
                .with_element(StructureElement::new("P").with_marked_content(second, 0))
                .with_element(StructureElement::new("Figure").with_object_reference(annotation)),
        ),
    );

    let doc = build_and_parse(&mut b);
    let tree = tree_of(&doc);
    // 页面键 0、1 之后，OBJR 键接为 2；下一键 3。
    assert_eq!(
        tree.root().parent_tree_next_key,
        Some(3),
        "父树键 0..2 连续分配"
    );
    for page in [first, second] {
        let parents = doc
            .page_struct_parents(page)
            .expect("page parents")
            .expect("/StructParents");
        assert_eq!(parents.key, page as i64, "第 {page} 页键为 {page}");
    }
    let annotation_dict = &doc.page_annotations(second).expect("annotations")[0].dict;
    assert_eq!(
        annotation_dict.get("StructParent"),
        Some(&Value::Integer(2)),
        "OBJR 键接在页面键之后"
    );
    assert!(matches!(
        tree.parent_tree_entry(2),
        Some(ParentTreeEntry::Object(_))
    ));
}

#[test]
fn object_reference_mixes_with_mcid_kids() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    b.begin_structure_content(page, "P", 0);
    b.end_structure_content(page);
    let annotation = b.add_annotation(page, marker_annotation());
    b.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document").with_element(
                StructureElement::new("P")
                    .with_marked_content(page, 0)
                    .with_object_reference(annotation),
            ),
        ),
    );

    let doc = build_and_parse(&mut b);
    let tree = tree_of(&doc);
    let p_index = tree
        .elements()
        .iter()
        .position(|element| element.structure_type == "P")
        .expect("P element");
    match &tree.elements()[p_index].kids[..] {
        [
            ReaderKid::MarkedContent(mcr),
            ReaderKid::ObjectReference(objr),
        ] => {
            assert_eq!(mcr.mcid, 0);
            assert_eq!(mcr.page, objr.page, "同页的 MCR 与 OBJR `/Pg` 一致");
            let annotation_ref = doc.page_annotations(0).expect("annotations")[0]
                .reference
                .expect("indirect annotation");
            assert_eq!(objr.object, annotation_ref);
        }
        other => panic!("P 应为 MCR + OBJR 两个子项，实际 {other:?}"),
    }
    // 页面数组键 0 与 OBJR 单元键 1 并存。
    assert!(matches!(
        tree.parent_tree_entry(0),
        Some(ParentTreeEntry::Page(_))
    ));
    assert_eq!(tree.mcid_parent(0, 0), Some(p_index));
    assert!(matches!(
        tree.parent_tree_entry(1),
        Some(ParentTreeEntry::Object(_))
    ));
    assert_eq!(tree.object_parent(1), Some(p_index));
    assert_eq!(tree.mcid_parent(1, 0), None);
    let parents = doc
        .page_struct_parents(page)
        .expect("page parents")
        .expect("/StructParents");
    assert_eq!(parents.key, 0);
    let annotation_dict = &doc.page_annotations(0).expect("annotations")[0].dict;
    assert_eq!(
        annotation_dict.get("StructParent"),
        Some(&Value::Integer(1))
    );
    assert_eq!(tree.root().parent_tree_next_key, Some(2));
}

#[test]
fn object_reference_uses_annotation_page_for_pg() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let first = b.page(PageSize::A4);
    let second = b.page(PageSize::A4);
    // 注解绑定在第 2 页（0 基索引 1），结构元素不携带任何页信息。
    let annotation = b.add_annotation(second, marker_annotation());
    b.set_structure_tree(objr_tree(annotation));

    let doc = build_and_parse(&mut b);
    let tree = tree_of(&doc);
    let pages = page_references(&doc);
    assert_eq!(pages.len(), 2, "两页文档");
    let figure = tree
        .elements()
        .iter()
        .find(|element| element.structure_type == "Figure")
        .expect("Figure element");
    match &figure.kids[..] {
        [ReaderKid::ObjectReference(objr)] => {
            assert_eq!(
                objr.page,
                Some(pages[1]),
                "/Pg 取注解所在页（第 2 页），不是第 1 页 {first} 或整页顺序"
            );
            assert_ne!(objr.page, Some(pages[0]));
        }
        other => panic!("Figure 应只有一个 OBJR 子项，实际 {other:?}"),
    }
}

#[test]
fn object_reference_handle_out_of_range_is_rejected() {
    // 未注册任何注解：句柄 0 越界。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    b.page(PageSize::A4);
    b.set_structure_tree(objr_tree(0));
    assert_invalid(
        &mut b,
        "structure OBJR annotation handle 0 is out of range (count 0)",
    );

    // 注册一条注解：句柄 3 越界。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    b.add_annotation(page, marker_annotation());
    b.set_structure_tree(objr_tree(3));
    assert_invalid(
        &mut b,
        "structure OBJR annotation handle 3 is out of range (count 1)",
    );
}

#[test]
fn object_reference_duplicate_annotation_is_rejected() {
    // 两个兄弟元素声称同一注解。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let annotation = b.add_annotation(page, marker_annotation());
    b.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document")
                .with_element(StructureElement::new("Figure").with_object_reference(annotation))
                .with_element(StructureElement::new("Note").with_object_reference(annotation)),
        ),
    );
    assert_invalid(
        &mut b,
        "structure OBJR annotation handle 0 is claimed by more than one structure element",
    );

    // 同一元素的两个 `/K` 子项声称同一注解同样拒绝。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let annotation = b.add_annotation(page, marker_annotation());
    b.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document")
                .with_object_reference(annotation)
                .with_object_reference(annotation),
        ),
    );
    assert_invalid(
        &mut b,
        "structure OBJR annotation handle 0 is claimed by more than one structure element",
    );
}

#[test]
fn failed_object_reference_validation_writes_no_bytes() {
    // 页面上有字体与文本对象：OBJR 校验失败也必须整档不落一个字节。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    b.text(page, &Font::helvetica(12.0), 72.0, 700.0, "Body");
    b.add_annotation(page, marker_annotation());
    b.set_structure_tree(objr_tree(7));
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("越界句柄应拒绝构建");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(out.is_empty(), "失败时不应写出任何字节: {} 字节", out.len());

    // 重复声称同样零字节。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    b.text(page, &Font::helvetica(12.0), 72.0, 700.0, "Body");
    let annotation = b.add_annotation(page, marker_annotation());
    b.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document")
                .with_element(StructureElement::new("Figure").with_object_reference(annotation))
                .with_element(StructureElement::new("Note").with_object_reference(annotation)),
        ),
    );
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("重复声称应拒绝构建");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(out.is_empty(), "失败时不应写出任何字节: {} 字节", out.len());
}
