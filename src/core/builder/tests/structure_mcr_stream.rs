//! 流形态 MCR（ISO 32000-1 14.7.4.2 Table 324 `/Stm` `/StmOwn`）的写端
//! 测试：`/K` 写出 `/Type /MCR /Stm <间接引用> /MCID n`（可带 `/StmOwn`
//! 间接引用与显式 `/Pg`，14.7.4.2 EXAMPLE 5 形态）；父树键由序列所在流
//! 的容器字典（`/StmOwn` 给出时为所有者，否则为 `/Stm` 流自身）以
//! `/StructParents` 承载，父树条目是 MCID 零基下标数组（14.7.4.4）；
//! 键序为「页面键 → OBJR 键 → `/Stm` 容器键」连续分配。
//!
//! `/MCID` 在本切片基线（PDF 32000-1:2008）的 Table 324 标注 Required
//! （raw 29480–29481 行），故流形态与页面形态一样必填，不存在「带
//! `/StmOwn` 时缺省 `/MCID`」的可写出形态。拒绝路径：`/Stm` 句柄越界、
//! `/Stm` 解析对象不存在或不是 `/Subtype /Form` 流、`/StmOwn` 句柄越界、
//! 注解绑定页越界或与引用 `/Pg` 冲突、三路都取不到 `/Pg`、`/StmOwn` 的
//! `/AP` 不指向 `/Stm`、同容器 MCID 重复声称、容器与 OBJR 声称互斥
//! （Table 326「not both」shall）与容器字典已带 `/StructParent(s)`。
//! 后两类按本目录既有模式经 `assemble_document` 篡改 `Document` 后直接
//! 调用 `navigation::apply` 触发；零字节契约在公开 API 上由
//! [`assert_invalid`] 覆盖。

use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::core::builder::navigation;
use crate::core::builder::navigation::structure::StructureTargets;
use crate::core::content::{ContentStream, MarkedContentProperties};
use crate::core::reader::structure::StructureKid as ReaderKid;
use crate::core::reader::structure::{MarkedContentReference, ParentTreeEntry};
use crate::core::types::{Dict, Object, PdfVersion, Value};

/// 写出后重新解析：流形态 MCR 只在 `build` 的导航后处理中物化。
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
        "pdf-engine-structure-mcr-stream-{}-{}.pdf",
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

/// 一条 Text 注解：`/StmOwn` 的候选所有者。
fn marker_annotation() -> PdfAnnotation {
    PdfAnnotation::new(AnnotationKind::Text, [72.0, 700.0, 92.0, 720.0]).with_contents("Note")
}

/// 持有一个 BDC/EMC 标记内容序列（`/MCID 0`）的自包含 Form XObject：
/// 内容流里没有可提取文本，序列本身以空标记呈现，形态与规范 EXAMPLE 5
/// 的流侧一致（写端不校验流内是否真有该 MCID，见模块注释的剩余缺口）。
fn marked_form() -> FormXObject {
    let mut content = ContentStream::new();
    content.begin_marked_content_properties(
        "P",
        MarkedContentProperties::Dict(Dict::new().insert("MCID", Value::Integer(0))),
    );
    content.end_marked_content();
    FormXObject::new([0.0, 0.0, 20.0, 20.0], content)
}

/// 单位矩阵绘制（表单原尺寸落在页面上）。
const IDENTITY_AT_72: [f64; 6] = [1.0, 0.0, 0.0, 1.0, 72.0, 700.0];

/// 取解析后文档的 StructureTree。
fn tree_of(doc: &Document) -> crate::core::reader::structure::StructureTree {
    doc.structure_tree()
        .expect("structure tree")
        .expect("structure tree present")
}

/// 前序元素里指定类型首个元素的下标。
fn element_of(tree: &crate::core::reader::structure::StructureTree, structure_type: &str) -> usize {
    tree.elements()
        .iter()
        .position(|element| element.structure_type == structure_type)
        .unwrap_or_else(|| panic!("结构树缺少 {structure_type} 元素"))
}

