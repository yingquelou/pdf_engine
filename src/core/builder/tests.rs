//! builder 单元测试：对象布局、字体编码写出与内容操作符生成。

use super::*;

fn render(b: &mut PdfBuilder) -> String {
    let mut out = Vec::new();
    b.build(&mut out).expect("build should succeed");
    // PDF 是二进制；用 lossy 转成字符串做子串断言。
    String::from_utf8_lossy(&out).into_owned()
}

#[test]
fn builds_minimal_text_pdf() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &Font::helvetica(24.0), 72.0, 700.0, "Hello");
    let s = render(&mut b);

    assert!(
        s.contains("%PDF-1.7"),
        "应输出 1.7 版本头: {}",
        &s[..s.len().min(200)]
    );
    assert!(s.contains("/Type /Page"), "应含 Page 类型");
    assert!(s.contains("/Type /Catalog"), "应含 Catalog 类型");
    assert!(s.contains("/Root 1 0 R"), "trailer 应指向 Catalog");
    assert!(s.contains("BT"), "内容流应含 BT");
    assert!(s.contains("Tf"), "内容流应含 Tf");
    assert!(s.contains("Td"), "内容流应含 Td");
    assert!(s.contains("(Hello) Tj"), "内容流应含 (Hello) Tj");
    assert!(s.contains("ET"), "内容流应含 ET");
    // 字体对象
    assert!(s.contains("/Type /Font"));
    assert!(s.contains("/BaseFont /Helvetica"));
    assert!(s.contains("/Encoding /WinAnsiEncoding"));
}

#[test]
fn text_escapes_parens_and_backslash() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &Font::helvetica(12.0), 10.0, 10.0, r"a(b)c\d");
    let s = render(&mut b);
    assert!(s.contains(r"(a\(b\)c\\d) Tj"), "应转义 ( ) \\");
}

#[test]
fn image_xobject_emitted() {
    // 2x2 RGB 全红。
    let img = ImageData {
        width: 2,
        height: 2,
        color_space: ColorSpace::DeviceRGB,
        bits: 8,
        data: vec![255, 0, 0, 255, 0, 0, 255, 0, 0, 255, 0, 0],
    };
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::Letter);
    b.image(p, 100.0, 100.0, 50.0, 50.0, &img);
    let s = render(&mut b);

    assert!(s.contains("/Type /XObject"));
    assert!(s.contains("/Subtype /Image"));
    assert!(s.contains("/Width 2"));
    assert!(s.contains("/Height 2"));
    assert!(s.contains("/ColorSpace /DeviceRGB"));
    assert!(s.contains("/BitsPerComponent 8"));
    // 内容流应含 Do
    assert!(s.contains("/Im1 Do"));
    assert!(s.contains("cm"));
}

#[test]
fn multi_page_count() {
    let mut b = PdfBuilder::new();
    let p0 = b.page(PageSize::A4);
    let p1 = b.page(PageSize::A4);
    b.text(p0, &Font::helvetica(12.0), 10.0, 700.0, "First");
    b.text(p1, &Font::helvetica(12.0), 10.0, 700.0, "Second");
    let s = render(&mut b);

    assert!(s.contains("/Count 2"), "Pages Count 应为 2");
    // Kids 数组应含 2 个引用（对象 3 与 5）。
    assert!(s.contains("/Kids [3 0 R 5 0 R]"), "Kids 应引用两个 Page");
}

#[test]
fn custom_page_size_mediabox() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::Custom(200.0, 300.0));
    b.text(p, &Font::helvetica(10.0), 5.0, 5.0, "x");
    let s = render(&mut b);
    assert!(s.contains("/MediaBox [0 0 200.0 300.0]"));
}

#[test]
fn version_override_in_header() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_4);
    let p = b.page(PageSize::A4);
    b.text(p, &Font::helvetica(12.0), 10.0, 10.0, "hi");
    let s = render(&mut b);
    assert!(s.contains("%PDF-1.4"));
}

#[test]
fn named_encoding_is_emitted() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let font = Font::helvetica(12.0).with_encoding(FontEncoding::named("MacRomanEncoding"));
    b.text(p, &font, 10.0, 10.0, "hi");
    let s = render(&mut b);
    assert!(s.contains("/Encoding /MacRomanEncoding"));
    assert!(!s.contains("WinAnsiEncoding"));
}

