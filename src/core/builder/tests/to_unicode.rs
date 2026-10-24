//! ToUnicode CMap 生成测试。

use super::*;

#[test]
fn text_cid_unicode_writes_to_unicode_cmap() {
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    );

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_cid_unicode(p, idx, 10.0, 10.0, &[(1, "T"), (2, "é"), (3, "😀")]);
    let s = render(&mut b);

    assert!(
        s.contains("/ToUnicode"),
        "Type 0 字体应引用 /ToUnicode: {s}"
    );
    assert!(
        s.contains("/CMapName /Adobe-Identity-UCS"),
        "应写 CMap 头: {s}"
    );
    assert!(s.contains("3 beginbfchar"), "应写三条 bfchar: {s}");
    assert!(s.contains("<0001> <0054>"), "CID 1 → T 映射: {s}");
    assert!(s.contains("<0002> <00E9>"), "CID 2 → é 映射: {s}");
    assert!(
        s.contains("<0003> <D83DDE00>"),
        "补充平面字符应写 UTF-16BE 代理对: {s}"
    );
    assert!(
        s.contains("<000100020003> Tj"),
        "内容流应按 Identity-H 写 CID 序列: {s}"
    );
}

#[test]
fn to_unicode_cmap_uses_bfrange_for_consecutive_cids() {
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    );

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_cid_unicode(
        p,
        idx,
        10.0,
        10.0,
        &[(1, "A"), (2, "B"), (3, "C"), (4, "D"), (5, "E")],
    );
    let s = render(&mut b);

    assert!(
        s.contains("1 beginbfrange"),
        "连续 CID 应合并为一条 bfrange: {s}"
    );
    assert!(
        s.contains("<0001> <0005> <0041>"),
        "应写 <起始 CID> <结束 CID> <首个目标码元>: {s}"
    );
    assert!(
        !s.contains("beginbfchar"),
        "全部映射并入 bfrange 后不应再写 bfchar: {s}"
    );
    assert!(
        !s.contains("<0001> <0041>"),
        "被 bfrange 覆盖的 CID 不应重复写 bfchar: {s}"
    );
}

#[test]
fn to_unicode_cmap_keeps_surrogate_pairs_and_discontinuous_targets_in_bfchar() {
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    );

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_cid_unicode(
        p,
        idx,
        10.0,
        10.0,
        &[(1, "A"), (2, "B"), (3, "😀"), (4, "AB"), (5, "F"), (6, "E")],
    );
    let s = render(&mut b);

    // 代理对之前的 A/B 仍可合并，代理对本身必须逐条 bfchar。
    assert!(
        s.contains("<0001> <0002> <0041>"),
        "代理对之前的连续段应写 bfrange: {s}"
    );
    assert!(
        s.contains("4 beginbfchar"),
        "代理对、多码元串与目标跳变应各写一条 bfchar: {s}"
    );
    assert!(
        s.contains("<0003> <D83DDE00>"),
        "补充平面字符应写 UTF-16BE 代理对: {s}"
    );
    assert!(s.contains("<0004> <00410042>"), "多码元串应写 bfchar: {s}");
    assert!(
        s.contains("<0005> <0046>") && s.contains("<0006> <0045>"),
        "目标码元跳变（F → E）不应并入 bfrange: {s}"
    );
    assert!(
        s.contains("1 beginbfrange\n<0001> <0002> <0041>\nendbfrange"),
        "bfrange 节应只覆盖 A/B 两个连续 CID: {s}"
    );
}

#[test]
fn to_unicode_cmap_splits_sections_at_100_entries() {
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    );

    // 150 个 CID 的目标步长为 2，无法合并成 bfrange，全部走 bfchar。
    let texts: Vec<String> = (0..150u16)
        .map(|i| {
            char::from_u32(0x4E00 + u32::from(i) * 2)
                .expect("BMP 字符应有效")
                .to_string()
        })
        .collect();
    let glyphs: Vec<(u16, &str)> = texts
        .iter()
        .enumerate()
        .map(|(i, text)| (i as u16 + 1, text.as_str()))
        .collect();

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_cid_unicode(p, idx, 10.0, 10.0, &glyphs);
    let s = render(&mut b);

    assert!(!s.contains("beginbfrange"), "目标跳变不应生成 bfrange: {s}");
    assert_eq!(
        s.matches("beginbfchar").count(),
        2,
        "150 条 bfchar 应拆成两节: {s}"
    );
    assert!(s.contains("100 beginbfchar"), "首节应为 100 条: {s}");
    assert!(s.contains("50 beginbfchar"), "次节应为 50 条: {s}");
    assert!(s.contains("<0001> <4E00>"), "首条映射: {s}");
    assert!(s.contains("<0096> <4F2A>"), "末条映射: {s}");
}

#[test]
fn text_cid_unicode_last_mapping_wins() {
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    );

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_cid_unicode(p, idx, 10.0, 10.0, &[(1, "A")]);
    b.text_cid_unicode(p, idx, 10.0, 30.0, &[(1, "B")]);
    let s = render(&mut b);

    assert!(s.contains("1 beginbfchar"), "同一 CID 只应保留一条: {s}");
    assert!(s.contains("<0001> <0042>"), "应保留最后一次映射: {s}");
    assert!(!s.contains("<0001> <0041>"), "旧映射不应写出: {s}");
}
