//! 结构元素 OBJR 内容项（ISO 32000-1 14.7.4.3 Table 325、14.7.4.4 与
//! Table 326 `/StructParent`）的写端测试：`/K` 对象引用字典（`/Type`
//! `/OBJR`、间接 `/Pg`、间接 `/Obj`）、父树单元引用条目与页面数组条目
//! 的区分、父树键先页面后 OBJR（按目标对象号升序）的连续分配、目标字典
//! `/StructParent` 追加，以及句柄/页/资源序号越界、Form 未在该页绘制、
//! 两路都取不到 `/Pg`、按解析对象号判重的重复声称与所在页越界校验的
//! 零字节输出契约。
//!
//! `/Obj` 的目标（14.7.4.3）覆盖三档：注解字典、文档级 Form XObject 与页
//! 资源里的图像 XObject。Table 326 的 `/StructParent` ↔ `/StructParents`
//! 互斥与目标形态（注解不得是流、XObject 须带对应 `/Subtype`、目标对象须
//! 存在）在结构对象号分配之前拒绝；这些分支中「目标字典已带
//! `/StructParent`/`/StructParents`」与类别错配在公开 API 上不可达（写端
//! 不给目标字典预置这两个条目），故按本目录既有模式直接调用
//! `navigation::apply` 并传入自定义 `StructureTargets` 触发。

use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::core::builder::navigation;
use crate::core::builder::navigation::structure::StructureTargets;
use crate::core::content::ContentStream;
use crate::core::reader::structure::{ParentTreeEntry, StructureKid as ReaderKid};
use crate::core::types::{Object, PdfVersion, Value};

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

/// 最小自包含 Form XObject：空内容流，不依赖页资源。
fn simple_form() -> FormXObject {
    FormXObject::new([0.0, 0.0, 20.0, 20.0], ContentStream::new())
}

/// 单位矩阵绘制（表单原尺寸落在页面上）。
const IDENTITY_AT_72: [f64; 6] = [1.0, 0.0, 0.0, 1.0, 72.0, 700.0];

/// 2×2 DeviceRGB 纯色图像；`fill` 决定像素内容（内容完全相等才去重共享
/// 对象号，见 `assemble/alloc_pools.rs` 的图像池）。
fn rgb_image(fill: u8) -> ImageData {
    ImageData {
        width: 2,
        height: 2,
        color_space: ColorSpace::DeviceRGB,
        bits: 8,
        data: vec![fill; 12],
        decode: None,
        interpolate: false,
        intent: None,
        alternates: Vec::new(),
        matte: None,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    }
}

/// 页 `page_num` 的资源字典 `/XObject/<name>` 的间接引用。
fn page_xobject(doc: &Document, page_num: i64, name: &str) -> (i64, i64) {
    let page = doc
        .get_object(page_num)
        .expect("page lookup")
        .expect("page exists")
        .value
        .as_dict()
        .expect("page dict")
        .clone();
    let resources = page
        .get("Resources")
        .and_then(Value::as_dict)
        .expect("page /Resources")
        .clone();
    let xobjects = resources
        .get("XObject")
        .and_then(Value::as_dict)
        .expect("page /Resources /XObject")
        .clone();
    xobjects
        .get(name)
        .and_then(Value::as_ref)
        .unwrap_or_else(|| panic!("资源 {name} 应是间接引用，实际 {xobjects:?}"))
}

