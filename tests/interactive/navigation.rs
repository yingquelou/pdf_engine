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