#[test]
fn builtin_encoding_omits_encoding_entry() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let font = Font::helvetica(12.0).with_encoding(FontEncoding::BuiltIn);
    b.text(p, &font, 10.0, 10.0, "hi");
    let s = render(&mut b);
    assert!(!s.contains("/Encoding"), "BuiltIn 不应写出 /Encoding");
}

#[test]
fn custom_encoding_writes_base_and_sorted_differences() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let font = Font::helvetica(12.0).with_encoding(FontEncoding::with_differences(
        Some("WinAnsiEncoding"),
        vec![
            (66, "adieresis".to_string()),
            (65, "eacute".to_string()),
            (66, "egrave".to_string()),
            (200, "Euro".to_string()),
        ],
    ));
    b.text(p, &font, 10.0, 10.0, "AB");
    let s = render(&mut b);
    assert!(s.contains("/BaseEncoding /WinAnsiEncoding"));
    // 升序输出，且重复的 66 以后定义者为准。
    assert!(
        s.contains("/Differences [65 /eacute 66 /egrave 200 /Euro]"),
        "Differences 应按字符码升序: {s}"
    );
}

#[test]
fn custom_encoding_without_base_omits_base_encoding() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let font = Font::helvetica(12.0)
        .with_encoding(FontEncoding::with_differences(None, vec![(65, "A".into())]));
    b.text(p, &font, 10.0, 10.0, "A");
    let s = render(&mut b);
    assert!(!s.contains("/BaseEncoding"));
    assert!(s.contains("/Differences [65 /A]"));
}

#[test]
fn font_dedup_includes_encoding() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &Font::helvetica(12.0), 10.0, 10.0, "a");
    let mac = Font::helvetica(12.0).with_encoding(FontEncoding::named("MacRomanEncoding"));
    b.text(p, &mac, 10.0, 20.0, "b");
    b.text(p, &mac, 10.0, 30.0, "c");
    let s = render(&mut b);

    assert_eq!(
        s.matches("/Type /Font").count(),
        2,
        "不同编码应生成两个字体对象"
    );
    assert!(s.contains("/Encoding /WinAnsiEncoding"));
    assert!(s.contains("/Encoding /MacRomanEncoding"));
}

#[test]
fn text_encodes_winansi_non_ascii_as_hex() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &Font::helvetica(12.0), 10.0, 10.0, "Café €");
    let s = render(&mut b);
    assert!(
        s.contains("<436166E92080> Tj"),
        "WinAnsi 应按 CP1252 转码: {s}"
    );
}

#[test]
fn text_encodes_macroman_codes() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let font = Font::helvetica(12.0).with_encoding(FontEncoding::named("MacRomanEncoding"));
    b.text(p, &font, 10.0, 10.0, "é");
    let s = render(&mut b);
    assert!(s.contains("<8E> Tj"), "MacRoman 中 é 应为 0x8E: {s}");
}

#[test]
fn text_replaces_unencodable_characters() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &Font::helvetica(12.0), 10.0, 10.0, "a中b");
    let s = render(&mut b);
    // 替换后的 ? 仍是可打印 ASCII，因此走字面量输出。
    assert!(s.contains("(a?b) Tj"), "无法映射的字符应替换为 ?: {s}");
}

#[test]
fn text_uses_differences_and_skips_overridden_base_codes() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let font = Font::helvetica(12.0).with_encoding(FontEncoding::with_differences(
        Some("WinAnsiEncoding"),
        vec![(65, "eacute".to_string())],
    ));
    b.text(p, &font, 10.0, 10.0, "éA");
    let s = render(&mut b);
    // é 使用 /Differences 的 65；A 的原 65 被覆盖，退化为 ?。
    assert!(
        s.contains("(A?) Tj"),
        "Differences 应优先且屏蔽被覆盖的基码: {s}"
    );
}

#[test]
fn text_builtin_encoding_uses_standard_default() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let font = Font::helvetica(12.0).with_encoding(FontEncoding::BuiltIn);
    b.text(p, &font, 10.0, 10.0, "Aé");
    let s = render(&mut b);
    // A 在 StandardEncoding 为 65（可打印 ASCII），é 无映射。
    assert!(s.contains("(A?) Tj"), "BuiltIn 应按 Standard 编码转码: {s}");
}