/// 对象的字典（直接字典或流字典）：OBJR 目标写 `/StructParent` 的位置。
fn object_dict(doc: &Document, num: i64) -> Dict {
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

/// 结构树里首个该类型元素的 arena 下标。
fn element_of(tree: &crate::core::reader::structure::StructureTree, structure_type: &str) -> usize {
    tree.elements()
        .iter()
        .position(|element| element.structure_type == structure_type)
        .unwrap_or_else(|| panic!("结构树缺少 {structure_type} 元素"))
}

/// OBJR 子项（要求恰好一个）：返回 `/Obj` 引用与 `/Pg` 引用。
fn only_objr(
    tree: &crate::core::reader::structure::StructureTree,
    structure_type: &str,
) -> ((i64, i64), Option<(i64, i64)>) {
    let index = element_of(tree, structure_type);
    match &tree.elements()[index].kids[..] {
        [ReaderKid::ObjectReference(objr)] => (objr.object, objr.page),
        other => panic!("{structure_type} 应只有一个 OBJR 子项，实际 {other:?}"),
    }
}

/// 用自定义 OBJR 解析表直接跑一次导航阶段，返回错误文本。
///
/// Table 326 的 `/StructParent` ↔ `/StructParents` 互斥与目标形态/类别错配
/// 在公开 API 上不可达：写端不会给目标字典预置这两个条目，也不会让句柄解析
/// 到别的类别。故沿用 `action_ocg.rs`、`embedded_file.rs` 等的既有模式——
/// 先 `assemble_document` 取得真实对象号，按需篡改 `Document` 与解析表，再
/// 调用 `navigation::apply`。这些校验同样先于结构对象号分配，故失败零字节
/// （零字节契约在公开 API 上由 [`assert_invalid`] 覆盖）。
fn apply_structure_error(
    b: &PdfBuilder,
    doc: &mut Document,
    targets: StructureTargets<'_>,
) -> String {
    let error = navigation::apply(
        doc,
        b.version,
        b.pages.len(),
        &b.navigation,
        crate::core::builder::types::ActionRefs {
            annotation_nums: &b.annotation_nums,
            ocg_nums: &b.ocg_nums,
            sound_nums: &b.sound_nums,
            form_nums: &b.form_nums,
            embedded_nums: &b.embedded_nums,
            rendition_nums: &b.rendition_nums,
            rendition_cursor: Some(&b.rendition_cursor),
        },
        targets,
    )
    .expect_err("OBJR 目标校验应拒绝导航阶段");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    error.to_string()
}

/// 断言错误文本包含期望片段。
fn assert_message(message: impl AsRef<str>, expected: &str) {
    let message = message.as_ref();
    assert!(message.contains(expected), "实际错误：{message}");
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

// —— Form XObject 目标（14.7.4.3「an entire PDF object, such as an XObject
//    or an annotation」+ Table 325 的 `/Obj` 必填间接引用）——

/// Document → 单个 OBJR 元素的最小结构树；`element_page` 为元素的 `/Pg`。
fn objr_element_tree(
    structure_type: &str,
    element_page: Option<usize>,
    object: StructureObjectReference,
) -> StructureTree {
    let mut element = StructureElement::new(structure_type);
    if let Some(page) = element_page {
        element = element.with_page(page);
    }
    StructureTree::new().with_element(
        StructureElement::new("Document").with_element(element.with_object_reference(object)),
    )
}

#[test]
fn objr_accepts_form_xobject_target() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let form = b.form_xobject(&simple_form());
    b.draw_form(page, form, IDENTITY_AT_72);
    b.set_structure_tree(objr_element_tree(
        "Figure",
        None,
        StructureObjectReference::form_xobject_on_page(form, page),
    ));

    let doc = build_and_parse(&mut b);
    let tree = tree_of(&doc);
    let pages = page_references(&doc);
    let form_ref = page_xobject(&doc, pages[0].0, "Fm1");
    let (object, objr_page) = only_objr(&tree, "Figure");
    assert_eq!(object, form_ref, "/Obj 须指向绘制页资源里的表单流");
    assert_eq!(objr_page, Some(pages[0]), "/Pg 是表单渲染页（Table 325）");
    let dict = object_dict(&doc, form_ref.0);
    assert_eq!(
        dict.get("Subtype"),
        Some(&Value::Name("Form".into())),
        "目标是表单流"
    );
    assert_eq!(
        dict.get("StructParent"),
        Some(&Value::Integer(0)),
        "整对象内容项在流字典追加 /StructParent（Table 326 + 14.7.4.4 Example 1）"
    );
    assert_eq!(tree.object_parent(0), Some(element_of(&tree, "Figure")));
    assert_eq!(tree.root().parent_tree_next_key, Some(1));
}

#[test]
fn objr_form_target_pg_falls_back_to_element_pg() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let form = b.form_xobject(&simple_form());
    b.draw_form(page, form, IDENTITY_AT_72);
    // 引用不带页：Table 325「it shall be used if the structure element has
    // no such entry」的相反面——缺页时取元素 `/Pg`。
    b.set_structure_tree(objr_element_tree(
        "Figure",
        Some(page),
        StructureObjectReference::form_xobject(form),
    ));

    let doc = build_and_parse(&mut b);
    let tree = tree_of(&doc);
    let pages = page_references(&doc);
    let (object, objr_page) = only_objr(&tree, "Figure");
    assert_eq!(objr_page, Some(pages[0]), "/Pg 沿用元素 /Pg");
    assert_eq!(
        tree.elements()[element_of(&tree, "Figure")].page,
        Some(pages[0]),
        "元素 /Pg 自身也写出"
    );
    assert_eq!(
        object_dict(&doc, object.0).get("StructParent"),
        Some(&Value::Integer(0))
    );
}

