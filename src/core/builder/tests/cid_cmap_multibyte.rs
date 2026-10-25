//! 多字节码字符码、CMapName 与 usecmap 的写端测试。

use super::*;

/// 构造一个 CIDFont 并在单页注册，返回构建器、页号与字体索引。
fn builder_with(encoding: CidEncoding) -> (PdfBuilder, usize, usize) {
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_encoding(encoding);
    let mut b = PdfBuilder::new();
    let page = b.page(PageSize::A4);
    let index = b.cid_font(page, &cid_font);
    (b, page, index)
}

/// 取 assemble 后 Type 0 字体的解码结果。
fn decode_after_assemble(b: &mut PdfBuilder, codes: &[u8]) -> Vec<(Vec<u8>, u16, Option<String>)> {
    let doc = b.assemble_document().expect("assemble");
    let (_, dict) = find_font_object(&doc, "Type0");
    let font = doc
        .type0_font(&Value::Dict(dict))
        .expect("读端应能解析 Type 0 字体")
        .expect("应为 Type 0 字体");
    font.decode_codes(codes)
        .expect("字符码应落在 codespace 内")
        .into_iter()
        .map(|code| (code.code, code.cid, code.unicode))
        .collect()
}

#[test]
fn custom_cmap_name_replaces_default() {
    let (mut b, page, index) = builder_with(
        CidEncoding::new(0)
            .with_cmap_name("My-Custom-H")
            .mapping(0x41, 7),
    );
    b.text_codes(page, index, 10.0, 10.0, &[0x41]);
    let s = render(&mut b);

    assert!(
        s.contains("/CMapName /My-Custom-H def"),
        "应写出自定义 CMapName: {s}"
    );
    assert!(
        !s.contains("/CMapName /Custom-H def"),
        "不应再写默认 CMapName: {s}"
    );
}

#[test]
fn usecmap_identity_base_is_written_before_codespace_and_inherited() {
    let (mut b, page, index) = builder_with(
        CidEncoding::new(0)
            .with_usecmap("Identity-H")
            .mapping(0x41, 7),
    );
    b.text_codes(page, index, 10.0, 10.0, &[0x41, 0x43]);
    let s = render(&mut b);

    assert!(
        s.contains("/WMode 0 def\n/Identity-H usecmap\n1 begincodespacerange"),
        "usecmap 应写在 codespace 之前: {s}"
    );

    // 本 CMap 覆盖 0x41，0x43 仍从 Identity-H 继承。
    let decoded = decode_after_assemble(&mut b, &[0x00, 0x41, 0x00, 0x43]);
    assert_eq!(decoded[0].1, 7, "本地映射应覆盖继承值");
    assert_eq!(decoded[1].1, 0x43, "未覆盖的码应继承基座映射");
}

#[test]
fn usecmap_roster_base_is_written_and_inherited() {
    let (mut b, page, index) = builder_with(
        CidEncoding::new(0)
            .with_usecmap("Adobe-GB1")
            .mapping(0xA1A1, 500),
    );
    b.text_codes(page, index, 10.0, 10.0, &[0xA1A1, 0xA1A2]);
    let s = render(&mut b);

    assert!(s.contains("/Adobe-GB1 usecmap"), "应写名册基座: {s}");
    let decoded = decode_after_assemble(&mut b, &[0xA1, 0xA1, 0xA1, 0xA2]);
    assert_eq!(decoded[0].1, 500, "本地映射应覆盖基座");
    assert_eq!(decoded[1].1, 0x0101, "相邻码应继承 Adobe-GB1 映射");
}

#[test]
fn one_byte_codes_write_codespace_and_decode() {
    let (mut b, page, index) = builder_with(
        CidEncoding::new(0)
            .mapping_bytes(&[0x41], 5)
            .mapping_bytes(&[0x42], 6),
    );
    b.text_code_bytes(page, index, 10.0, 10.0, &[&[0x41], &[0x42]]);
    let s = render(&mut b);

    assert!(
        s.contains("1 begincodespacerange\n<41> <42>\nendcodespacerange"),
        "应按 1 字节写 codespace: {s}"
    );
    assert!(
        s.contains("1 begincidrange\n<41> <42> 5\nendcidrange"),
        "连续 1 字节码应写 cidrange: {s}"
    );
    assert!(s.contains("<4142> Tj"), "内容流应按 1 字节写码: {s}");

    let decoded = decode_after_assemble(&mut b, &[0x41, 0x42]);
    assert_eq!(
        decoded.iter().map(|entry| entry.1).collect::<Vec<_>>(),
        [5, 6]
    );
}

