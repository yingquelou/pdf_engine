//! Tag suspects（ISO 32000-1 14.8.2.3.1，PDF 1.6 引入）的写端测试：
//! [`PdfBuilder::begin_tag_suspect`] 的
//! `/TagSuspect << /TagSuspect /Ordering >> BDC` 输出、构建期的属性表形态
//! 校验、目录 `/MarkInfo /Suspects true` 的强制要求与 PDF 1.6 门禁，以及
//! `BDC` 引用 `/Properties` 资源名时的刻意豁免。全部失败均要求零字节输出。

use super::*;
use crate::core::content::MarkedContentProperties;
use crate::core::types::{Dict, PdfFeature, PdfVersion, Value};

/// 指定版本的单页构建器。
fn one_page_builder(version: PdfVersion) -> (PdfBuilder, usize) {
    let mut builder = PdfBuilder::with_version(version);
    let page = builder.page(PageSize::A4);
    (builder, page)
}

/// 单页且目录已声明 `/MarkInfo << /Suspects true >>`：tag suspects 的文档级前提。
fn suspects_builder() -> (PdfBuilder, usize) {
    let (mut builder, page) = one_page_builder(PdfVersion::V1_7);
    builder.set_mark_info(
        StructureMarkInfo::new()
            .with_marked(true)
            .with_suspects(true),
    );
    (builder, page)
}

/// 注入不带属性表的 `TagSuspect` 标记内容（`BMC` 或 `MP`）后构建。
fn build_bare_form(point: bool) -> (Result<(), WriteError>, Vec<u8>) {
    let (mut builder, page) = suspects_builder();
    if point {
        builder.pages[page]
            .content
            .marked_content_point("TagSuspect");
    } else {
        builder.pages[page]
            .content
            .begin_marked_content("TagSuspect");
        builder.pages[page].content.end_marked_content();
    }
    let mut out = Vec::new();
    let result = builder.build(&mut out);
    (result, out)
}

/// 注入指定属性表的 `TagSuspect` BDC 后构建。
fn build_with_properties(properties: MarkedContentProperties) -> (Result<(), WriteError>, Vec<u8>) {
    let (mut builder, page) = suspects_builder();
    builder.pages[page]
        .content
        .begin_marked_content_properties("TagSuspect", properties);
    builder.pages[page].content.end_marked_content();
    let mut out = Vec::new();
    let result = builder.build(&mut out);
    (result, out)
}

fn assert_invalid(result: Result<(), WriteError>, out: &[u8], expected: &str) {
    let error = result.expect_err("非法 tag suspects 写法应被拒绝");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(error.to_string().contains(expected), "{error}");
    assert!(out.is_empty(), "失败时不应写出任何字节");
}

#[test]
fn begin_tag_suspect_writes_ordering_property_list() {
    let (mut builder, page) = suspects_builder();
    builder.begin_tag_suspect(page);
    builder.text(page, &Font::helvetica(12.0), 72.0, 700.0, "Suspect");
    builder.end_tag_suspect(page);
    // 未知页索引按库内约定静默忽略。
    builder.begin_tag_suspect(page + 9);
    builder.end_tag_suspect(page + 9);

    let emitted = String::from_utf8_lossy(&builder.pages[page].content.to_bytes()).into_owned();
    assert!(
        emitted.contains("/TagSuspect << /TagSuspect /Ordering >> BDC"),
        "{emitted}"
    );
    assert_eq!(emitted.matches("EMC").count(), 1, "{emitted}");
    assert!(
        !emitted.contains("/TagSuspect BMC"),
        "规范只承认带属性表的写法: {emitted}"
    );

    let doc = builder.assemble_document().expect("assemble");
    assert!(
        doc.features.borrow().contains(PdfFeature::TagSuspects),
        "特征画像应登记 tag suspects（14.8.2.3.1），供版本推导"
    );

    let document = render(&mut builder);
    for entry in [
        "/TagSuspect << /TagSuspect /Ordering >> BDC",
        "/MarkInfo <<",
        "/Suspects true",
    ] {
        assert!(document.contains(entry), "缺少 {entry}: {document}");
    }
}