#[test]
fn objr_form_target_pg_overrides_element_pg() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let first = b.page(PageSize::A4);
    let second = b.page(PageSize::A4);
    let form = b.form_xobject(&simple_form());
    b.draw_form(first, form, IDENTITY_AT_72);
    b.draw_form(second, form, IDENTITY_AT_72);
    // 元素 /Pg 指第一页，引用指第二页：Table 325「This entry overrides any
    // Pg entry in the structure element containing the object reference」。
    b.set_structure_tree(objr_element_tree(
        "Figure",
        Some(first),
        StructureObjectReference::form_xobject_on_page(form, second),
    ));

    let doc = build_and_parse(&mut b);
    let tree = tree_of(&doc);
    let pages = page_references(&doc);
    assert_eq!(pages.len(), 2, "两页文档");
    let (object, objr_page) = only_objr(&tree, "Figure");
    assert_eq!(objr_page, Some(pages[1]), "引用的 /Pg 覆盖元素 /Pg");
    assert_eq!(
        tree.elements()[element_of(&tree, "Figure")].page,
        Some(pages[0]),
        "元素 /Pg 保持第一页，不被改写"
    );
    assert_eq!(
        object,
        page_xobject(&doc, pages[0].0, "Fm1"),
        "同一表单在两页各登记一次资源名，对象号仍是同一个"
    );
    assert_eq!(
        object_dict(&doc, object.0).get("StructParent"),
        Some(&Value::Integer(0))
    );
}

#[test]
fn objr_form_target_page_must_draw_the_form() {
    // 显式 /Pg 谎报：表单只画在第 2 页，引用却声称第 1 页。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let first = b.page(PageSize::A4);
    let second = b.page(PageSize::A4);
    let form = b.form_xobject(&simple_form());
    b.draw_form(second, form, IDENTITY_AT_72);
    b.set_structure_tree(objr_element_tree(
        "Figure",
        None,
        StructureObjectReference::form_xobject_on_page(form, first),
    ));
    assert_invalid(
        &mut b,
        "structure OBJR form XObject handle 0 is not drawn on page 0",
    );

    // 沿用元素 /Pg 的那一路同样核对渲染页（两路共用一次检查）。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let first = b.page(PageSize::A4);
    let second = b.page(PageSize::A4);
    let form = b.form_xobject(&simple_form());
    b.draw_form(second, form, IDENTITY_AT_72);
    b.set_structure_tree(objr_element_tree(
        "Figure",
        Some(first),
        StructureObjectReference::form_xobject(form),
    ));
    assert_invalid(
        &mut b,
        "structure OBJR form XObject handle 0 is not drawn on page 0",
    );
}