/// Figure 的流形态 MCR 子项（要求恰好一个）。
fn only_stream_mcr(tree: &crate::core::reader::structure::StructureTree) -> MarkedContentReference {
    let index = element_of(tree, "Figure");
    match &tree.elements()[index].kids[..] {
        [ReaderKid::MarkedContent(mcr)] => mcr.clone(),
        other => panic!("Figure 应只有一个 MCR 子项，实际 {other:?}"),
    }
}

/// 按对象号取解析后的原始字典（流对象取流字典）。
fn object_dict(doc: &Document, num: i64) -> Dict {
    let value = &doc
        .get_object(num)
        .expect("object lookup")
        .expect("object exists")
        .value;
    match value {
        Value::Dict(dict) => dict.clone(),
        Value::Stream(dict, _) => dict.clone(),
        other => panic!("对象 {num} 不是字典/流（{}）", other.type_name()),
    }
}

/// Document → Figure（流形态 MCR 单子项）的结构树。
fn stream_tree(kid: StructureKid) -> StructureTree {
    StructureTree::new().with_element(
        StructureElement::new("Document")
            .with_element(StructureElement::new("Figure").with_kid(kid)),
    )
}

/// 用自定义流形态解析表直接跑一次导航阶段，返回错误文本（篡改分支）。
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
    .expect_err("流形态 MCR 目标校验应拒绝导航阶段");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    error.to_string()
}

/// 文档级流形态目标解析表（真实句柄表；`form_pages` 只有 OBJR 表单目标
/// 分支消费，流形态 MCR 只读 `form_nums`）。
fn real_targets(b: &PdfBuilder) -> StructureTargets<'_> {
    StructureTargets {
        annotation_nums: &b.annotation_nums,
        annotation_pages: &b.annotation_pages,
        form_nums: &b.form_nums,
        form_pages: &[],
        image_nums: &b.image_nums,
    }
}

/// 断言错误文本包含期望片段。
fn assert_message(message: impl AsRef<str>, expected: &str) {
    let message = message.as_ref();
    assert!(message.contains(expected), "实际错误：{message}");
}

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

// —— 拒绝路径（公开 API，零字节）——

#[test]
fn stream_mcr_form_handle_out_of_range_is_rejected() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    b.set_structure_tree(stream_tree(StructureKid::StreamMarkedContent {
        stream: 7,
        owner: None,
        page: Some(page),
        mcid: 0,
    }));
    assert_invalid(
        &mut b,
        "structure MCR /Stm form XObject handle 7 is out of range",
    );
}

#[test]
fn stream_mcr_without_any_page_source_is_rejected() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    b.page(PageSize::A4);
    let form = b.form_xobject(&marked_form());
    b.set_structure_tree(stream_tree(StructureKid::StreamMarkedContent {
        stream: form,
        owner: None,
        page: None,
        mcid: 0,
    }));
    assert_invalid(
        &mut b,
        "needs a page: the reference gives no /Pg, there is no /StmOwn page to fall back to, and the element has no /Pg",
    );
}

#[test]
fn stream_mcr_page_out_of_range_is_rejected() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let form = b.form_xobject(&marked_form());
    b.set_structure_tree(stream_tree(StructureKid::StreamMarkedContent {
        stream: form,
        owner: None,
        page: Some(page + 5),
        mcid: 0,
    }));
    assert_invalid(&mut b, "structure MCR /Stm page 5 is out of range");
}

#[test]
fn stream_mcr_negative_mcid_is_rejected() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let form = b.form_xobject(&marked_form());
    b.draw_form(page, form, IDENTITY_AT_72);
    b.set_structure_tree(stream_tree(StructureKid::StreamMarkedContent {
        stream: form,
        owner: None,
        page: Some(page),
        mcid: -1,
    }));
    assert_invalid(&mut b, "structure MCR MCID must be non-negative, got -1");
}

