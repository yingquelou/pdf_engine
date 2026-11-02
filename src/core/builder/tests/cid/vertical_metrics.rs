//! CIDFont `/DW2` `/W2` 纵向度量的写出、默认省略与非法取值拒绝。

use super::super::*;

#[test]
fn cid_font_writes_dw2_and_w2_vertical_metrics() {
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_default_vertical_metrics(900.0, -800.0)
    .with_vertical_metrics(vec![
        CidVerticalMetrics::Single(1, -500.0, 250.0, 700.0),
        CidVerticalMetrics::Range(5, 6, -600.0, 300.0, 800.0),
    ]);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_cid(p, idx, 10.0, 10.0, &[1]);
    let doc = b.assemble_document().expect("assemble");

    let (_, descendant) = find_font_object(&doc, "CIDFontType2");
    let dw2 = descendant.get("DW2").expect("/DW2");
    assert_eq!(numbers(dw2), vec![900.0, -800.0], "/DW2 为 [v_y w1_y]");

    let w2 = descendant.get("W2").and_then(Value::as_array).expect("/W2");
    assert_eq!(w2.len(), 7, "两项 /W2 条目共 2 + 5 个元素");
    assert_eq!(scalar(Some(&w2.0[0])), Some(1.0));
    assert_eq!(numbers(&w2.0[1]), vec![-500.0, 250.0, 700.0]);
    assert_eq!(scalar(Some(&w2.0[2])), Some(5.0));
    assert_eq!(scalar(Some(&w2.0[3])), Some(6.0));
    assert_eq!(
        w2.0[4..]
            .iter()
            .map(|value| scalar(Some(value)).expect("number"))
            .collect::<Vec<_>>(),
        vec![-600.0, 300.0, 800.0]
    );
}

#[test]
fn cid_font_omits_dw2_and_w2_by_default() {
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType0,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    );

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_cid(p, idx, 10.0, 10.0, &[1]);
    let doc = b.assemble_document().expect("assemble");

    let (_, descendant) = find_font_object(&doc, "CIDFontType0");
    assert!(descendant.get("DW2").is_none(), "默认 /DW2 应省略");
    assert!(descendant.get("W2").is_none(), "无条目时不应写 /W2");
}

#[test]
fn cid_font_rejects_invalid_vertical_metrics() {
    let build_error = |cid_font: &CidFont| {
        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        let idx = b.cid_font(p, cid_font);
        b.text_cid(p, idx, 10.0, 10.0, &[1]);
        let mut out = Vec::new();
        b.build(&mut out).expect_err("invalid vertical metrics")
    };

    let nan_dw2 = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_default_vertical_metrics(f64::NAN, -1000.0);
    let error = build_error(&nan_dw2);
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(error.to_string().contains("/DW2"), "{error}");

    let infinite_w2 = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_vertical_metrics(vec![CidVerticalMetrics::Single(1, f64::INFINITY, 0.0, 0.0)]);
    let error = build_error(&infinite_w2);
    assert!(error.to_string().contains("/W2"), "{error}");

    let reversed_range = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_vertical_metrics(vec![CidVerticalMetrics::Range(6, 5, -500.0, 250.0, 700.0)]);
    let error = build_error(&reversed_range);
    assert!(
        error.to_string().contains("ends before it starts"),
        "{error}"
    );
}