#[test]
fn objr_form_target_never_drawn_is_rejected() {
    // 注册但从未绘制的表单没有对象号，也没有任何渲染页：先按「未绘制」拒绝，
    // 而不是让句柄解析出对象号 0 后报一个难以理解的「object 0 0 is missing」。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let form = b.form_xobject(&simple_form());
    b.set_structure_tree(objr_element_tree(
        "Figure",
        None,
        StructureObjectReference::form_xobject_on_page(form, page),
    ));
    assert_invalid(
        &mut b,
        "structure OBJR form XObject handle 0 is not drawn on page 0",
    );
}

#[test]
fn objr_form_target_without_any_page_is_rejected() {
    // 引用不给页且元素无 /Pg：Table 325 的 /Pg 无从写出。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let form = b.form_xobject(&simple_form());
    b.draw_form(page, form, IDENTITY_AT_72);
    b.set_structure_tree(objr_element_tree(
        "Figure",
        None,
        StructureObjectReference::form_xobject(form),
    ));
    assert_invalid(
        &mut b,
        "structure OBJR form XObject handle 0 needs a page: the reference gives no /Pg and the element has no /Pg (Table 325)",
    );
}

#[test]
fn objr_form_handle_out_of_range_is_rejected() {
    // 未注册任何表单。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    b.page(PageSize::A4);
    b.set_structure_tree(objr_element_tree(
        "Figure",
        Some(0),
        StructureObjectReference::form_xobject(0),
    ));
    assert_invalid(
        &mut b,
        "structure OBJR form XObject handle 0 is out of range (count 0)",
    );

    // 注册并绘制一个表单：句柄 3 越界。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let form = b.form_xobject(&simple_form());
    b.draw_form(page, form, IDENTITY_AT_72);
    b.set_structure_tree(objr_element_tree(
        "Figure",
        Some(page),
        StructureObjectReference::form_xobject_on_page(3, page),
    ));
    assert_invalid(
        &mut b,
        "structure OBJR form XObject handle 3 is out of range (count 1)",
    );
    // 句柄未使用，错误按声明的类别与句柄报告。
}

#[test]
fn objr_form_target_page_out_of_range_is_rejected() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let form = b.form_xobject(&simple_form());
    b.draw_form(page, form, IDENTITY_AT_72);
    b.set_structure_tree(objr_element_tree(
        "Figure",
        None,
        StructureObjectReference::form_xobject_on_page(0, 5),
    ));
    assert_invalid(
        &mut b,
        "structure OBJR form XObject handle 0 names page 5 out of range (page count 1)",
    );
}

#[test]
fn objr_form_target_claimed_twice_is_rejected() {
    // 同一表单在两页各绘制一次，但写端只出一个对象号：第二次声称按对象号
    // 判重拒绝（14.7.4.3 NOTE 2 要求每次渲染各用一个对象引用，此处无法满足）。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let first = b.page(PageSize::A4);
    let second = b.page(PageSize::A4);
    let form = b.form_xobject(&simple_form());
    b.draw_form(first, form, IDENTITY_AT_72);
    b.draw_form(second, form, IDENTITY_AT_72);
    b.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document")
                .with_element(
                    StructureElement::new("Figure").with_form_xobject_reference(form, Some(first)),
                )
                .with_element(
                    StructureElement::new("Logo").with_form_xobject_reference(form, Some(second)),
                ),
        ),
    );
    assert_invalid(
        &mut b,
        "structure OBJR form XObject handle 0 is claimed by more than one structure element",
    );
}

// —— 图像 XObject 目标（页资源 `/Im<序号+1>`，14.7.4.3）——