#[test]
fn text_with_unknown_encoding_name_falls_back_to_utf8() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let font = Font::helvetica(12.0).with_encoding(FontEncoding::named("UnknownEncoding"));
    b.text(p, &font, 10.0, 10.0, "é");
    let s = render(&mut b);
    assert!(s.contains("(é) Tj"), "未知编码名应保留 UTF-8 直写: {s}");
}

#[test]
fn font_descriptor_is_written_when_provided() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let descriptor = FontDescriptor::new()
        .with_family("Helvetica")
        .with_ascent(718.0)
        .with_descent(-207.0)
        .with_cap_height(718.0)
        .with_stem_v(80.0)
        .with_flags(32);
    let font = Font::helvetica(12.0).with_descriptor(descriptor);
    b.text(p, &font, 10.0, 10.0, "Test");
    let s = render(&mut b);
    // 应输出 FontDescriptor 对象
    assert!(
        s.contains("/Type /FontDescriptor"),
        "应含 FontDescriptor 类型: {s}"
    );
    assert!(s.contains("/FontFamily (Helvetica)"), "应含字体族名: {s}");
    assert!(s.contains("/Ascent 718"), "应含上升高度: {s}");
    assert!(s.contains("/Descent -207"), "应含下降深度: {s}");
    assert!(s.contains("/CapHeight 718"), "应含大写字母高度: {s}");
    assert!(s.contains("/StemV 80"), "应含竖笔宽度: {s}");
    assert!(s.contains("/Flags 32"), "应含标志位: {s}");
    // 字体字典应引用描述符
    assert!(s.contains("/FontDescriptor"), "字体字典应引用描述符: {s}");
}

#[test]
fn type1_font_program_is_embedded() {
    use super::FontProgram;
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    // Type 1 字体程序数据（简化的 PostScript 格式）
    let type1_data = b"%!PS-AdobeFont-1.0: TestFont 001.000\n11 dict begin\n/TestFont def\n";
    let font = Font::helvetica(12.0).with_font_program(FontProgram::Type1(type1_data.to_vec()));
    b.text(p, &font, 10.0, 10.0, "Test");
    let s = render(&mut b);
    // 应输出 FontDescriptor 和 FontFile 流（builder 的 Owned 流按约定写明文）
    assert!(
        s.contains("/Type /FontDescriptor"),
        "应含 FontDescriptor 类型: {s}"
    );
    assert!(s.contains("/FontFile"), "应含 /FontFile 引用: {s}");
    assert!(
        s.contains("%!PS-AdobeFont-1.0: TestFont"),
        "应含 Type 1 程序明文: {s}"
    );
    assert!(
        !s.contains("/Filter /FlateDecode"),
        "builder Owned 流不应声明 FlateDecode: {s}"
    );
}

#[test]
fn truetype_font_program_is_embedded() {
    use super::FontProgram;
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    // TrueType 字体程序数据（简化的 SFNT 格式）
    let truetype_data = vec![0x00, 0x01, 0x00, 0x00, 0x00, 0x0A]; // SFNT header
    let font =
        Font::helvetica(12.0).with_font_program(FontProgram::TrueType(truetype_data.clone()));
    b.text(p, &font, 10.0, 10.0, "Test");
    let s = render(&mut b);
    // 应输出 FontDescriptor 和 FontFile2 流
    assert!(
        s.contains("/Type /FontDescriptor"),
        "应含 FontDescriptor 类型: {s}"
    );
    assert!(s.contains("/FontFile2"), "应含 /FontFile2 引用: {s}");
    assert!(
        s.contains(&format!("/Length {}", truetype_data.len())),
        "FontFile2 流长度应与程序字节一致: {s}"
    );
}

#[test]
fn cff_font_program_is_embedded() {
    use super::FontProgram;
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    // CFF 字体程序数据（简化的 CFF 格式）
    let cff_data = vec![0x01, 0x00, 0x04, 0x05]; // CFF header
    let font = Font::helvetica(12.0).with_font_program(FontProgram::CFF(cff_data.clone()));
    b.text(p, &font, 10.0, 10.0, "Test");
    let s = render(&mut b);
    // 应输出 FontDescriptor 和 FontFile3 流
    assert!(
        s.contains("/Type /FontDescriptor"),
        "应含 FontDescriptor 类型: {s}"
    );
    assert!(s.contains("/FontFile3"), "应含 /FontFile3 引用: {s}");
    assert!(s.contains("/Subtype /Type1C"), "应含 /Subtype /Type1C: {s}");
}

