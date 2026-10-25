//! 简单字体编码与描述符写出测试。

use super::*;

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
fn unknown_named_encoding_fails_build() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let font = Font::helvetica(12.0).with_encoding(FontEncoding::named("UnknownEncoding"));
    b.text(p, &font, 10.0, 10.0, "é");

    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("未知编码名应构建失败");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(
        error.to_string().contains("UnknownEncoding"),
        "错误信息应指出未知编码名: {error}"
    );
    assert!(
        error.to_string().contains("/Encoding"),
        "错误信息应指出 /Encoding: {error}"
    );
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

#[test]
fn unknown_custom_base_encoding_fails_build() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let font = Font::helvetica(12.0).with_encoding(FontEncoding::with_differences(
        Some("BogusEncoding"),
        vec![(65, "A".to_string())],
    ));
    b.text(p, &font, 10.0, 10.0, "A");

    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("未知 /BaseEncoding 应构建失败");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(error.to_string().contains("BogusEncoding"), "{error}");
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

#[test]
fn known_predefined_encoding_names_build() {
    for name in [
        "StandardEncoding",
        "WinAnsiEncoding",
        "MacRomanEncoding",
        "SymbolEncoding",
        "ZapfDingbatsEncoding",
    ] {
        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        let font = Font::helvetica(12.0).with_encoding(FontEncoding::named(name));
        b.text(p, &font, 10.0, 10.0, "A");
        let s = render(&mut b);
        assert!(
            s.contains(&format!("/Encoding /{name}")),
            "应写出 /Encoding /{name}: {s}"
        );
    }
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