#[test]
fn objr_accepts_image_xobject_target() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    b.image(page, 72.0, 700.0, 20.0, 20.0, &rgb_image(0x40));
    b.set_structure_tree(objr_element_tree(
        "Figure",
        None,
        StructureObjectReference::image_xobject(page, 0),
    ));

    let doc = build_and_parse(&mut b);
    let tree = tree_of(&doc);
    let pages = page_references(&doc);
    let image_ref = page_xobject(&doc, pages[0].0, "Im1");
    let (object, objr_page) = only_objr(&tree, "Figure");
    assert_eq!(object, image_ref, "/Obj 指向该页图像资源里的流");
    assert_eq!(objr_page, Some(pages[0]), "/Pg 即图像所在页");
    let dict = object_dict(&doc, image_ref.0);
    assert_eq!(
        dict.get("Subtype"),
        Some(&Value::Name("Image".into())),
        "目标是图像流"
    );
    assert_eq!(
        dict.get("StructParent"),
        Some(&Value::Integer(0)),
        "图像流字典追加 /StructParent（Table 326 第一行）"
    );
    assert_eq!(tree.object_parent(0), Some(element_of(&tree, "Figure")));
    assert_eq!(tree.root().parent_tree_next_key, Some(1));
}

#[test]
fn objr_accepts_two_distinct_images_on_one_page() {
    // 两张不同内容的图像是两个对象号：两个元素各自声称合法，键按对象号升序。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    b.image(page, 72.0, 700.0, 20.0, 20.0, &rgb_image(0x10));
    b.image(page, 120.0, 700.0, 20.0, 20.0, &rgb_image(0x20));
    b.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document")
                .with_element(
                    StructureElement::new("FigureA").with_image_xobject_reference(page, 0),
                )
                .with_element(
                    StructureElement::new("FigureB").with_image_xobject_reference(page, 1),
                ),
        ),
    );

    let doc = build_and_parse(&mut b);
    let tree = tree_of(&doc);
    let pages = page_references(&doc);
    let first = page_xobject(&doc, pages[0].0, "Im1");
    let second = page_xobject(&doc, pages[0].0, "Im2");
    assert_ne!(first.0, second.0, "内容不同 → 两个流对象");
    let (object_a, _) = only_objr(&tree, "FigureA");
    let (object_b, _) = only_objr(&tree, "FigureB");
    assert_eq!(object_a, first);
    assert_eq!(object_b, second);
    // 键按解析后的对象号升序接排。
    let low = first.0.min(second.0);
    let low_key = if object_a.0 == low { 0 } else { 1 };
    assert_eq!(
        object_dict(&doc, first.0).get("StructParent"),
        Some(&Value::Integer(low_key))
    );
    assert_eq!(
        object_dict(&doc, second.0).get("StructParent"),
        Some(&Value::Integer(if low_key == 0 { 1 } else { 0 }))
    );
    assert_eq!(tree.root().parent_tree_next_key, Some(2));
}

#[test]
fn objr_duplicate_claim_is_keyed_by_resolved_object_number() {
    // 同一图像内容在同页放两次：去重池让 `/Im1` 与 `/Im2` 共用一个对象号，
    // 于是两个资源序号指向同一对象——第二次声称即重复声称。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    b.image(page, 72.0, 700.0, 20.0, 20.0, &rgb_image(0x80));
    b.image(page, 120.0, 700.0, 20.0, 20.0, &rgb_image(0x80));
    b.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document")
                .with_element(
                    StructureElement::new("FigureA").with_image_xobject_reference(page, 0),
                )
                .with_element(
                    StructureElement::new("FigureB").with_image_xobject_reference(page, 1),
                ),
        ),
    );
    assert_invalid(
        &mut b,
        "structure OBJR image XObject page 0 index 1 is claimed by more than one structure element",
    );
}

#[test]
fn objr_image_target_page_out_of_range_is_rejected() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    b.image(page, 72.0, 700.0, 20.0, 20.0, &rgb_image(0x40));
    b.set_structure_tree(objr_element_tree(
        "Figure",
        None,
        StructureObjectReference::image_xobject(3, 0),
    ));
    assert_invalid(
        &mut b,
        "structure OBJR image XObject page 3 is out of range (page count 1)",
    );
}

