//! E1 导航：页面标签、命名目标/页面与大纲的写出→解析往返。

use super::*;

/// E1：builder 写出的页面标签、命名目标、命名页面与大纲经解析器还原
/// （含非 ASCII 文本字符串、全部 8 种显式目标与折叠 Count）。
#[test]
fn builder_navigation_roundtrips_through_parser() {
    use pdf_engine::core::builder::{OutlineEntry, PageSize, PdfBuilder};
    use pdf_engine::core::reader::navigation::{
        Destination, OutlineDestination, PageLabelRange, PageLabelStyle,
    };

    let labels = vec![
        PageLabelRange {
            start: 0,
            style: None,
            prefix: "Cover".into(),
            first_number: 1,
        },
        PageLabelRange {
            start: 1,
            style: Some(PageLabelStyle::Decimal),
            prefix: "p.".into(),
            first_number: 1,
        },
        PageLabelRange {
            start: 2,
            style: Some(PageLabelStyle::UppercaseRoman),
            prefix: String::new(),
            first_number: 1,
        },
        PageLabelRange {
            start: 3,
            style: Some(PageLabelStyle::LowercaseLetters),
            prefix: "第 ".into(),
            first_number: 3,
        },
    ];

    let mut builder = PdfBuilder::new();
    for _ in 0..4 {
        builder.page(PageSize::A4);
    }
    builder.page_labels(&labels);

    let destinations = [
        ("fit", Destination::Fit { page: 0 }),
        (
            "fit-h",
            Destination::FitH {
                page: 1,
                top: Some(700.0),
            },
        ),
        (
            "fit-v",
            Destination::FitV {
                page: 2,
                left: Some(10.0),
            },
        ),
        (
            "fit-r",
            Destination::FitR {
                page: 3,
                rect: [0.0, 0.0, 100.0, 200.0],
            },
        ),
        ("fit-b", Destination::FitB { page: 0 }),
        ("fit-bh", Destination::FitBH { page: 1, top: None }),
        (
            "fit-bv",
            Destination::FitBV {
                page: 2,
                left: Some(5.0),
            },
        ),
        (
            "xyz",
            Destination::Xyz {
                page: 3,
                left: Some(1.0),
                top: Some(2.0),
                zoom: Some(1.5),
            },
        ),
    ];
    for (name, destination) in &destinations {
        builder.named_destination(*name, destination.clone());
    }
    builder.named_page("appendix", 3);
    builder.named_page("start", 0);

    let chapter = OutlineEntry {
        title: "Chapter 1".into(),
        destination: Some(OutlineDestination::Explicit(Destination::Fit { page: 0 })),
        action: None,
        color: Some([1.0, 0.0, 0.0]),
        flags: 3,
        expanded: true,
        children: vec![
            OutlineEntry {
                title: "Section 1.1".into(),
                destination: Some(OutlineDestination::Explicit(Destination::FitH {
                    page: 1,
                    top: Some(700.0),
                })),
                action: None,
                color: None,
                flags: 0,
                expanded: false,
                children: Vec::new(),
            },
            OutlineEntry {
                title: "Section 1.2".into(),
                destination: Some(OutlineDestination::Named("fit".into())),
                action: None,
                color: None,
                flags: 0,
                expanded: false,
                children: Vec::new(),
            },
        ],
    };
    let appendix = OutlineEntry {
        title: "附录 A".into(),
        destination: Some(OutlineDestination::Explicit(Destination::FitR {
            page: 3,
            rect: [0.0, 0.0, 100.0, 200.0],
        })),
        action: None,
        color: None,
        flags: 0,
        expanded: false,
        children: vec![OutlineEntry {
            title: "Details".into(),
            destination: Some(OutlineDestination::Explicit(Destination::FitB { page: 0 })),
            action: None,
            color: None,
            flags: 0,
            expanded: false,
            children: Vec::new(),
        }],
    };
    builder.outline(&[chapter, appendix]);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    assert_eq!(reparsed.page_count().expect("page count"), 4);
    let parsed_labels = reparsed.page_labels().expect("labels").expect("present");
    assert_eq!(parsed_labels.ranges(), labels.as_slice());
    assert_eq!(
        reparsed.page_label(0).expect("label 0").as_deref(),
        Some("Cover")
    );
    assert_eq!(
        reparsed.page_label(1).expect("label 1").as_deref(),
        Some("p.1")
    );
    assert_eq!(
        reparsed.page_label(2).expect("label 2").as_deref(),
        Some("I")
    );
    assert_eq!(
        reparsed.page_label(3).expect("label 3").as_deref(),
        Some("第 c")
    );

    for (name, destination) in &destinations {
        assert_eq!(
            reparsed.named_destination(name).expect("named destination"),
            Some(destination.clone()),
            "{name}"
        );
    }

    assert_eq!(
        reparsed.named_page("appendix").expect("named page"),
        Some(3)
    );
    assert_eq!(reparsed.named_page("start").expect("named page"), Some(0));

    let outline = reparsed.outline().expect("outline").expect("present");
    assert_eq!(outline.items.len(), 2);
    let first = &outline.items[0];
    assert_eq!(first.title, "Chapter 1");
    assert_eq!(
        first.destination,
        Some(OutlineDestination::Explicit(Destination::Fit { page: 0 }))
    );
    assert_eq!(first.color, Some([1.0, 0.0, 0.0]));
    assert_eq!(first.flags, 3);
    assert_eq!(first.count, 2);
    assert_eq!(first.children.len(), 2);
    assert_eq!(
        first.children[1].destination,
        Some(OutlineDestination::Named("fit".into()))
    );
    assert_eq!(first.children[1].count, 0);
    let second = &outline.items[1];
    assert_eq!(second.title, "附录 A");
    assert_eq!(second.count, -1);
    assert_eq!(second.children.len(), 1);
    assert_eq!(second.children[0].title, "Details");
}

