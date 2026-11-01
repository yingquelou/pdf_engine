//! 14.7.4.2 整数 MCID 简写与结构元素 `/Pg`（Table 323）的写端测试：
//! `/K` 里的裸整数内容项、元素 `/Pg` 的间接页引用、简写只在「序列所在页
//! 等于元素 `/Pg`」时合法（缺 `/Pg` 与页不一致均零字节拒绝）、MCR 字典仍
//! 保留自身显式 `/Pg`，以及简写项与 MCR 一样参与页面父树数组与内容流
//! 一致性预检。

use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::core::reader::structure::StructureKid as ReaderKid;
use crate::core::types::{PdfVersion, Value};

/// 写出后重新解析：元素 `/Pg` 与整数简写都要回读验证。
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
        "pdf-engine-structure-inline-mcid-{}-{}.pdf",
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

/// 一条内容流：页 0 标记 MCID 0，结构树以元素 `/Pg` + 整数简写声称它。
fn inline_tree(page: usize, mcid: i64) -> StructureTree {
    StructureTree::new().with_element(
        StructureElement::new("Document").with_element(
            StructureElement::new("P")
                .with_page(page)
                .with_inline_marked_content(page, mcid),
        ),
    )
}

#[test]
fn inline_marked_content_writes_bare_integer_kid_and_element_page() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    b.begin_structure_content(page, "P", 0);
    b.end_structure_content(page);
    b.set_structure_tree(inline_tree(page, 0));

    let doc = build_and_parse(&mut b);
    let tree = doc
        .structure_tree()
        .expect("structure tree")
        .expect("structure tree present");
    let paragraph = tree
        .elements()
        .iter()
        .find(|element| element.structure_type == "P")
        .expect("P element");
    let page_ref = page_references(&doc)[page];
    assert_eq!(
        paragraph.page,
        Some(page_ref),
        "元素 /Pg 是页对象的间接引用（Table 323）"
    );
    assert!(
        matches!(paragraph.kids[..], [ReaderKid::Mcid(0)]),
        "整数简写应回读为 Mcid，实际 {:?}",
        paragraph.kids
    );
    // 14.7.4.4：简写项与 MCR 一样占用页面父树数组的 MCID 槽位。
    let parents = doc
        .page_struct_parents(page)
        .expect("page parents")
        .expect("/StructParents");
    let paragraph_index = tree
        .elements()
        .iter()
        .position(|element| element.structure_type == "P")
        .expect("P element");
    assert_eq!(parents.parents, vec![Some(paragraph_index)]);
    assert_eq!(tree.mcid_parent(parents.key, 0), Some(paragraph_index));
    assert_eq!(tree.root().parent_tree_next_key, Some(1));
}

#[test]
fn inline_marked_content_without_element_page_is_rejected() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    b.begin_structure_content(page, "P", 0);
    b.end_structure_content(page);
    b.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document")
                .with_element(StructureElement::new("P").with_inline_marked_content(page, 0)),
        ),
    );

    assert_invalid(
        &mut b,
        "structure integer MCID 0 needs the element /Pg to name page 0 (14.7.4.2)",
    );
}

#[test]
fn inline_marked_content_page_must_match_element_page() {
    // 元素 /Pg 落在 second，简写却声称 first 的 MCID：整数值无法定位页。
    let mut mismatch = PdfBuilder::with_version(PdfVersion::V1_7);
    let first = mismatch.page(PageSize::A4);
    let second = mismatch.page(PageSize::A4);
    mismatch.set_structure_tree(inline_tree_mismatched(second, first, 0));
    assert_invalid(
        &mut mismatch,
        "structure integer MCID 0 names page 0 but the element /Pg is page 1 (14.7.4.2)",
    );
}

/// 元素 `/Pg` 与简写页不一致的树（`inline_tree` 两者同页，无法表达）。
fn inline_tree_mismatched(element_page: usize, kid_page: usize, mcid: i64) -> StructureTree {
    StructureTree::new().with_element(
        StructureElement::new("Document").with_element(
            StructureElement::new("P")
                .with_page(element_page)
                .with_inline_marked_content(kid_page, mcid),
        ),
    )
}

#[test]
fn structure_element_page_out_of_range_is_rejected_before_any_object_number() {
    let mut out_of_range = PdfBuilder::with_version(PdfVersion::V1_7);
    out_of_range.page(PageSize::A4);
    out_of_range.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document").with_element(
                StructureElement::new("P")
                    .with_page(7)
                    .with_marked_content(0, 0),
            ),
        ),
    );
    assert_invalid(
        &mut out_of_range,
        "structure element /Pg page 7 is out of range (page count 1) (Table 323, 14.7.4.2)",
    );
}

