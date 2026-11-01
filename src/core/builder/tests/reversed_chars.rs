//! 反序显示串（ISO 32000-1 14.8.2.3.3，Tagged PDF 自 PDF 1.4 引入）的写端测试：
//! [`PdfBuilder::begin_reversed_chars`] 的
//! `/ReversedChars << /ReversedChars true >> BDC` + 逐串反转 `Tj` 输出、
//! 特征画像登记，以及构建期门禁对空串、内部 U+0020 空格、非可打印内容、
//! 无显示串/未闭合区段、`MP`/`DP` 点形态与 PDF 1.4 版本不足的拒绝。
//! 全部失败均要求零字节输出。

use super::*;
use crate::core::content::MarkedContentProperties;
use crate::core::types::{Dict, PdfFeature, PdfVersion, Value};

/// 指定版本的单页构建器。
fn one_page_builder(version: PdfVersion) -> (PdfBuilder, usize) {
    let mut builder = PdfBuilder::with_version(version);
    let page = builder.page(PageSize::A4);
    (builder, page)
}

fn s(value: &str) -> String {
    value.to_owned()
}

/// 本写端属性表的规范形态：`/ReversedChars true` 自证条目。
fn reversed_dict() -> MarkedContentProperties {
    MarkedContentProperties::Dict(Dict::new().insert("ReversedChars", Value::Bool(true)))
}

/// 构建并断言失败为 [`WriteError::InvalidObject`]、消息含子串且零字节。
fn assert_invalid(builder: &mut PdfBuilder, expected: &str) {
    let mut out = Vec::new();
    let error = builder
        .build(&mut out)
        .expect_err("非法反序显示串写法应被拒绝");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(error.to_string().contains(expected), "{error}");
    assert!(out.is_empty(), "失败时不应写出任何字节");
}

#[test]
fn begin_reversed_chars_writes_property_list_and_reversed_show_strings() {
    let (mut builder, page) = one_page_builder(PdfVersion::V1_7);
    builder.begin_reversed_chars(
        page,
        &Font::helvetica(12.0),
        72.0,
        700.0,
        &[s("Hello "), s("world")],
    );
    builder.end_reversed_chars(page);
    // 未知页索引按库内约定静默忽略。
    builder.begin_reversed_chars(page + 9, &Font::helvetica(12.0), 0.0, 0.0, &[s("x")]);
    builder.end_reversed_chars(page + 9);

    let emitted = String::from_utf8_lossy(&builder.pages[page].content.to_bytes()).into_owned();
    assert!(
        emitted.contains("/ReversedChars << /ReversedChars true >> BDC"),
        "{emitted}"
    );
    assert!(emitted.contains("( olleH) Tj"), "{emitted}");
    assert!(emitted.contains("(dlrow) Tj"), "{emitted}");
    assert!(
        !emitted.contains("(Hello) Tj") && !emitted.contains("(world) Tj"),
        "显示串必须反序: {emitted}"
    );
    assert!(emitted.contains("/F1 12.0 Tf"), "{emitted}");
    assert!(emitted.contains("72.0 700.0 Td"), "{emitted}");
    assert_eq!(emitted.matches("EMC").count(), 1, "{emitted}");

    let doc = builder.assemble_document().expect("assemble");
    assert!(
        doc.features
            .borrow()
            .contains(PdfFeature::ReversedCharacters),
        "特征画像应登记反序显示串（14.8.2.3.3），供版本推导"
    );

    let document = render(&mut builder);
    assert!(
        document.contains("/ReversedChars << /ReversedChars true >> BDC"),
        "{document}"
    );
}

#[test]
fn begin_reversed_chars_rejects_interior_space_in_show_string() {
    // "Hello world" 反转成 "dlrow olleH"，U+0020 落在串中间，违反
    // 14.8.2.3.3 的 shall not contain interior SPACEs。
    let (mut builder, page) = one_page_builder(PdfVersion::V1_7);
    builder.begin_reversed_chars(
        page,
        &Font::helvetica(12.0),
        72.0,
        700.0,
        &[s("Hello world")],
    );
    builder.end_reversed_chars(page);
    assert_invalid(&mut builder, "contains an interior U+0020 space");
}

#[test]
fn begin_reversed_chars_rejects_empty_show_string() {
    let (mut builder, page) = one_page_builder(PdfVersion::V1_7);
    builder.begin_reversed_chars(
        page,
        &Font::helvetica(12.0),
        72.0,
        700.0,
        &[s("Hello"), s("")],
    );
    builder.end_reversed_chars(page);
    assert_invalid(&mut builder, "show string is empty");
}

#[test]
fn begin_reversed_chars_rejects_non_printable_show_string() {
    // API 路径：非可打印 ASCII 文本以十六进制串写出，交给门禁拒绝。
    let (mut builder, page) = one_page_builder(PdfVersion::V1_7);
    builder.begin_reversed_chars(page, &Font::helvetica(12.0), 72.0, 700.0, &[s("你好")]);
    builder.end_reversed_chars(page);
    assert_invalid(&mut builder, "hexadecimal string");

    // 手工路径：`BMC` 段内直接 `show_text` 非可打印内容，按可打印区间拒绝。
    let (mut builder, page) = one_page_builder(PdfVersion::V1_7);
    builder.pages[page]
        .content
        .begin_marked_content("ReversedChars");
    builder.pages[page].content.begin_text();
    builder.pages[page].content.show_text("café");
    builder.pages[page].content.end_text();
    builder.pages[page].content.end_marked_content();
    assert_invalid(&mut builder, "outside printable ASCII");
}