#[test]
fn objr_image_target_index_out_of_range_is_rejected() {
    // 页上有一张图像：序号 1 越界。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    b.image(page, 72.0, 700.0, 20.0, 20.0, &rgb_image(0x40));
    b.set_structure_tree(objr_element_tree(
        "Figure",
        None,
        StructureObjectReference::image_xobject(page, 1),
    ));
    assert_invalid(
        &mut b,
        "structure OBJR image XObject index 1 on page 0 is out of range (count 1)",
    );

    // 页上没有图像：序号 0 即越界。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    b.page(PageSize::A4);
    b.set_structure_tree(objr_element_tree(
        "Figure",
        None,
        StructureObjectReference::image_xobject(0, 0),
    ));
    assert_invalid(
        &mut b,
        "structure OBJR image XObject index 0 on page 0 is out of range (count 0)",
    );
}

#[test]
fn objr_image_and_form_targets_mix_with_annotation_target() {
    // 一页同时有标记内容、图像、表单与注解：父树键先是页面键（按页升序），
    // 后接全部 OBJR 键（按解析后的目标对象号升序）。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    b.begin_structure_content(page, "P", 0);
    b.end_structure_content(page);
    b.image(page, 72.0, 700.0, 20.0, 20.0, &rgb_image(0x10));
    let form = b.form_xobject(&simple_form());
    b.draw_form(page, form, IDENTITY_AT_72);
    let annotation = b.add_annotation(page, marker_annotation());
    b.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document")
                .with_element(StructureElement::new("P").with_marked_content(page, 0))
                .with_element(StructureElement::new("Figure").with_image_xobject_reference(page, 0))
                .with_element(StructureElement::new("Logo").with_object_reference(
                    StructureObjectReference::form_xobject_on_page(form, page),
                ))
                .with_element(StructureElement::new("Note").with_object_reference(annotation)),
        ),
    );

    let doc = build_and_parse(&mut b);
    let tree = tree_of(&doc);
    let pages = page_references(&doc);
    let targets = [
        (
            "Figure",
            page_xobject(&doc, pages[0].0, "Im1").0,
            element_of(&tree, "Figure"),
        ),
        (
            "Logo",
            page_xobject(&doc, pages[0].0, "Fm1").0,
            element_of(&tree, "Logo"),
        ),
        (
            "Note",
            doc.page_annotations(0).expect("annotations")[0]
                .reference
                .expect("indirect annotation")
                .0,
            element_of(&tree, "Note"),
        ),
    ];
    // 页面键 0 之后，OBJR 键按对象号升序 1、2、3。
    let mut ordered = targets.to_vec();
    ordered.sort_by_key(|(_, num, _)| *num);
    for (rank, (_, num, element)) in ordered.iter().enumerate() {
        let key = rank as i64 + 1;
        assert_eq!(
            object_dict(&doc, *num).get("StructParent"),
            Some(&Value::Integer(key)),
            "对象 {num} 的父树键应接在页面键之后按对象号升序排为 {key}"
        );
        match tree.parent_tree_entry(key) {
            Some(ParentTreeEntry::Object(found)) => assert_eq!(
                *found, *element,
                "父树键 {key} 的单元条目应指向声称它的元素"
            ),
            other => panic!("父树键 {key} 应为单元条目，实际 {other:?}"),
        }
    }
    let parents = doc
        .page_struct_parents(page)
        .expect("page parents")
        .expect("/StructParents");
    assert_eq!(parents.key, 0, "页面键仍为 0");
    assert_eq!(tree.root().parent_tree_next_key, Some(4));
}