#[test]
fn tag_suspect_and_structure_content_build_together() {
    let (mut builder, page) = suspects_builder();
    builder.begin_structure_content(page, "P", 0);
    builder.text(page, &Font::helvetica(12.0), 72.0, 700.0, "Body");
    builder.end_structure_content(page);
    builder.begin_tag_suspect(page);
    builder.text(page, &Font::helvetica(12.0), 72.0, 680.0, "Unknown order");
    builder.end_tag_suspect(page);
    builder.set_structure_tree(
        StructureTree::new().with_element(
            StructureElement::new("Document")
                .with_element(StructureElement::new("P").with_marked_content(page, 0)),
        ),
    );

    let document = render(&mut builder);
    for entry in [
        "/P << /MCID 0 >> BDC",
        "/TagSuspect << /TagSuspect /Ordering >> BDC",
        "/StructParents 0",
        "/ParentTreeNextKey 1",
        "/Suspects true",
    ] {
        assert!(document.contains(entry), "缺少 {entry}: {document}");
    }
    assert_eq!(
        document.matches("/TagSuspect <<").count(),
        1,
        "tag suspect 段只应出现一次: {document}"
    );
}

#[test]
fn tag_suspect_without_suspects_flag_is_rejected() {
    // 目录完全没有 /MarkInfo。
    let (mut builder, page) = one_page_builder(PdfVersion::V1_7);
    builder.begin_tag_suspect(page);
    builder.end_tag_suspect(page);
    let mut out = Vec::new();
    let result = builder.build(&mut out);
    assert_invalid(
        result,
        &out,
        "page 0 marks tag suspects but the catalog /MarkInfo does not set /Suspects true",
    );

    // 有 /MarkInfo 但 /Suspects 缺省或为 false：都不算声明 tag suspects。
    for info in [
        StructureMarkInfo::new().with_marked(true),
        StructureMarkInfo::new()
            .with_marked(true)
            .with_user_properties(true),
    ] {
        let (mut builder, page) = one_page_builder(PdfVersion::V1_7);
        builder.set_mark_info(info);
        builder.begin_tag_suspect(page);
        builder.end_tag_suspect(page);
        let mut out = Vec::new();
        let result = builder.build(&mut out);
        assert_invalid(
            result,
            &out,
            "tag suspect: page 0 marks tag suspects but the catalog /MarkInfo does not set /Suspects true (14.8.2.3.1, Table 321)",
        );
    }
}

#[test]
fn tag_suspect_bmc_without_property_list_is_rejected() {
    for point in [false, true] {
        let (result, out) = build_bare_form(point);
        assert_invalid(
            result,
            &out,
            "page 0 TagSuspect marked content has no property list",
        );
    }
}

#[test]
fn tag_suspect_property_list_without_tag_suspect_entry_is_rejected() {
    // 属性表存在但没有 `/TagSuspect` 条目。带 `/MCID` 的属性表由既有的
    // 结构 `/MCID` 门禁先接管，不在本条规则里重复报错。
    let dicts = [
        Dict::new(),
        Dict::new().insert("Type", Value::Name("Layout".into())),
        Dict::new().insert("Ordering", Value::Bool(true)),
    ];
    for dict in dicts {
        let (result, out) = build_with_properties(MarkedContentProperties::Dict(dict.clone()));
        assert_invalid(
            result,
            &out,
            "page 0 TagSuspect property list has no /TagSuspect entry",
        );
    }
}

#[test]
fn tag_suspect_property_with_wrong_value_is_rejected() {
    let values = [
        Value::Name("Sequence".into()),
        Value::Integer(1),
        Value::String("Ordering".into()),
        Value::Bool(true),
        Value::Null,
    ];
    for value in values {
        let dict = Dict::new().insert("TagSuspect", value.clone());
        let (result, out) = build_with_properties(MarkedContentProperties::Dict(dict));
        assert_invalid(
            result,
            &out,
            &format!(
                "page 0 TagSuspect property list must hold the name Ordering, got {}",
                value.type_name()
            ),
        );
    }
}

