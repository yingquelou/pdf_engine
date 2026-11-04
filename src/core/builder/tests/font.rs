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

/// §9.6.2.3 MMType1 实例：写出的字典条目与 Table 111 同构，实例名按规范把
/// 空格换成下划线，且读端 [`Document::mmtype1_font`] 认领同一字典。
#[test]
fn mmtype1_font_writes_table_111_instance_dict() {
    use crate::core::reader::font::SimpleFontWidths;

    let font = Font::mmtype1(12.0, "BuilderMM Bold")
        .with_widths(65, &[500.0, 600.0])
        .with_descriptor(
            FontDescriptor::new()
                .with_ascent(728.0)
                .with_descent(-222.0)
                .with_missing_width(400.0),
        );
    assert!(font.mmtype1, "Font::mmtype1 应置位 MM 标记");

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "AB");
    let doc = b.assemble_document().expect("assemble");
    let (_, dict) = find_font_object(&doc, "MMType1");

    assert_eq!(dict.get("Type").and_then(Value::as_name), Some("Font"));
    assert_eq!(
        dict.get("Subtype").and_then(Value::as_name),
        Some("MMType1"),
        "§9.6.2.3 raw 13974：/Subtype shall 为 MMType1"
    );
    assert_eq!(
        dict.get("BaseFont").and_then(Value::as_name),
        Some("BuilderMM_Bold")
    );
    assert_eq!(dict.get("FirstChar").and_then(Value::as_integer), Some(65));
    assert_eq!(dict.get("LastChar").and_then(Value::as_integer), Some(66));
    assert_eq!(
        numbers(dict.get("Widths").expect("Widths")),
        vec![500.0, 600.0]
    );
    assert_eq!(
        dict.get("Encoding").and_then(Value::as_name),
        Some("WinAnsiEncoding")
    );
    // ISO 32000-1 没有定义任何 MM 私有的字体字典条目（设计轴/`/RS`/blend 属
    // Adobe TN 5015），因此写出的键只能是 Table 111 的条目。
    let keys: Vec<&str> = dict.0.keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        [
            "BaseFont",
            "Encoding",
            "FirstChar",
            "FontDescriptor",
            "LastChar",
            "Subtype",
            "Type",
            "Widths"
        ],
        "MM 实例字典不得含 Table 111 之外的条目"
    );

    // 读端认领同一字典：类型化视图与写出的值一致，未建模条目为空。
    let value = Value::Dict(dict.clone());
    let typed = doc
        .mmtype1_font(&value)
        .expect("MM 字典应可解析")
        .expect("MM 字典应被认领");
    assert_eq!(typed.base_font, "BuilderMM_Bold");
    assert_eq!(typed.instance_name_with_spaces(), "BuilderMM Bold");
    assert_eq!(typed.first_char, 65);
    assert_eq!(typed.widths, vec![500.0, 600.0]);
    assert_eq!(typed.width_for(66), Some(600.0));
    // Table 111：范围外字符码取 /MissingWidth。
    assert_eq!(typed.width_for(20), Some(400.0));
    assert_eq!(typed.descriptor.ascent, Some(728.0));
    assert!(typed.name.is_none(), "不写已废弃的 /Name");
    assert!(!typed.has_to_unicode);
    assert!(
        typed.entries().is_empty(),
        "读端不应看到任何额外键: {:?}",
        typed.entries()
    );
    assert!(
        matches!(
            doc.simple_font_widths(&value)
                .expect("宽度入口")
                .expect("实例"),
            SimpleFontWidths::MultipleMaster(_)
        ),
        "Tf 的简单字体宽度入口应认领 MM 实例"
    );
    assert!(
        doc.type1_font(&value).expect("Type 1 入口").is_none(),
        "/Type1 入口不认领 MMType1 字典"
    );
}

/// §9.6.2.3 的命名规则用规范 EXAMPLE 2 的实例名验证：
/// `"MinionMM 366 465 11 "`（以空格结尾）写成 `/MinionMM_366_465_11_`。
#[test]
fn mmtype1_base_font_replaces_the_spec_instance_spaces() {
    let font = Font::mmtype1(12.0, "MinionMM 366 465 11 ")
        .with_widths(32, &[187.0; 4])
        .with_descriptor(FontDescriptor::new().with_missing_width(500.0));

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "AB");
    let s = render(&mut b);
    assert!(
        s.contains("/BaseFont /MinionMM_366_465_11_"),
        "空格应写成下划线（raw 13975–13977 与其 EXAMPLE 2）: {s}"
    );
    assert!(
        !s.contains("#20"),
        "/BaseFont 不得用 #20 转义空格，应使用下划线: {s}"
    );

    let doc = b.assemble_document().expect("assemble");
    let (_, dict) = find_font_object(&doc, "MMType1");
    let typed = doc
        .mmtype1_font(&Value::Dict(dict))
        .expect("parse")
        .expect("claimed");
    assert_eq!(typed.base_font, "MinionMM_366_465_11_");
    assert_eq!(typed.instance_name_with_spaces(), "MinionMM 366 465 11 ");
}

/// Table 111 的标准 14 豁免只覆盖 Type 1 字体（§9.6.2.2 raw 13953–13956）：
/// MM 实例即使取标准 14 名也必须有 `/Widths`，否则构建期拒绝且零字节。
#[test]
fn mmtype1_standard_14_name_still_requires_widths() {
    let font = Font::mmtype1(12.0, "Helvetica")
        .with_descriptor(FontDescriptor::new().with_missing_width(500.0));

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "A");

    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("MM 实例不享受标准 14 的 /Widths 豁免");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(
        error.to_string().contains("MMType1") && error.to_string().contains("/Widths"),
        "错误应指出 MMType1 缺 /Widths: {error}"
    );
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

