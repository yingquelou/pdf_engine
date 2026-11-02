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
//!
//! 按主题目录化拆分（与 `tests/graphics_state/`、`tests/acro_form/`、
//! `tests/structure/` 的目录化形态一致）：
//! - `write_positive`：写出/正向形态（`/Stm` 引用 + `/MCID` + 容器
//!   `/StructParents`、元素 `/Pg` 继承省略显式条目、`/StmOwn` 外观与回落页、
//!   页面/OBJR/容器键序、多状态外观接受）；
//! - `target_validation`：句柄/目标校验拒绝（`/Stm` 与 `/StmOwn` 句柄越界、
//!   `/StmOwn` 的 `/AP` 不指向 `/Stm`、零字节契约，以及篡改分支的对象不存在
//!   与类别错配）；
//! - `page_source`：页面来源冲突拒绝（三路皆取不到 `/Pg`、页越界、注解绑定
//!   页与引用 `/Pg` 冲突）；
//! - `mcid_claims`：MCID 与声称互斥拒绝（负 `/MCID`、同容器重复声称、容器
//!   与 OBJR 声称互斥、容器字典已带 `/StructParent(s)`、`/StmOwn` 指向流对象）。
//!
//! 模块 doc、helper、夹具与常量保留在本文件，子文件经 `use super::*;` 复用。

mod mcid_claims;
mod page_source;
mod target_validation;
mod write_positive;

use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::core::builder::navigation;
use crate::core::builder::navigation::structure::StructureTargets;
use crate::core::content::{ContentStream, MarkedContentProperties};
use crate::core::reader::structure::MarkedContentReference;
use crate::core::reader::structure::StructureKid as ReaderKid;
use crate::core::types::{Dict, Value};

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