#[test]
fn failed_objr_xobject_validation_writes_no_bytes() {
    // 页上有文字、图像与表单：XObject 目标的句柄越界也必须整档零字节。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    b.text(page, &Font::helvetica(12.0), 72.0, 700.0, "Body");
    b.image(page, 72.0, 600.0, 20.0, 20.0, &rgb_image(0x40));
    let form = b.form_xobject(&simple_form());
    b.draw_form(page, form, IDENTITY_AT_72);
    b.set_structure_tree(objr_element_tree(
        "Figure",
        None,
        StructureObjectReference::image_xobject(page, 9),
    ));
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("越界的图像资源序号应拒绝构建");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(out.is_empty(), "失败时不应写出任何字节: {} 字节", out.len());

    // 表单渲染页谎报同样零字节。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let first = b.page(PageSize::A4);
    let second = b.page(PageSize::A4);
    b.text(first, &Font::helvetica(12.0), 72.0, 700.0, "Body");
    let form = b.form_xobject(&simple_form());
    b.draw_form(second, form, IDENTITY_AT_72);
    b.add_annotation(first, marker_annotation());
    b.set_structure_tree(objr_element_tree(
        "Figure",
        None,
        StructureObjectReference::form_xobject_on_page(form, first),
    ));
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("未绘制的页应拒绝构建");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(out.is_empty(), "失败时不应写出任何字节: {} 字节", out.len());
}

// —— Table 326 `/StructParent` ↔ `/StructParents` 互斥与目标形态校验 ——
//
// 这些分支在公开 API 上不可达（写端不给目标字典预置这两个条目，也不会让
// 句柄解析到别的类别的对象），故沿用 `action_ocg.rs` 等的既有模式：先
// `assemble_document` 取得真实对象号，再篡改 `Document` 或解析表并直接调用
// `navigation::apply`。校验仍在结构对象号分配之前。

#[test]
fn objr_rejects_missing_target_object() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let annotation = b.add_annotation(page, marker_annotation());
    b.set_structure_tree(objr_tree(annotation));
    let mut doc = b.assemble_document().expect("assemble");

    // 解析表给出文档里不存在的对象号。
    let message = apply_structure_error(
        &b,
        &mut doc,
        StructureTargets {
            annotation_nums: &[9_024],
            annotation_pages: &[0],
            ..Default::default()
        },
    );
    assert_message(
        message,
        "structure OBJR annotation handle 0 object 9024 0 is missing",
    );

    // 句柄在对象号表里存在、但页表短于对象号表（表本身不一致）。
    let message = apply_structure_error(
        &b,
        &mut doc,
        StructureTargets {
            annotation_nums: &[b.annotation_nums[annotation]],
            annotation_pages: &[],
            ..Default::default()
        },
    );
    assert_message(
        message,
        "structure OBJR annotation handle 0 is out of range (count 0)",
    );
}

#[test]
fn objr_rejects_struct_parent_and_struct_parents_on_same_object() {
    // Table 326 shall：「At most one of these two entries shall be present in
    // a given object」——目标字典已带任一条目时不再追加另一个，直接拒绝。
    for entry in ["StructParent", "StructParents"] {
        let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
        let page = b.page(PageSize::A4);
        let annotation = b.add_annotation(page, marker_annotation());
        b.set_structure_tree(objr_tree(annotation));
        let mut doc = b.assemble_document().expect("assemble");

        let annotation_num = b.annotation_nums[annotation];
        let dict = object_dict(&doc, annotation_num).insert(entry, Value::Integer(9));
        doc.set_object(Object::new(annotation_num, 0, Value::Dict(dict)));

        let message = apply_structure_error(
            &b,
            &mut doc,
            StructureTargets {
                annotation_nums: &b.annotation_nums,
                annotation_pages: &b.annotation_pages,
                ..Default::default()
            },
        );
        assert_message(&message, &format!("already has /{entry}"));
        assert_message(
            message,
            "at most one of /StructParent and /StructParents may appear in an object (Table 326)",
        );
    }
}