#[test]
fn begin_reversed_chars_requires_show_string_in_section() {
    // 空串列表：区段里没有任何可反转的显示串。
    let (mut builder, page) = one_page_builder(PdfVersion::V1_7);
    builder.begin_reversed_chars(page, &Font::helvetica(12.0), 72.0, 700.0, &[]);
    builder.end_reversed_chars(page);
    assert_invalid(&mut builder, "contains no show string to reverse");

    // 手工注入不带任何算子的区段同样拒绝。
    let (mut builder, page) = one_page_builder(PdfVersion::V1_7);
    builder.pages[page]
        .content
        .begin_marked_content_properties("ReversedChars", reversed_dict());
    builder.pages[page].content.end_marked_content();
    assert_invalid(&mut builder, "contains no show string to reverse");
}

#[test]
fn reversed_chars_marked_content_point_is_rejected() {
    // `MP` 点不包围任何显示串，不构成 14.8.2.3.3 的序列。
    let (mut builder, page) = one_page_builder(PdfVersion::V1_7);
    builder.pages[page]
        .content
        .marked_content_point("ReversedChars");
    assert_invalid(&mut builder, "appears as a marked-content point");

    let (mut builder, page) = one_page_builder(PdfVersion::V1_7);
    builder.pages[page]
        .content
        .marked_content_point_properties("ReversedChars", reversed_dict());
    assert_invalid(&mut builder, "appears as a marked-content point");
}

#[test]
fn reversed_chars_section_must_close_before_page_end() {
    // 缺少 `end_reversed_chars`：区段未在内容流内以 EMC 闭合。
    let (mut builder, page) = one_page_builder(PdfVersion::V1_7);
    builder.begin_reversed_chars(page, &Font::helvetica(12.0), 72.0, 700.0, &[s("Hello")]);
    assert_invalid(&mut builder, "not closed by EMC");
}

#[test]
fn reversed_chars_bmc_without_property_list_is_accepted() {
    // 规范 EXAMPLE 的 `BMC` 形态（不带属性表）：手工写法同样通过门禁。
    let (mut builder, page) = one_page_builder(PdfVersion::V1_7);
    builder.pages[page]
        .content
        .begin_marked_content("ReversedChars");
    builder.pages[page].content.begin_text();
    builder.pages[page].content.set_font("F1", 12.0);
    builder.pages[page].content.move_next_line(72.0, 700.0);
    builder.pages[page].content.show_text("olleH");
    builder.pages[page].content.end_text();
    builder.pages[page].content.end_marked_content();

    let mut out = Vec::new();
    builder.build(&mut out).expect("BMC 形态应通过门禁");
    let document = String::from_utf8_lossy(&out).into_owned();
    assert!(document.contains("/ReversedChars BMC"), "{document}");
    assert!(document.contains("(olleH) Tj"), "{document}");
}

#[test]
fn begin_reversed_chars_reuses_deduplicated_font() {
    let (mut builder, page) = one_page_builder(PdfVersion::V1_7);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "Lead");
    // 与已登记字体同 BaseFont/字号/编码：复用 F1。
    builder.begin_reversed_chars(page, &Font::helvetica(12.0), 72.0, 700.0, &[s("Hello")]);
    builder.end_reversed_chars(page);
    // 新字号：分配 F2。
    builder.begin_reversed_chars(page, &Font::helvetica(24.0), 72.0, 660.0, &[s("World")]);
    builder.end_reversed_chars(page);

    let emitted = String::from_utf8_lossy(&builder.pages[page].content.to_bytes()).into_owned();
    assert!(emitted.contains("/F1 12.0 Tf"), "{emitted}");
    assert!(emitted.contains("/F2 24.0 Tf"), "{emitted}");
    assert!(!emitted.contains("/F3"), "同字体应去重: {emitted}");
    assert!(emitted.contains("(olleH) Tj"), "{emitted}");
    assert!(emitted.contains("(dlroW) Tj"), "{emitted}");
}

#[test]
fn begin_reversed_chars_requires_pdf_1_4() {
    // PDF 1.3：反序显示串尚未引入，拒绝且不写出任何字节。
    let (mut builder, page) = one_page_builder(PdfVersion::V1_3);
    builder.begin_reversed_chars(page, &Font::helvetica(12.0), 72.0, 700.0, &[s("Hello")]);
    builder.end_reversed_chars(page);
    let mut out = Vec::new();
    let error = builder
        .build(&mut out)
        .expect_err("PDF 1.3 不应接受反序显示串");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(_)),
        "{error}"
    );
    assert!(
        error
            .to_string()
            .contains("reversed show strings require PDF 1.4, got 1.3"),
        "{error}"
    );
    assert!(out.is_empty(), "失败时不应写出任何字节");

    // PDF 1.4 起可用。
    let (mut builder, page) = one_page_builder(PdfVersion::V1_4);
    builder.begin_reversed_chars(page, &Font::helvetica(12.0), 72.0, 700.0, &[s("Hello")]);
    builder.end_reversed_chars(page);
    let document = render(&mut builder);
    assert!(document.contains("%PDF-1.4"), "{document}");
    assert!(
        document.contains("/ReversedChars << /ReversedChars true >> BDC"),
        "{document}"
    );
}

#[test]
fn failed_reversed_chars_validation_writes_no_bytes() {
    // 页面上同时有常规文本对象：反序段校验失败也必须整页不落一个字节。
    let (mut builder, page) = one_page_builder(PdfVersion::V1_7);
    builder.text(page, &Font::helvetica(12.0), 72.0, 700.0, "Body");
    builder.begin_reversed_chars(
        page,
        &Font::helvetica(12.0),
        72.0,
        680.0,
        &[s("Hello world")],
    );
    builder.end_reversed_chars(page);
    let mut out = Vec::new();
    let error = builder
        .build(&mut out)
        .expect_err("内部空格显示串应拒绝构建");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(out.is_empty(), "失败时不应写出任何字节: {} 字节", out.len());
}