/// Table 111 的 `/FontDescriptor` 对 MM 实例同样是必需项：缺失时构建期拒绝，
/// 不写出读端会报 `FontError::Invalid` 的字典。
#[test]
fn mmtype1_without_descriptor_fails_build() {
    let font = Font::mmtype1(12.0, "BuilderMM Bold").with_widths(65, &[500.0]);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "A");

    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("MM 实例必须有 /FontDescriptor");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(
        error.to_string().contains("/FontDescriptor"),
        "错误应指出缺 /FontDescriptor: {error}"
    );
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

/// Table 110（raw 13801–13810）把 `MMType1` 与 `Type1`/`Type3`/`TrueType`
/// 并列且无 `(PDF 1.x)` 标注 ⇒ PDF 1.0 起可用，因此不需要新的 `PdfFeature`
/// 门禁：目标版本 1.0 时 MM 实例照常写出。
#[test]
fn mmtype1_needs_no_new_feature_gate_at_pdf_1_0() {
    use crate::core::types::PdfVersion;

    let font = Font::mmtype1(12.0, "BuilderMM Bold")
        .with_widths(65, &[500.0])
        .with_descriptor(FontDescriptor::new().with_missing_width(500.0));

    let mut b = PdfBuilder::with_version(PdfVersion::V1_0);
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "A");
    let s = render(&mut b);
    assert!(s.contains("%PDF-1.0"), "头部应保持 1.0: {s}");
    assert!(s.contains("/Subtype /MMType1"), "MM 字典应写出: {s}");
}

/// Table 110 里 MMType1 与 Type1 是两种字体：同名、同编码、同宽度的 MM 实例与
/// Type 1 字体不得共用一个字体对象（跨页字体池）。
#[test]
fn mmtype1_and_type1_of_same_name_get_separate_font_objects() {
    let widths = [500.0, 600.0];
    // 与 Type 1 取同一个 `/BaseFont` 名：MM 实例名不含空格时改名规则是恒等的，
    // 因此两者的去重键除 MM 标记外完全一致，正是需要区分的场景。
    let mm = Font::mmtype1(12.0, "Helvetica")
        .with_widths(65, &widths)
        .with_descriptor(
            FontDescriptor::new()
                .with_font_name("Helvetica")
                .with_ascent(700.0)
                .with_descent(-200.0),
        );
    let type1 = Font::helvetica(12.0)
        .with_widths(65, &widths)
        .with_descriptor(FontDescriptor::new().with_font_name("Helvetica"));
    assert_eq!(mm.name, type1.name);
    assert_eq!(mm.encoding, type1.encoding);
    assert_eq!(mm.widths, type1.widths);
    assert_ne!(mm.mmtype1, type1.mmtype1);
    assert!(!mm.same_dedup_key(&type1), "MM 标记应进入字体去重键");
    // 去重键对称：池在左/候选在右与相反写法命中结果一致（调用点两种顺序都在用）。
    assert!(
        !type1.same_dedup_key(&mm),
        "same_dedup_key 应对称，参数顺序无关"
    );
    assert!(mm.same_dedup_key(&mm), "同键字体应命中");
    assert!(type1.same_dedup_key(&type1), "同键字体应对称命中");

    let mut b = PdfBuilder::new();
    let first = b.page(PageSize::A4);
    let second = b.page(PageSize::A4);
    b.text(first, &mm, 10.0, 10.0, "A");
    b.text(second, &type1, 10.0, 10.0, "A");
    let doc = b.assemble_document().expect("assemble");
    let (mm_num, mm_dict) = find_font_object(&doc, "MMType1");
    let (type1_num, _) = find_font_object(&doc, "Type1");
    assert_eq!(
        mm_dict.get("BaseFont").and_then(Value::as_name),
        Some("Helvetica")
    );
    assert_ne!(
        mm_num, type1_num,
        "MM 标记应进入字体去重键：MM 实例与 Type 1 不得共用字体对象"
    );

    let mut subtypes = Vec::new();
    for num in 1..=64 {
        if let Ok(Some(object)) = doc.get_object(num)
            && let Value::Dict(dict) = &object.value
            && dict.get("Type").and_then(Value::as_name) == Some("Font")
        {
            subtypes.push(
                dict.get("Subtype")
                    .and_then(Value::as_name)
                    .unwrap_or_default()
                    .to_owned(),
            );
        }
    }
    subtypes.sort();
    assert_eq!(
        subtypes,
        vec!["MMType1".to_owned(), "Type1".to_owned()],
        "文档应恰好含一个 MM 实例字典与一个 Type 1 字典"
    );
}

/// Table 110（raw 13801–13810）把 `Type3` 与 `MMType1` 列为两种字体类型，
/// §9.6.2.3 只把 MM 实例字典定义为 Type 1 字体字典（Table 111）的条目；
/// Type 3 字典没有 `/BaseFont`，实例命名规则无从套用，故两个标记互斥。
///
/// 公开 API 无法组合出「既是 Type 3 又是 MM」的字体（`Font::type3` 与
/// `Font::mmtype1` 互不置位），这里直接置字段只为覆盖装配层的门禁。
#[test]
fn mmtype1_mark_is_rejected_for_type3_font() {
    let type3 = Type3Font::new().glyph(Type3Glyph::new(65, "A", 500.0, b"500 0 d0"));
    let mut font = Font::type3(12.0, type3);
    font.mmtype1 = true;

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "A");

    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("Type 3 字体不得同时标记为 MMType1");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(
        error.to_string().contains("MMType1"),
        "错误应指出 MMType1 标记冲突: {error}"
    );
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}