#[test]
fn stream_mcr_stmown_handle_out_of_range_is_rejected() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let form = b.form_xobject(&marked_form());
    b.draw_form(page, form, IDENTITY_AT_72);
    b.set_structure_tree(stream_tree(StructureKid::StreamMarkedContent {
        stream: form,
        owner: Some(3),
        page: Some(page),
        mcid: 0,
    }));
    assert_invalid(
        &mut b,
        "structure MCR /StmOwn annotation handle 3 is out of range",
    );
}

#[test]
fn stream_mcr_stmown_page_conflict_is_rejected() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let first = b.page(PageSize::A4);
    let second = b.page(PageSize::A4);
    let form = b.form_xobject(&marked_form());
    b.draw_form(first, form, IDENTITY_AT_72);
    let annotation = b.add_annotation(first, marker_annotation().with_appearance(form));
    // 引用谎报渲染页：注解绑定页是 first，/Pg 却给 second。
    b.set_structure_tree(stream_tree(StructureKid::StreamMarkedContent {
        stream: form,
        owner: Some(annotation),
        page: Some(second),
        mcid: 0,
    }));
    assert_invalid(
        &mut b,
        "is attached to page 0 but the reference names page 1",
    );
}

#[test]
fn stream_mcr_stmown_without_matching_appearance_is_rejected() {
    for with_other_form in [false, true] {
        let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
        let page = b.page(PageSize::A4);
        let form = b.form_xobject(&marked_form());
        b.draw_form(page, form, IDENTITY_AT_72);
        let annotation = if with_other_form {
            // /AP 指向另一个表单：不是 /Stm 流的所有者。
            let other = b.form_xobject(&marked_form());
            b.add_annotation(page, marker_annotation().with_appearance(other))
        } else {
            // 无 /AP：Table 324 的「owning the stream」无从成立。
            b.add_annotation(page, marker_annotation())
        };
        b.set_structure_tree(stream_tree(StructureKid::StreamMarkedContent {
            stream: form,
            owner: Some(annotation),
            page: Some(page),
            mcid: 0,
        }));
        assert_invalid(&mut b, "does not own the /Stm object");
    }
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

#[test]
fn stream_mcr_duplicate_mcid_in_container_is_rejected() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let form = b.form_xobject(&marked_form());
    b.draw_form(page, form, IDENTITY_AT_72);
    let kid = || StructureKid::StreamMarkedContent {
        stream: form,
        owner: None,
        page: Some(page),
        mcid: 0,
    };
    b.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document")
                .with_element(StructureElement::new("Figure").with_kid(kid()))
                .with_element(StructureElement::new("Sect").with_kid(kid())),
        ),
    );
    assert_invalid(
        &mut b,
        "MCID 0 is claimed by more than one structure element",
    );
}

#[test]
fn stream_mcr_container_conflicts_with_objr_claim_is_rejected() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let form = b.form_xobject(&marked_form());
    b.draw_form(page, form, IDENTITY_AT_72);
    // 同一表单既被 OBJR 声称（写出 /StructParent）又作 /Stm 容器
    // （写出 /StructParents）：Table 326「not both」拒绝。OBJR 子项先被
    // 展开，故报错来自流形态分支。
    b.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document")
                .with_element(
                    StructureElement::new("Widget").with_form_xobject_reference(form, Some(page)),
                )
                .with_element(StructureElement::new("Figure").with_kid(
                    StructureKid::StreamMarkedContent {
                        stream: form,
                        owner: None,
                        page: Some(page),
                        mcid: 0,
                    },
                )),
        ),
    );
    assert_invalid(&mut b, "is also claimed as an OBJR content item");
}