#[test]
fn objr_rejects_target_that_will_carry_struct_parents() {
    // 目标对象号落在「将写 /StructParents 的页对象」上：该对象是含标记内容
    // 序列的内容流，不得再作为整对象内容项追加 /StructParent。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    b.begin_structure_content(page, "P", 0);
    b.end_structure_content(page);
    b.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document")
                .with_element(StructureElement::new("P").with_marked_content(page, 0))
                .with_element(StructureElement::new("Figure").with_object_reference(0)),
        ),
    );
    let mut doc = b.assemble_document().expect("assemble");
    let pages = page_references(&doc);
    let message = apply_structure_error(
        &b,
        &mut doc,
        StructureTargets {
            annotation_nums: &[pages[0].0],
            annotation_pages: &[0],
            ..Default::default()
        },
    );
    assert_message(
        message,
        "is also a content stream carrying /StructParents; at most one of /StructParent and /StructParents may appear in an object (Table 326)",
    );
}

#[test]
fn objr_rejects_target_that_is_not_a_dictionary() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let annotation = b.add_annotation(page, marker_annotation());
    b.set_structure_tree(objr_tree(annotation));
    let mut doc = b.assemble_document().expect("assemble");

    let annotation_num = b.annotation_nums[annotation];
    doc.set_object(Object::new(annotation_num, 0, Value::Integer(3)));
    let message = apply_structure_error(
        &b,
        &mut doc,
        StructureTargets {
            annotation_nums: &[annotation_num],
            annotation_pages: &[0],
            ..Default::default()
        },
    );
    assert_message(
        message,
        &format!(
            "structure OBJR annotation handle 0 object {annotation_num} 0 is not a dictionary"
        ),
    );
}

#[test]
fn objr_rejects_annotation_target_that_is_a_stream() {
    // 句柄解析到表单流对象，却按注解声称：对象形态与类别必须一致。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let form = b.form_xobject(&simple_form());
    b.draw_form(page, form, IDENTITY_AT_72);
    b.set_structure_tree(objr_tree(0));
    let mut doc = b.assemble_document().expect("assemble");

    let form_num = b.form_nums[form];
    let message = apply_structure_error(
        &b,
        &mut doc,
        StructureTargets {
            annotation_nums: &[form_num],
            annotation_pages: &[0],
            ..Default::default()
        },
    );
    assert_message(
        message,
        &format!(
            "structure OBJR annotation handle 0 object {form_num} 0 is a stream, not an annotation dictionary (Table 325)"
        ),
    );
}

#[test]
fn objr_rejects_xobject_target_with_wrong_subtype() {
    // 表单目标解析到图像流。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    b.image(page, 72.0, 700.0, 20.0, 20.0, &rgb_image(0x40));
    b.set_structure_tree(objr_element_tree(
        "Figure",
        Some(page),
        StructureObjectReference::form_xobject(0),
    ));
    let mut doc = b.assemble_document().expect("assemble");
    let image_num = b.image_nums[page][0];
    let message = apply_structure_error(
        &b,
        &mut doc,
        StructureTargets {
            form_nums: &[image_num],
            form_pages: &[vec![page]],
            ..Default::default()
        },
    );
    assert_message(
        message,
        &format!(
            "structure OBJR form XObject handle 0 object {image_num} 0 is not a /Subtype /Form XObject stream (Table 325)"
        ),
    );

    // 图像目标解析到表单流。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let form = b.form_xobject(&simple_form());
    b.draw_form(page, form, IDENTITY_AT_72);
    b.set_structure_tree(objr_element_tree(
        "Figure",
        None,
        StructureObjectReference::image_xobject(page, 0),
    ));
    let mut doc = b.assemble_document().expect("assemble");
    let form_num = b.form_nums[form];
    let message = apply_structure_error(
        &b,
        &mut doc,
        StructureTargets {
            image_nums: &[vec![form_num]],
            ..Default::default()
        },
    );
    assert_message(
        message,
        &format!(
            "structure OBJR image XObject page 0 index 0 object {form_num} 0 is not a /Subtype /Image XObject stream (Table 325)"
        ),
    );
}