#[test]
fn cid_font_descriptor_is_written_when_provided() {
    use super::{CidFont, CidFontSubtype, CidSystemInfo, CidWidth};

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);

    // CIDSet：CID 0, 2, 15 存在 → 位数组 [0b10100000, 0b00000001] = [0xA0, 0x01]
    let descriptor = FontDescriptor::new()
        .with_font_name("ExampleCIDFont")
        .with_family("Example Family")
        .with_ascent(800.0)
        .with_descent(-200.0)
        .with_cap_height(700.0)
        .with_stem_v(80.0)
        .with_flags(4)
        .with_bbox([-10.0, -200.0, 1000.0, 900.0])
        .with_cid_set(vec![0xA0, 0x01]);

    let system_info = CidSystemInfo::new("Adobe", "Identity", 0);
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        system_info,
        descriptor,
        12.0,
    )
    .with_default_width(1000.0)
    .with_widths(vec![
        CidWidth::Single(65, 500.0),
        CidWidth::Range(100, 102, 600.0),
    ]);

    let cf_idx = b.cid_font(p, &cid_font);
    b.text_cid(p, cf_idx, 10.0, 10.0, &[65, 66]);
    let s = render(&mut b);

    // Type 0 父字体
    assert!(s.contains("/Subtype /Type0"), "应含 Type0 父字体: {s}");
    assert!(
        s.contains("/Encoding /Identity-H"),
        "应含 Identity-H 编码: {s}"
    );
    assert!(s.contains("/DescendantFonts"), "应含 DescendantFonts: {s}");

    // CIDFont 后代字典
    assert!(
        s.contains("/Subtype /CIDFontType2"),
        "应含 CIDFontType2: {s}"
    );
    assert!(
        s.contains("/BaseFont /ExampleCIDFont"),
        "应含 BaseFont: {s}"
    );
    assert!(s.contains("/CIDSystemInfo"), "应含 CIDSystemInfo: {s}");
    assert!(s.contains("/Registry (Adobe)"), "应含 Registry: {s}");
    assert!(s.contains("/Ordering (Identity)"), "应含 Ordering: {s}");

    // FontDescriptor
    assert!(
        s.contains("/Type /FontDescriptor"),
        "应含 FontDescriptor 类型: {s}"
    );
    assert!(
        s.contains("/FontName /ExampleCIDFont"),
        "应含 FontName: {s}"
    );
    assert!(
        s.contains("/FontFamily (Example Family)"),
        "应含 FontFamily: {s}"
    );
    assert!(s.contains("/Ascent 800"), "应含 Ascent: {s}");
    assert!(s.contains("/Descent -200"), "应含 Descent: {s}");
    assert!(s.contains("/CapHeight 700"), "应含 CapHeight: {s}");
    assert!(s.contains("/StemV 80"), "应含 StemV: {s}");
    assert!(s.contains("/Flags 4"), "应含 Flags: {s}");

    // CIDSet 流
    assert!(s.contains("/CIDSet"), "应含 CIDSet 引用: {s}");
}

#[test]
fn cid_font_type2_embeds_truetype_program() {
    use super::{CidFont, CidFontSubtype, CidSystemInfo, FontProgram};

    let program = vec![0x00, 0x01, 0x00, 0x00, 0x00, 0x0A];
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_font_program(FontProgram::TrueType(program.clone()));

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_cid(p, idx, 10.0, 10.0, &[1]);
    let s = render(&mut b);

    assert!(s.contains("/FontFile2"), "应含 /FontFile2 引用: {s}");
    assert!(
        s.contains(&format!("/Length {}", program.len())),
        "FontFile2 流长度应与程序字节一致: {s}"
    );
    assert!(!s.contains("/FontFile3"), "不应写 /FontFile3: {s}");
}

