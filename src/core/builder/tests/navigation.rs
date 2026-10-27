//! 文档级导航写端测试：页面标签、命名目标、命名页面与大纲
//! （ISO 32000-1 12.3、12.4.2、12.7.6）。

use super::*;
use crate::core::reader::navigation::{
    Destination, OutlineDestination, PageLabelRange, PageLabelStyle,
};
use crate::core::types::PdfVersion;
use crate::core::writer::WriteError;

/// 与 `text_string_value` 相同的文本字符串编码，用于断言十六进制字面量。
fn text_hex(text: &str) -> String {
    let bytes = crate::core::types::PdfTextString::from(text).encode();
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02X}");
    }
    hex
}

fn label(start: usize, style: Option<PageLabelStyle>, prefix: &str, first: u64) -> PageLabelRange {
    PageLabelRange {
        start,
        style,
        prefix: prefix.into(),
        first_number: first,
    }
}

fn destination(page: usize) -> OutlineDestination {
    OutlineDestination::Explicit(Destination::Fit { page })
}

/// 版本门控用例：目标版本 + 配置构建器特性的闭包。
type VersionCase = (PdfVersion, fn(&mut PdfBuilder));

fn entry(title: &str, destination: OutlineDestination) -> OutlineEntry {
    OutlineEntry {
        title: title.into(),
        destination: Some(destination),
        action: None,
        color: None,
        flags: 0,
        expanded: false,
        children: Vec::new(),
    }
}

fn build_error(builder: &mut PdfBuilder) -> (WriteError, usize) {
    let mut out = Vec::new();
    let error = builder.build(&mut out).expect_err("build should fail");
    (error, out.len())
}

#[test]
fn page_labels_write_number_tree() {
    let mut b = PdfBuilder::new();
    for _ in 0..3 {
        b.page(PageSize::A4);
    }
    b.page_labels(&[
        label(0, None, "Cover", 1),
        label(1, Some(PageLabelStyle::Decimal), "", 1),
        label(2, Some(PageLabelStyle::LowercaseRoman), "p-", 4),
    ]);
    let s = render(&mut b);

    assert!(s.contains("/PageLabels <<"), "{s}");
    assert!(s.contains("/Nums ["), "{s}");
    assert!(s.contains(&format!("/P <{}>", text_hex("Cover"))), "{s}");
    assert!(s.contains("/S /D"), "{s}");
    assert!(s.contains(&format!("/P <{}>", text_hex("p-"))), "{s}");
    assert!(s.contains("/S /r"), "{s}");
    assert!(s.contains("/St 4"), "{s}");
    assert!(!s.contains("/St 1"), "缺省起始号 1 不应写出: {s}");
}

#[test]
fn navigation_absent_leaves_catalog_untouched() {
    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    let s = render(&mut b);

    assert!(!s.contains("/PageLabels"), "{s}");
    assert!(!s.contains("/Outlines"), "{s}");
    assert!(!s.contains("/Names <<"), "{s}");
}

#[test]
fn named_destinations_and_pages_write_name_trees() {
    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    b.page(PageSize::A4);
    b.named_destination("intro", Destination::Fit { page: 0 });
    b.named_destination(
        "zoom",
        Destination::Xyz {
            page: 1,
            left: Some(10.0),
            top: None,
            zoom: Some(2.0),
        },
    );
    b.named_page("chapter", 1);
    let s = render(&mut b);

    assert!(s.contains("/Names <<"), "{s}");
    assert!(s.contains("/Dests <<"), "{s}");
    assert!(s.contains("/Pages <<"), "{s}");
    assert!(s.contains(&format!("<{}>", text_hex("intro"))), "{s}");
    assert!(s.contains(&format!("<{}>", text_hex("zoom"))), "{s}");
    assert!(s.contains(&format!("<{}>", text_hex("chapter"))), "{s}");
    assert!(s.contains("/Fit]"), "{s}");
    assert!(s.contains("/XYZ "), "{s}");
    assert!(s.contains("null"), "{s}");
    // 命名页面必须是指向 Page 对象的间接引用（对象 3 = 第 1 页）。
    assert!(s.contains("3 0 R"), "{s}");
}

#[test]
fn outline_writes_linked_items_and_counts() {
    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    b.page(PageSize::A4);
    let mut chapter = entry(
        "Chapter 1",
        OutlineDestination::Explicit(Destination::FitH {
            page: 0,
            top: Some(700.0),
        }),
    );
    chapter.color = Some([0.25, 0.5, 0.75]);
    chapter.flags = 3;
    chapter.expanded = true;
    chapter.children = vec![
        entry("Section 1.1", destination(0)),
        entry("Section 1.2", destination(1)),
    ];
    let mut appendix = entry("Appendix", OutlineDestination::Named("appendix".into()));
    appendix.children = vec![entry("Details", destination(1))];
    b.outline(&[chapter, appendix]);
    let s = render(&mut b);

    assert!(s.contains("/Type /Outlines"), "{s}");
    assert!(
        s.contains(&format!("/Title <{}>", text_hex("Chapter 1"))),
        "{s}"
    );
    assert!(
        s.contains(&format!("/Title <{}>", text_hex("Appendix"))),
        "{s}"
    );
    assert!(s.contains("/C [0.25 0.5 0.75]"), "{s}");
    assert!(s.contains("/F 3"), "{s}");
    // 展开条目 Count = 可见子项数；折叠条目 Count 为负；根 Count = 可见总数。
    assert!(s.contains("/Count 2"), "{s}");
    assert!(s.contains("/Count -1"), "{s}");
    assert!(s.contains("/Count 4"), "{s}");
    assert!(s.contains("/Prev"), "{s}");
    assert!(s.contains("/Next"), "{s}");
    assert!(s.contains("/Parent"), "{s}");
    assert!(s.contains("/FitH "), "{s}");
}