#[test]
fn tag_suspect_requires_pdf_1_6() {
    // PDF 1.5：tag suspects 尚未引入，拒绝且不写出任何字节。
    let (mut builder, page) = one_page_builder(PdfVersion::V1_5);
    builder.set_mark_info(StructureMarkInfo::new().with_suspects(true));
    builder.begin_tag_suspect(page);
    builder.end_tag_suspect(page);
    let mut out = Vec::new();
    let error = builder
        .build(&mut out)
        .expect_err("PDF 1.5 不应接受 tag suspects");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(_)),
        "{error}"
    );
    assert!(
        error
            .to_string()
            .contains("tag suspects require PDF 1.6, got 1.5"),
        "{error}"
    );
    assert!(out.is_empty(), "失败时不应写出任何字节");

    // PDF 1.6 起可用。
    let (mut builder, page) = one_page_builder(PdfVersion::V1_6);
    builder.set_mark_info(StructureMarkInfo::new().with_suspects(true));
    builder.begin_tag_suspect(page);
    builder.end_tag_suspect(page);
    let document = render(&mut builder);
    assert!(
        document.contains("/TagSuspect << /TagSuspect /Ordering >> BDC"),
        "{document}"
    );
    assert!(document.contains("%PDF-1.6"), "{document}");
    assert!(document.contains("/Suspects true"), "{document}");
}

#[test]
fn failed_tag_suspect_validation_writes_no_bytes() {
    // 页面上同时有字体与文本对象：校验失败也必须整页不落一个字节。
    let (mut builder, page) = one_page_builder(PdfVersion::V1_7);
    builder.text(page, &Font::helvetica(12.0), 72.0, 700.0, "Body");
    builder.begin_tag_suspect(page);
    builder.text(page, &Font::helvetica(12.0), 72.0, 680.0, "Suspect");
    builder.end_tag_suspect(page);
    let mut out = Vec::new();
    let error = builder
        .build(&mut out)
        .expect_err("缺少 /MarkInfo /Suspects 应拒绝构建");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(out.is_empty(), "失败时不应写出任何字节: {} 字节", out.len());

    // 版本门禁失败同样零字节。
    let (mut builder, page) = one_page_builder(PdfVersion::V1_4);
    builder.begin_tag_suspect(page);
    builder.end_tag_suspect(page);
    let mut out = Vec::new();
    let error = builder
        .build(&mut out)
        .expect_err("PDF 1.4 不应接受 tag suspects");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(_)),
        "{error}"
    );
    assert!(out.is_empty(), "失败时不应写出任何字节");
}

#[test]
fn tag_suspect_named_property_resource_is_not_rejected() {
    // `BDC` 以名字引用页 `/Properties` 资源：内容流扫描看不到属性表内容，
    // 构建期不拒绝，但仍按「含 tag suspects」处理（14.8.2.3.1 的文档级要求）。
    let (result, out) = build_with_properties(MarkedContentProperties::Name("MC0".into()));
    result.expect("引用 /Properties 资源名不应被拒绝");
    let document = String::from_utf8_lossy(&out).into_owned();
    assert!(document.contains("/TagSuspect /MC0 BDC"), "{document}");

    // 同一形态若缺 /MarkInfo /Suspects 仍要被拒绝。
    let (mut builder, page) = one_page_builder(PdfVersion::V1_7);
    builder.pages[page]
        .content
        .begin_marked_content_properties("TagSuspect", MarkedContentProperties::Name("MC0".into()));
    builder.pages[page].content.end_marked_content();
    let mut out = Vec::new();
    let result = builder.build(&mut out);
    assert_invalid(
        result,
        &out,
        "marks tag suspects but the catalog /MarkInfo does not set /Suspects true",
    );

    let (mut builder, page) = suspects_builder();
    builder.pages[page]
        .content
        .begin_marked_content_properties("TagSuspect", MarkedContentProperties::Name("MC0".into()));
    builder.pages[page].content.end_marked_content();
    let doc = builder.assemble_document().expect("assemble");
    assert!(
        doc.features.borrow().contains(PdfFeature::TagSuspects),
        "资源名形态也应登记 tag suspects 特征"
    );
}