#[test]
fn cid_font_type0_embeds_cff_program_with_cidfonttype0c_subtype() {
    use super::{CidFont, CidFontSubtype, CidSystemInfo, FontProgram};

    let program = vec![0x01, 0x00, 0x04, 0x05];
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType0,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_font_program(FontProgram::CFF(program.clone()));

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_cid(p, idx, 10.0, 10.0, &[1]);
    let s = render(&mut b);

    assert!(s.contains("/FontFile3"), "应含 /FontFile3 引用: {s}");
    assert!(
        s.contains("/Subtype /CIDFontType0C"),
        "FontFile3 流应声明 /CIDFontType0C: {s}"
    );
    assert!(!s.contains("/FontFile2"), "不应写 /FontFile2: {s}");
}

#[test]
fn cid_font_rejects_mismatched_program_type() {
    use super::{CidFont, CidFontSubtype, CidSystemInfo, FontProgram};

    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType0,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_font_program(FontProgram::TrueType(vec![0x00]));

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_cid(p, idx, 10.0, 10.0, &[1]);

    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("CIDFontType0 不应接受 TrueType 程序");
    assert!(
        error.to_string().contains("CIDFontType0"),
        "错误信息应指出子类型: {error}"
    );
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

#[test]
fn subset_font_prefixes_base_font_with_six_letters() {
    use super::FontProgram;
    let program = FontProgram::TrueType(vec![0x00, 0x01, 0x00, 0x00, 0x00, 0x0A]);
    let tag = program.subset_tag();
    assert_eq!(tag.len(), 6, "子集标签应为六个字母");
    assert!(
        tag.bytes().all(|byte| byte.is_ascii_uppercase()),
        "子集标签应为大写 ASCII 字母: {tag}"
    );

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let font = Font::helvetica(12.0).with_font_program(program).subset();
    b.text(p, &font, 10.0, 10.0, "Test");
    let s = render(&mut b);

    assert!(
        s.contains(&format!("/BaseFont /{tag}+Helvetica")),
        "BaseFont 应带子集标签前缀: {s}"
    );
    assert!(
        !s.contains("/BaseFont /Helvetica"),
        "不应再写出无前缀的 BaseFont: {s}"
    );
}

#[test]
fn subset_tag_is_deterministic_and_content_dependent() {
    use super::FontProgram;
    let data = b"%!PS-AdobeFont-1.0: TestFont 001.000\n".to_vec();
    let first = FontProgram::Type1(data.clone()).subset_tag();
    let second = FontProgram::Type1(data).subset_tag();
    assert_eq!(first, second, "同一字体程序应得到同一标签");

    let other = FontProgram::Type1(b"%!PS-AdobeFont-1.0: OtherFont".to_vec()).subset_tag();
    assert_ne!(first, other, "不同字体程序应得到不同标签");
    assert_eq!(other.len(), 6);
}

#[test]
fn subset_without_font_program_keeps_base_font() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &Font::helvetica(12.0).subset(), 10.0, 10.0, "Test");
    let s = render(&mut b);

    assert!(s.contains("/BaseFont /Helvetica"), "应写原始 BaseFont: {s}");
    assert!(
        !s.contains("+Helvetica"),
        "未嵌入字体程序时不应加子集标签: {s}"
    );
}

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
fn cid_font_without_unicode_mappings_omits_to_unicode() {
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
    b.text_cid(p, idx, 10.0, 10.0, &[65]);
    let s = render(&mut b);

    assert!(
        !s.contains("/ToUnicode"),
        "未登记映射时不应写 /ToUnicode: {s}"
    );
}

