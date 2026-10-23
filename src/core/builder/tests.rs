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