#[test]
fn three_and_four_byte_codes_write_codespace_and_decode() {
    let (mut b, page, index) = builder_with(
        CidEncoding::new(0)
            .mapping_bytes(&[0x01, 0x02, 0x03], 100)
            .mapping_bytes(&[0x7F, 0x00, 0x00, 0x01], 200),
    );
    b.text_code_bytes(
        page,
        index,
        10.0,
        10.0,
        &[&[0x01, 0x02, 0x03], &[0x7F, 0x00, 0x00, 0x01]],
    );
    let s = render(&mut b);

    assert!(
        s.contains("2 begincodespacerange\n<010203> <010203>\n<7F000001> <7F000001>\n"),
        "应按 3/4 字节分别写 codespace: {s}"
    );
    assert!(
        s.contains("2 begincidchar\n<010203> 100\n<7F000001> 200\nendcidchar"),
        "孤立 3/4 字节码应写 cidchar: {s}"
    );
    assert!(
        s.contains("<0102037F000001> Tj"),
        "内容流应按实际长度写码: {s}"
    );

    let decoded = decode_after_assemble(&mut b, &[0x01, 0x02, 0x03, 0x7F, 0x00, 0x00, 0x01]);
    assert_eq!(
        decoded.iter().map(|entry| entry.1).collect::<Vec<_>>(),
        [100, 200]
    );
    assert_eq!(decoded[1].0.len(), 4, "4 字节码应按 4 字节回读");
}

#[test]
fn mixed_lengths_decode_by_longest_match() {
    let (mut b, page, index) = builder_with(
        CidEncoding::new(0)
            .mapping_bytes(&[0x41], 1)
            .mapping(0xA1A1, 2),
    );
    b.text_code_bytes(page, index, 10.0, 10.0, &[&[0x41], &[0xA1, 0xA1]]);
    let s = render(&mut b);

    assert!(
        s.contains("2 begincodespacerange\n<41> <41>\n<A1A1> <A1A1>\n"),
        "混合码长应分组写 codespace: {s}"
    );
    assert!(s.contains("<41A1A1> Tj"), "内容流应混合写码: {s}");

    let decoded = decode_after_assemble(&mut b, &[0x41, 0xA1, 0xA1]);
    assert_eq!(decoded[0].0, vec![0x41], "1 字节码应优先按 1 字节解析");
    assert_eq!(decoded[1].0, vec![0xA1, 0xA1], "应最长匹配 2 字节码");
    assert_eq!(
        decoded.iter().map(|entry| entry.1).collect::<Vec<_>>(),
        [1, 2]
    );
}

#[test]
fn to_unicode_keys_follow_code_length() {
    let (mut b, page, index) = builder_with(
        CidEncoding::new(0)
            .mapping_bytes(&[0x41], 1)
            .mapping(0xA1A1, 2),
    );
    b.text_code_bytes_unicode(
        page,
        index,
        10.0,
        10.0,
        &[(&[0x41], "A"), (&[0xA1, 0xA1], "中")],
    );
    let s = render(&mut b);

    assert!(s.contains("<41> <0041>"), "1 字节键应写 1 字节: {s}");
    assert!(s.contains("<A1A1> <4E2D>"), "2 字节键应写 2 字节: {s}");
    assert!(
        s.contains("2 begincodespacerange\n<41> <41>\n<A1A1> <A1A1>\nendcodespacerange"),
        "ToUnicode codespace 应按码长分组: {s}"
    );

    let decoded = decode_after_assemble(&mut b, &[0x41, 0xA1, 0xA1]);
    assert_eq!(
        decoded
            .iter()
            .map(|entry| entry.2.as_deref())
            .collect::<Vec<_>>(),
        [Some("A"), Some("中")]
    );
}

#[test]
fn usecmap_with_custom_code_length_keeps_explicit_codespace() {
    let (mut b, page, index) = builder_with(
        CidEncoding::new(0)
            .with_usecmap("Identity-H")
            .mapping_bytes(&[0x41], 3),
    );
    b.text_code_bytes(page, index, 10.0, 10.0, &[&[0x41]]);
    let s = render(&mut b);

    // 基座不提供 1 字节码空间；本 CMap 必须显式写自己的 codespace。
    assert!(
        s.contains("/Identity-H usecmap\n1 begincodespacerange\n<41> <41>\nendcodespacerange"),
        "自定义码长应显式写 codespace: {s}"
    );
    let decoded = decode_after_assemble(&mut b, &[0x41]);
    assert_eq!(decoded[0].1, 3, "本地 1 字节映射应生效");
}

#[test]
fn invalid_cmap_name_usecmap_base_code_length_and_conflict_are_rejected() {
    let cases = [
        CidEncoding::new(0).with_cmap_name("").mapping(1, 1),
        CidEncoding::new(0).with_cmap_name("Bad Name").mapping(1, 1),
        CidEncoding::new(0).with_cmap_name("Bad/Name").mapping(1, 1),
        CidEncoding::new(0)
            .with_usecmap("NoSuch-CMap")
            .mapping(1, 1),
        CidEncoding::new(0)
            .with_cmap_name("Identity-H")
            .with_usecmap("Identity-H")
            .mapping(1, 1),
        CidEncoding::new(0).mapping_bytes(&[], 1),
        CidEncoding::new(0).mapping_bytes(&[1, 2, 3, 4, 5], 1),
    ];
    for encoding in cases {
        let (mut b, page, index) = builder_with(encoding);
        b.text_codes(page, index, 10.0, 10.0, &[1]);
        let mut out = Vec::new();
        let error = b.build(&mut out).expect_err("非法编码配置应使构建失败");
        assert!(
            matches!(&error, WriteError::InvalidObject(_)),
            "应返回 InvalidObject: {error}"
        );
        assert!(out.is_empty(), "构建失败时不应写出任何字节");
    }
}