#[test]
fn navigation_rejects_invalid_input_without_output() {
    // 标签起点越界。
    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    b.page_labels(&[label(1, Some(PageLabelStyle::Decimal), "", 1)]);
    let (error, len) = build_error(&mut b);
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert_eq!(len, 0);

    // 标签起点乱序。
    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    b.page(PageSize::A4);
    b.page_labels(&[
        label(1, Some(PageLabelStyle::Decimal), "", 1),
        label(0, Some(PageLabelStyle::Decimal), "", 1),
    ]);
    let (error, len) = build_error(&mut b);
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert_eq!(len, 0);

    // 命名目标重复。
    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    b.named_destination("dup", Destination::Fit { page: 0 });
    b.named_destination("dup", Destination::Fit { page: 0 });
    let (error, len) = build_error(&mut b);
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert_eq!(len, 0);

    // 命名目标页码越界。
    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    b.named_destination("bad", Destination::Fit { page: 1 });
    let (error, len) = build_error(&mut b);
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert_eq!(len, 0);

    // 命名页面重复。
    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    b.named_page("dup", 0);
    b.named_page("dup", 0);
    let (error, len) = build_error(&mut b);
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert_eq!(len, 0);

    // 大纲颜色越界。
    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    let mut bad_color = entry("Bad", destination(0));
    bad_color.color = Some([1.5, 0.0, 0.0]);
    b.outline(&[bad_color]);
    let (error, len) = build_error(&mut b);
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert_eq!(len, 0);

    // 大纲标志位越界。
    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    let mut bad_flags = entry("Bad", destination(0));
    bad_flags.flags = 4;
    b.outline(&[bad_flags]);
    let (error, len) = build_error(&mut b);
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert_eq!(len, 0);

    // 大纲目标页越界。
    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    b.outline(&[entry("Bad", destination(1))]);
    let (error, len) = build_error(&mut b);
    assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    assert_eq!(len, 0);
}

#[test]
fn navigation_version_gates_require_minimum_versions() {
    let configure_all = |b: &mut PdfBuilder| {
        b.page(PageSize::A4);
        b.page_labels(&[label(0, Some(PageLabelStyle::Decimal), "", 1)]);
        b.named_destination("intro", Destination::Fit { page: 0 });
        b.named_page("chapter", 0);
        b.outline(&[entry("Chapter", destination(0))]);
    };

    // 各特性低于最低版本时构建失败且零字节。
    let cases: [VersionCase; 4] = [
        (PdfVersion::V1_2, |b| {
            b.page(PageSize::A4);
            b.page_labels(&[label(0, Some(PageLabelStyle::Decimal), "", 1)]);
        }),
        (PdfVersion::V1_1, |b| {
            b.page(PageSize::A4);
            b.named_destination("intro", Destination::Fit { page: 0 });
        }),
        (PdfVersion::V1_2, |b| {
            b.page(PageSize::A4);
            b.named_page("chapter", 0);
        }),
        (PdfVersion::V1_0, |b| {
            b.page(PageSize::A4);
            b.outline(&[entry("Chapter", destination(0))]);
        }),
    ];
    for (version, configure) in cases {
        let mut b = PdfBuilder::with_version(version);
        configure(&mut b);
        let (error, len) = build_error(&mut b);
        assert!(
            matches!(error, WriteError::UnsupportedVersion(_)),
            "{version}: {error}"
        );
        assert_eq!(len, 0, "{version}");
    }

    // 1.3 起全部可用。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_3);
    configure_all(&mut b);
    let mut out = Vec::new();
    b.build(&mut out).expect("PDF 1.3 navigation should build");
    assert!(!out.is_empty());
}

#[test]
fn outline_with_action_writes_a_reference() {
    let mut builder = PdfBuilder::new();
    builder.page(PageSize::A4);
    let action_handle = builder.action(crate::core::builder::types::Action::Uri(
        "https://example.com".into(),
    ));
    let entry = OutlineEntry {
        title: "Link".into(),
        destination: None,
        action: Some(action_handle),
        color: None,
        flags: 0,
        expanded: false,
        children: Vec::new(),
    };
    builder.outline(&[entry]);
    let mut out = Vec::new();
    builder.build(&mut out).expect("build should succeed");
    assert!(!out.is_empty(), "output should not be empty");
}

#[test]
fn outline_action_requires_pdf_1_2() {
    let mut builder = PdfBuilder::with_version(PdfVersion::V1_1);
    builder.page(PageSize::A4);
    let action_handle = builder.action(crate::core::builder::types::Action::Uri(
        "https://example.com".into(),
    ));
    let entry = OutlineEntry {
        title: "Link".into(),
        destination: None,
        action: Some(action_handle),
        color: None,
        flags: 0,
        expanded: false,
        children: Vec::new(),
    };
    builder.outline(&[entry]);
    let mut out = Vec::new();
    let error = builder.build(&mut out).expect_err("should fail on PDF 1.1");
    assert!(matches!(error, WriteError::UnsupportedVersion(_)));
    assert_eq!(out.len(), 0);
}