#[test]
fn custom_cid_encoding_writes_embedded_cmap_with_ranges_and_chars() {
    let encoding = CidEncoding::new(0)
        .mapping(0x0020, 1)
        .mapping(0x0021, 2)
        .mapping(0x0022, 3)
        .mapping(0x0030, 10)
        .mapping(0x0032, 12);
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_encoding(encoding);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_codes_unicode(
        p,
        idx,
        10.0,
        10.0,
        &[(0x0020, "A"), (0x0021, "B"), (0x0022, "C")],
    );
    let s = render(&mut b);

    // 嵌入 CMap 的头部与书写模式。
    assert!(
        s.contains("/CIDInit /ProcSet findresource begin"),
        "应写 CMap 头: {s}"
    );
    assert!(s.contains("/CMapType 1 def"), "编码 CMap 应为 Type 1: {s}");
    assert!(
        s.contains("/CMapName /Custom-H def"),
        "水平编码应命名为 Custom-H: {s}"
    );
    assert!(s.contains("/WMode 0 def"), "水平编码 WMode 应为 0: {s}");
    assert!(
        s.contains("1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange"),
        "应写 2 字节 codespace: {s}"
    );

    // 字符码与 CID 同时连续的三项合并为 cidrange，孤立项写 cidchar。
    assert!(
        s.contains("1 begincidrange\n<0020> <0022> 1\nendcidrange"),
        "连续映射应写 cidrange: {s}"
    );
    assert!(
        s.contains("2 begincidchar\n<0030> 10\n<0032> 12\nendcidchar"),
        "非连续映射应写 cidchar: {s}"
    );
    assert!(s.contains("endcmap"), "应以 endcmap 收尾: {s}");

    // Type 0 的 /Encoding 必须引用 CMap 流，且不再写 Identity-H。
    let after_encoding = s
        .find("/Encoding ")
        .map(|pos| &s[pos + "/Encoding ".len()..])
        .expect("应写出 /Encoding");
    assert!(
        after_encoding.starts_with(|c: char| c.is_ascii_digit()),
        "自定义编码 /Encoding 应为流引用: {s}"
    );
    assert!(!s.contains("/Identity-H"), "不应再写 Identity-H: {s}");

    // 内容流按 2 字节大端序写字符码。
    assert!(
        s.contains("<002000210022> Tj"),
        "内容流应写字符码而非 CID: {s}"
    );
    // ToUnicode 的键是字符码 0x0020..0x0022，而不是 CID 1..3。
    assert!(
        s.contains("<0020> <0022> <0041>"),
        "ToUnicode 应以字符码为键: {s}"
    );
    assert!(
        !s.contains("<0001> <0003> <0041>"),
        "ToUnicode 不应以 CID 为键: {s}"
    );
}

#[test]
fn custom_cid_encoding_wmode_one_is_vertical() {
    let encoding = CidEncoding::new(1).mapping(0x41, 7);
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_encoding(encoding);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_codes(p, idx, 10.0, 10.0, &[0x41]);
    let s = render(&mut b);

    assert!(s.contains("/CMapName /Custom-V def"), "垂直编码命名: {s}");
    assert!(s.contains("/WMode 1 def"), "垂直编码 WMode 应为 1: {s}");
    assert!(
        s.contains("1 begincidchar\n<0041> 7\nendcidchar"),
        "单项映射应写 cidchar: {s}"
    );
    assert!(s.contains("<0041> Tj"), "内容流应写字符码: {s}");
}

#[test]
fn text_codes_writes_two_byte_big_endian_hex() {
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
    b.text_codes(p, idx, 10.0, 10.0, &[0x1234, 0xABCD]);
    let s = render(&mut b);

    assert!(
        s.contains("<1234ABCD> Tj"),
        "字符码应按 2 字节大端序写出: {s}"
    );
    // 未配置自定义编码时仍走 Identity-H。
    assert!(
        s.contains("/Encoding /Identity-H"),
        "默认仍为 Identity-H: {s}"
    );
}

#[test]
fn cid_encoding_rejects_invalid_wmode_and_empty_mapping() {
    for encoding in [
        CidEncoding::new(2).mapping(1, 1),
        CidEncoding::new(0),
        CidEncoding::new(1),
    ] {
        let cid_font = CidFont::new(
            "ExampleCIDFont",
            CidFontSubtype::CIDFontType2,
            CidSystemInfo::new("Adobe", "Identity", 0),
            FontDescriptor::new(),
            12.0,
        )
        .with_encoding(encoding);

        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        let idx = b.cid_font(p, &cid_font);
        b.text_codes(p, idx, 10.0, 10.0, &[1]);

        let mut out = Vec::new();
        let error = b
            .build(&mut out)
            .expect_err("非法 wmode 或空映射应使构建失败");
        assert!(
            matches!(error, WriteError::InvalidObject(_)),
            "应返回 InvalidObject: {error}"
        );
        assert!(out.is_empty(), "构建失败时不应写出任何字节");
    }
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