#[test]
fn failed_stream_mcr_validation_writes_no_bytes() {
    // 零字节契约：`/StmOwn` 与 `/Stm` 不一致的失败路径不 flush 任何字节。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let form = b.form_xobject(&marked_form());
    b.draw_form(page, form, IDENTITY_AT_72);
    let annotation = b.add_annotation(page, marker_annotation());
    b.set_structure_tree(stream_tree(StructureKid::StreamMarkedContent {
        stream: form,
        owner: Some(annotation),
        page: Some(page),
        mcid: 0,
    }));
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("mismatched /StmOwn should fail");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(out.is_empty(), "失败时不应写出任何字节: {} 字节", out.len());
}

// —— 篡改分支（公开 API 不可达：目标对象不存在 / 类别错配 / 字典已带条目）——

#[test]
fn stream_mcr_rejects_missing_stm_object() {
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
    let mut doc = b.assemble_document().expect("assemble");

    // 解析表把句柄指到文档里不存在的对象号。
    let message = apply_structure_error(
        &b,
        &mut doc,
        StructureTargets {
            form_nums: &[9_024],
            ..Default::default()
        },
    );
    assert_message(message, "structure MCR /Stm object 9024 0 is missing");
}

#[test]
fn stream_mcr_rejects_stm_that_is_not_a_form_stream() {
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
    let mut doc = b.assemble_document().expect("assemble");
    let form_num = b.form_nums[form];

    // 篡改一：解析目标变成普通字典（不是流）。
    doc.set_object(Object::new(form_num, 0, Value::Dict(Dict::new())));
    let message = apply_structure_error(&b, &mut doc, real_targets(&b));
    assert_message(
        message,
        &format!("structure MCR /Stm object {form_num} 0 is not a content stream"),
    );

    // 篡改二：流还在，但 /Subtype 不再是 Form。
    let mut doc = b.assemble_document().expect("assemble");
    let dict = object_dict(&doc, form_num).insert("Subtype", Value::Name("Image".into()));
    doc.set_object(Object::new(
        form_num,
        0,
        Value::Stream(dict, StreamData::Owned(Vec::new())),
    ));
    let message = apply_structure_error(&b, &mut doc, real_targets(&b));
    assert_message(message, "is not a /Subtype /Form content stream");
}

#[test]
fn stream_mcr_rejects_container_with_existing_struct_entries() {
    for entry in ["StructParent", "StructParents"] {
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
        let mut doc = b.assemble_document().expect("assemble");
        let form_num = b.form_nums[form];
        let dict = object_dict(&doc, form_num).insert(entry, Value::Integer(9));
        doc.set_object(Object::new(
            form_num,
            0,
            Value::Stream(dict, StreamData::Owned(Vec::new())),
        ));

        let message = apply_structure_error(&b, &mut doc, real_targets(&b));
        assert_message(
            &message,
            &format!("/Stm object {form_num} 0 already has /{entry}"),
        );
        assert_message(
            message,
            "at most one of /StructParent and /StructParents may appear in an object (Table 326)",
        );
    }
}

#[test]
fn stream_mcr_rejects_stmown_pointing_at_stream_object() {
    // /StmOwn 必须是普通注解字典：句柄被解析到流对象（篡改表）时拒绝。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    let form = b.form_xobject(&marked_form());
    b.draw_form(page, form, IDENTITY_AT_72);
    b.set_structure_tree(stream_tree(StructureKid::StreamMarkedContent {
        stream: form,
        owner: Some(0),
        page: Some(page),
        mcid: 0,
    }));
    let mut doc = b.assemble_document().expect("assemble");
    let form_num = b.form_nums[form];
    let message = apply_structure_error(
        &b,
        &mut doc,
        StructureTargets {
            annotation_nums: &[form_num],
            annotation_pages: &[0],
            form_nums: &[form_num],
            ..Default::default()
        },
    );
    assert_message(&message, "/StmOwn object");
    assert_message(message, "is not an annotation dictionary");
}