#[test]
fn marked_content_keeps_explicit_page_dict_when_the_element_also_carries_page() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    b.begin_structure_content(page, "P", 0);
    b.end_structure_content(page);
    // 同一元素同时携带 `/Pg` 与 MCR 子项：MCR 仍写自己的显式 `/Pg`
    // （Table 324），不被降级成整数简写。
    b.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document").with_element(
                StructureElement::new("P")
                    .with_page(page)
                    .with_marked_content(page, 0),
            ),
        ),
    );

    let doc = build_and_parse(&mut b);
    let tree = doc
        .structure_tree()
        .expect("structure tree")
        .expect("structure tree present");
    let paragraph = tree
        .elements()
        .iter()
        .find(|element| element.structure_type == "P")
        .expect("P element");
    match &paragraph.kids[..] {
        [ReaderKid::MarkedContent(mcr)] => {
            assert_eq!(mcr.mcid, 0);
            assert_eq!(
                mcr.page,
                Some(page_references(&doc)[page]),
                "MCR 的 /Pg 独立于元素 /Pg"
            );
        }
        other => panic!("应只有一个 MCR 子项，实际 {other:?}"),
    }
}

#[test]
fn inline_marked_content_range_checks_match_the_mcr_form() {
    // 整数值没有 `/MCID` 条目可查，但边界与重复声称必须和 MCR 字典同规则。
    let mut negative = PdfBuilder::with_version(PdfVersion::V1_7);
    negative.page(PageSize::A4);
    negative.set_structure_tree(inline_tree(0, -1));
    assert_invalid(
        &mut negative,
        "structure MCR MCID must be non-negative, got -1",
    );

    let mut too_large = PdfBuilder::with_version(PdfVersion::V1_7);
    too_large.page(PageSize::A4);
    too_large.set_structure_tree(inline_tree(0, 65536));
    assert_invalid(
        &mut too_large,
        "structure MCR MCID 65536 exceeds the limit 65535",
    );

    let mut duplicated = PdfBuilder::with_version(PdfVersion::V1_7);
    duplicated.page(PageSize::A4);
    duplicated.set_structure_tree(
        StructureTree::new()
            .with_element(
                StructureElement::new("Document").with_element(
                    StructureElement::new("P")
                        .with_page(0)
                        .with_inline_marked_content(0, 0),
                ),
            )
            .with_element(StructureElement::new("Span").with_marked_content(0, 0)),
    );
    assert_invalid(
        &mut duplicated,
        "page 0 MCID 0 is claimed by more than one structure element",
    );
}

#[test]
fn marked_content_dict_form_still_writes_type_mcr_when_the_element_has_no_page() {
    // 回归护栏：不调用 `with_page` 时既有 MCR 形态仍写 `/Type /MCR` 字典，
    // 且全篇只有 MCR 自带的一处 `/Pg`。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = b.page(PageSize::A4);
    b.begin_structure_content(page, "P", 0);
    b.end_structure_content(page);
    b.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document")
                .with_element(StructureElement::new("P").with_marked_content(page, 0)),
        ),
    );

    let raw = render(&mut b);
    assert!(raw.contains("/Type /MCR"), "{raw}");
    assert!(raw.contains("/MCID 0"), "{raw}");
    assert_eq!(raw.matches("/Pg ").count(), 1, "仅 MCR 一处 /Pg: {raw}");
}

#[test]
fn inline_marked_content_counts_toward_page_marked_content_consistency() {
    // 标记与声称一致时通过，且写出的是裸整数而非 MCR 字典。
    let mut ok = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = ok.page(PageSize::A4);
    ok.begin_structure_content(page, "P", 3);
    ok.end_structure_content(page);
    ok.set_structure_tree(inline_tree(page, 3));
    let raw = render(&mut ok);
    assert!(raw.contains("/P << /MCID 3 >> BDC"), "{raw}");
    let doc = build_and_parse(&mut ok);
    let tree = doc
        .structure_tree()
        .expect("structure tree")
        .expect("structure tree present");
    let parents = doc
        .page_struct_parents(page)
        .expect("page parents")
        .expect("/StructParents");
    assert_eq!(
        parents.parents,
        vec![None, None, None, Some(1)],
        "稀疏 MCID 3 在页面父树数组留下空位"
    );
    let paragraph = &tree.elements()[1];
    assert!(
        matches!(paragraph.kids[..], [ReaderKid::Mcid(3)]),
        "{:?}",
        paragraph.kids
    );
}
