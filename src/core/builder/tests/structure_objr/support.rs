//! OBJR 写端测试的共享夹具：构建后重解析、零字节断言、最小结构树与三类
//! OBJR 目标（注解 / Form XObject / 图像 XObject）的构造，以及解析侧字典、
//! 父树与资源引用的取用辅助。

use std::sync::atomic::{AtomicUsize, Ordering};

use super::super::*;
use crate::core::builder::navigation;
use crate::core::builder::navigation::structure::StructureTargets;
use crate::core::content::ContentStream;
use crate::core::reader::structure::StructureKid as ReaderKid;
use crate::core::types::Value;

/// 写出后重新解析：OBJR 子项只在 `build` 的导航后处理中物化。
pub(super) fn build_and_parse(b: &mut PdfBuilder) -> Document {
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

pub(super) fn assert_invalid(b: &mut PdfBuilder, expected: &str) {
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("build should fail");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(error.to_string().contains(expected), "{error}");
    assert!(out.is_empty(), "失败时不应写出任何字节");
}

/// 一条 Text 注解：OBJR 引用的目标对象。
pub(super) fn marker_annotation() -> PdfAnnotation {
    PdfAnnotation::new(AnnotationKind::Text, [72.0, 700.0, 92.0, 720.0]).with_contents("Note")
}

/// Document → Figure（OBJR 单子项）的最小结构树。
pub(super) fn objr_tree(annotation: usize) -> StructureTree {
    StructureTree::new().with_element(
        StructureElement::new("Document")
            .with_element(StructureElement::new("Figure").with_object_reference(annotation)),
    )
}

/// 按对象号升序收集全部 `/Type /Page` 字典的间接引用。
pub(super) fn page_references(doc: &Document) -> Vec<(i64, i64)> {
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
pub(super) fn tree_of(doc: &Document) -> crate::core::reader::structure::StructureTree {
    doc.structure_tree()
        .expect("structure tree")
        .expect("structure tree present")
}

/// 最小自包含 Form XObject：空内容流，不依赖页资源。
pub(super) fn simple_form() -> FormXObject {
    FormXObject::new([0.0, 0.0, 20.0, 20.0], ContentStream::new())
}

/// 单位矩阵绘制（表单原尺寸落在页面上）。
pub(super) const IDENTITY_AT_72: [f64; 6] = [1.0, 0.0, 0.0, 1.0, 72.0, 700.0];

/// 2×2 DeviceRGB 纯色图像；`fill` 决定像素内容（内容完全相等才去重共享
/// 对象号，见 `assemble/alloc_pools.rs` 的图像池）。
pub(super) fn rgb_image(fill: u8) -> ImageData {
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
        oc: None,
        metadata: None,
        matte: None,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    }
}

/// 页 `page_num` 的资源字典 `/XObject/<name>` 的间接引用。
pub(super) fn page_xobject(doc: &Document, page_num: i64, name: &str) -> (i64, i64) {
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
pub(super) fn object_dict(doc: &Document, num: i64) -> Dict {
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
pub(super) fn element_of(
    tree: &crate::core::reader::structure::StructureTree,
    structure_type: &str,
) -> usize {
    tree.elements()
        .iter()
        .position(|element| element.structure_type == structure_type)
        .unwrap_or_else(|| panic!("结构树缺少 {structure_type} 元素"))
}

/// OBJR 子项（要求恰好一个）：返回 `/Obj` 引用与 `/Pg` 引用。
pub(super) fn only_objr(
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
pub(super) fn apply_structure_error(
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
pub(super) fn assert_message(message: impl AsRef<str>, expected: &str) {
    let message = message.as_ref();
    assert!(message.contains(expected), "实际错误：{message}");
}

/// Document → 单个 OBJR 元素的最小结构树；`element_page` 为元素的 `/Pg`。
pub(super) fn objr_element_tree(
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