/// 旧式命名目标（12.3.2.3 的 PDF 1.1 形态：目录 `/Dests` 字典）写出 →
/// 解析 → 按名解析目标；目录条目必须是间接引用，未注册的名字返回 `None`。
#[test]
fn builder_legacy_named_destinations_roundtrip_through_parser() {
    use pdf_engine::core::builder::{PageSize, PdfBuilder};
    use pdf_engine::core::reader::navigation::Destination;
    use pdf_engine::core::types::PdfVersion;

    let destinations = [
        ("fit", Destination::Fit { page: 0 }),
        (
            "fit-h",
            Destination::FitH {
                page: 1,
                top: Some(700.0),
            },
        ),
        (
            "fit-v",
            Destination::FitV {
                page: 1,
                left: Some(10.0),
            },
        ),
        (
            "fit-r",
            Destination::FitR {
                page: 0,
                rect: [0.0, 0.0, 100.0, 200.0],
            },
        ),
        ("fit-b", Destination::FitB { page: 1 }),
        ("fit-bh", Destination::FitBH { page: 0, top: None }),
        (
            "fit-bv",
            Destination::FitBV {
                page: 0,
                left: Some(5.0),
            },
        ),
        (
            "xyz",
            Destination::Xyz {
                page: 1,
                left: Some(1.0),
                top: Some(2.0),
                zoom: Some(1.5),
            },
        ),
    ];

    // PDF 1.1 是旧式通道的最低版本：无对象流，目录条目明文可见。
    let mut builder = PdfBuilder::with_version(PdfVersion::V1_1);
    builder.page(PageSize::A4);
    builder.page(PageSize::A4);
    for (name, destination) in &destinations {
        builder.legacy_named_destination(*name, destination.clone());
    }

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("legacy dests should build");
    let text = String::from_utf8_lossy(&buf);
    let at = text.find("/Dests ").expect("catalog /Dests entry");
    let rest = &text[at + "/Dests ".len()..];
    assert!(
        rest.starts_with(|c: char| c.is_ascii_digit()),
        "Table 28 要求目录 /Dests 是间接引用，实得 {rest}"
    );

    let reparsed = parse_buf(&buf);
    assert_eq!(reparsed.page_count().expect("page count"), 2);
    for (name, destination) in &destinations {
        assert_eq!(
            reparsed.named_destination(name).expect("named destination"),
            Some(destination.clone()),
            "{name}"
        );
    }
    assert_eq!(reparsed.named_destination("absent").expect("lookup"), None);
}

/// 旧式命名目标的名字是**名字对象**：含需转义字节（空格、`#`）时经写端
/// `write_name` 的 `#xx` 转义与读端 `decode_name_escapes` 还原后仍可查得。
#[test]
fn builder_legacy_named_destination_survives_name_escape_roundtrip() {
    use pdf_engine::core::builder::{OutlineEntry, PageSize, PdfBuilder};
    use pdf_engine::core::reader::navigation::{Destination, OutlineDestination};

    let spaced = "Chap 6";
    let hashed = "fig#1";
    let spaced_target = Destination::Fit { page: 0 };
    let hashed_target = Destination::FitB { page: 1 };

    let mut builder = PdfBuilder::new();
    builder.page(PageSize::A4);
    builder.page(PageSize::A4);
    builder.legacy_named_destination(spaced, spaced_target.clone());
    builder.legacy_named_destination(hashed, hashed_target.clone());
    // 名称树通道（1.2+ 文本串形态）同时注册：读端名称树优先，两通道互不干扰。
    builder.named_destination("tree", Destination::Fit { page: 1 });

    // 大纲用命名目标引用旧式名字，验证命名目标可被导航结构消费。
    builder.outline(&[OutlineEntry {
        title: "Chapter".into(),
        destination: Some(OutlineDestination::Named(spaced.into())),
        action: None,
        color: None,
        flags: 0,
        expanded: false,
        children: Vec::new(),
    }]);

    let mut buf = Vec::new();
    builder
        .build(&mut buf)
        .expect("escaped legacy names should build");
    let text = String::from_utf8_lossy(&buf);
    // 写出侧确有 `#xx` 转义：空格 -> `#20`，`#` -> `#23`。
    assert!(text.contains("/Chap#206"), "{text}");
    assert!(text.contains("/fig#231"), "{text}");

    let reparsed = parse_buf(&buf);
    assert_eq!(
        reparsed
            .named_destination(spaced)
            .expect("spaced legacy destination"),
        Some(spaced_target)
    );
    assert_eq!(
        reparsed
            .named_destination(hashed)
            .expect("hashed legacy destination"),
        Some(hashed_target)
    );
    assert_eq!(
        reparsed
            .named_destination("tree")
            .expect("name tree destination"),
        Some(Destination::Fit { page: 1 })
    );

    let outline = reparsed
        .outline()
        .expect("outline")
        .expect("outline present");
    assert_eq!(
        outline.items[0].destination,
        Some(OutlineDestination::Named(spaced.into()))
    );
}
