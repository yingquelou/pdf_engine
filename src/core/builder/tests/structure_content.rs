//! 结构标记内容通道（ISO 32000-1 14.8.2）的写端测试：
//! [`PdfBuilder::begin_structure_content`] 的 `/<tag> << /MCID n >> BDC`
//! 输出，与构建期页面 `/MCID` 对逻辑结构树 MCR 的交叉校验——同页唯一、
//! 缺结构树、树上缺失（missing）、页面游离（orphan）、非整数 `/MCID`、
//! `Artifact` 误带 `/MCID`，以及「页面按至少标出一个 MCID 才选择加入」的
//! 非对称豁免。全部失败均要求零字节输出。

use super::*;
use crate::core::content::MarkedContentProperties;
use crate::core::types::{PdfVersion, Value};

/// 单页合法结构树：`Document > P`，页 `page` 上按 `mcids` 各挂一个 MCR 子项。
fn tree_with(page: usize, mcids: &[i64]) -> StructureTree {
    let mut paragraph = StructureElement::new("P");
    for mcid in mcids {
        paragraph = paragraph.with_marked_content(page, *mcid);
    }
    StructureTree::new().with_element(StructureElement::new("Document").with_element(paragraph))
}

/// 在一页上依次写出若干段结构内容（`BDC`/`EMC` 成对）。
fn mark(builder: &mut PdfBuilder, page: usize, mcids: &[i64]) {
    for mcid in mcids {
        builder.begin_structure_content(page, "P", *mcid);
        builder.end_structure_content(page);
    }
}

fn assert_invalid(builder: &mut PdfBuilder, expected: &str) {
    let mut out = Vec::new();
    let error = builder
        .build(&mut out)
        .expect_err("页面标记内容与结构树不一致应被拒绝");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(error.to_string().contains(expected), "{error}");
    assert!(out.is_empty(), "失败时不应写出任何字节");
}

fn one_page_builder() -> (PdfBuilder, usize) {
    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = builder.page(PageSize::A4);
    (builder, page)
}

#[test]
fn begin_structure_content_writes_mcid_property_list() {
    let (mut builder, page) = one_page_builder();
    builder.begin_structure_content(page, "P", 0);
    builder.text(page, &Font::helvetica(12.0), 72.0, 700.0, "Body");
    builder.end_structure_content(page);
    builder.begin_structure_content(page, "Span", 7);
    builder.end_structure_content(page);
    // 未知页索引按库内约定静默忽略。
    builder.begin_structure_content(page + 9, "P", 8);
    builder.end_structure_content(page + 9);

    let emitted = String::from_utf8_lossy(&builder.pages[page].content.to_bytes()).into_owned();
    assert!(emitted.contains("/P << /MCID 0 >> BDC"), "{emitted}");
    assert!(emitted.contains("/Span << /MCID 7 >> BDC"), "{emitted}");
    assert_eq!(emitted.matches("EMC").count(), 2, "{emitted}");
    assert!(
        !emitted.contains("/MCID 8"),
        "未知页不应写出内容: {emitted}"
    );

    builder.set_structure_tree(tree_with(page, &[0, 7]));
    let document = render(&mut builder);
    for entry in [
        "/P << /MCID 0 >> BDC",
        "/Span << /MCID 7 >> BDC",
        "/StructParents 0",
    ] {
        assert!(document.contains(entry), "缺少 {entry}: {document}");
    }
}

#[test]
fn structure_content_and_tree_agree_and_build_succeeds() {
    let (mut builder, page) = one_page_builder();
    mark(&mut builder, page, &[0, 1]);
    builder.set_structure_tree(tree_with(page, &[0, 1]));
    builder.set_mark_info(StructureMarkInfo::new().with_marked(true));
    let document = render(&mut builder);
    assert!(document.contains("/P << /MCID 0 >> BDC"), "{document}");
    assert!(document.contains("/P << /MCID 1 >> BDC"), "{document}");
    assert!(document.contains("/StructParents 0"), "{document}");
    assert!(document.contains("/ParentTreeNextKey 1"), "{document}");
}

#[test]
fn structure_content_mcid_without_tree_is_rejected() {
    let (mut builder, page) = one_page_builder();
    mark(&mut builder, page, &[0]);
    assert_invalid(
        &mut builder,
        "structure marked content: page 0 marks MCID 0 but the document has no structure tree",
    );
}

#[test]
fn structure_content_duplicate_mcid_on_page_is_rejected() {
    let (mut builder, page) = one_page_builder();
    mark(&mut builder, page, &[0, 0]);
    builder.set_structure_tree(tree_with(page, &[0]));
    assert_invalid(
        &mut builder,
        "page 0 marks MCID 0 more than once; MCIDs must be unique per page",
    );
}

#[test]
fn structure_content_tree_mcid_missing_from_page_is_rejected() {
    let (mut builder, page) = one_page_builder();
    mark(&mut builder, page, &[0]);
    builder.set_structure_tree(tree_with(page, &[0, 1]));
    assert_invalid(
        &mut builder,
        "structure elements claim MCID [1] that the page never marks",
    );
}

#[test]
fn structure_content_orphan_page_mcid_is_rejected() {
    let (mut builder, page) = one_page_builder();
    mark(&mut builder, page, &[0, 1]);
    builder.set_structure_tree(tree_with(page, &[0]));
    assert_invalid(
        &mut builder,
        "the page marks MCID [1] that no structure element claims",
    );
}

#[test]
fn structure_content_page_without_mcid_is_not_checked() {
    // 没有任何页面标记 MCID：既有结构用例的形态，树声明不参与比对。
    let (mut builder, page) = one_page_builder();
    builder.text(page, &Font::helvetica(12.0), 72.0, 700.0, "Body");
    builder.set_structure_tree(tree_with(page, &[0, 1]));
    builder
        .build(&mut Vec::new())
        .expect("只声明结构树、页面不标记 MCID 时照旧通过");

    // 页面按「至少标出一个 MCID」选择加入：未标记的页面整页豁免。
    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    let first = builder.page(PageSize::A4);
    let second = builder.page(PageSize::A4);
    mark(&mut builder, first, &[0]);
    builder.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document")
                .with_element(StructureElement::new("P").with_marked_content(first, 0))
                .with_element(StructureElement::new("Span").with_marked_content(second, 0)),
        ),
    );
    let document = render(&mut builder);
    assert!(document.contains("/P << /MCID 0 >> BDC"), "{document}");
    assert!(document.contains("/StructParents 0"), "{document}");
}

#[test]
fn non_integer_structure_mcid_is_rejected() {
    let cases = [
        Value::Float(1.0),
        Value::Name("one".into()),
        Value::String("1".into()),
        Value::Bool(true),
    ];
    for value in cases {
        let (mut builder, page) = one_page_builder();
        builder.set_structure_tree(tree_with(page, &[1]));
        builder.pages[page].content.begin_marked_content_properties(
            "P",
            MarkedContentProperties::Dict(Dict::new().insert("MCID", value.clone())),
        );
        builder.pages[page].content.end_marked_content();
        assert_invalid(
            &mut builder,
            &format!("page 0 /MCID must be an integer, got {}", value.type_name()),
        );
    }
}

#[test]
fn artifact_marked_content_with_mcid_is_rejected() {
    let (mut builder, page) = one_page_builder();
    builder.set_structure_tree(tree_with(page, &[0]));
    // 用结构内容通道标记 Artifact：14.8.2.3.1 禁止 artifact 获得结构身份。
    builder.begin_structure_content(page, "Artifact", 0);
    builder.end_structure_content(page);
    assert_invalid(&mut builder, "Artifact must not carry /MCID (14.8.2.3.1)");
}

#[test]
fn failed_structure_content_validation_writes_no_bytes() {
    let (mut builder, page) = one_page_builder();
    builder.text(page, &Font::helvetica(12.0), 72.0, 700.0, "Body");
    mark(&mut builder, page, &[0]);
    builder.set_structure_tree(tree_with(page, &[3]));
    let mut out = Vec::new();
    let error = builder
        .build(&mut out)
        .expect_err("页面 MCID 与结构树不一致应拒绝构建");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(out.is_empty(), "失败时不应写出任何字节");
}
