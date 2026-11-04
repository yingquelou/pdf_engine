//! Multiple Master（MMType1）实例字典的写端集成往返：
//! `PdfBuilder::build` 写出 → 重新解析 → 用读端类型化 API 断言（PDF 32000-1 §9.6.2.3）。
//!
//! 单元测试（`src/core/builder/tests/font.rs` 与
//! `src/core/builder/tests/font_program/program_embedding.rs`）覆盖装配层的中间
//! 结构；本主题文件断言真实产出的 PDF 字节与重新解析后的文档对象。

use super::*;
use pdf_engine::core::builder::{Font, FontDescriptor, FontProgram, PageSize, PdfBuilder};
use pdf_engine::core::writer::WriteError;

/// 收集文档里全部字体字典的（对象号, `/Subtype`），用于断言「两个不同字体对象」。
fn mm_font_objects(doc: &Document) -> Vec<(i64, String)> {
    let mut found = Vec::new();
    for num in 1..=64 {
        if let Ok(Some(object)) = doc.get_object(num)
            && let Value::Dict(dict) = &object.value
            && dict.get("Type").and_then(Value::as_name) == Some("Font")
        {
            found.push((
                num,
                dict.get("Subtype")
                    .and_then(Value::as_name)
                    .unwrap_or_default()
                    .to_owned(),
            ));
        }
    }
    found.sort();
    found
}

/// 规范 §9.6.2.3 EXAMPLE 2（raw 13977、13981）的实例名：
/// `"MinionMM 366 465 11 "` 写成 `/BaseFont /MinionMM_366_465_11_`。
fn spec_instance_font() -> Font {
    Font::mmtype1(12.0, "MinionMM 366 465 11 ")
        .with_widths(65, &[500.0, 600.0])
        .with_descriptor(
            FontDescriptor::new()
                .with_font_name("MinionMM_366_465_11_")
                .with_missing_width(400.0),
        )
}

/// 写入 → 重新解析：MM 实例经 `PdfBuilder::build` 落盘后，读端
/// `Document::mmtype1_font` 认领该字典，`/Subtype` 为 `MMType1`，`/BaseFont`
/// 的下划线形式与实例名的空格形式互逆。
#[test]
fn builder_mmtype1_instance_roundtrips_as_typed_mm_font() {
    let font = spec_instance_font();
    assert!(font.mmtype1, "Font::mmtype1 应置位 MM 标记");

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &font, 72.0, 700.0, "AB");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let text = String::from_utf8_lossy(&buf);
    assert!(
        text.contains("/Subtype /MMType1"),
        "产出的字节应含 MMType1 /Subtype: {text}"
    );
    assert!(
        text.contains("/BaseFont /MinionMM_366_465_11_"),
        "实例名的空格（20h）应写成 LOW LINE（5Fh）: {text}"
    );
    assert!(
        !text.contains("#20"),
        "/BaseFont 不得用 #20 转义空格，规范用下划线: {text}"
    );

    let reparsed = parse_buf(&buf);
    let font_dict = find_font_dict(&reparsed, "MMType1");
    let typed = reparsed
        .mmtype1_font(&font_dict)
        .expect("MM 字典应可解析")
        .expect("MMType1 字典应被读端认领");
    assert_eq!(typed.base_font, "MinionMM_366_465_11_");
    assert_eq!(typed.instance_name_with_spaces(), "MinionMM 366 465 11 ");
    assert_eq!(typed.first_char, 65);
    assert_eq!(typed.widths, vec![500.0, 600.0]);
    assert_eq!(typed.width_for(65), Some(500.0));
    assert_eq!(typed.width_for(66), Some(600.0));
    // Table 111：/Widths 覆盖范围外的字符码取描述符的 /MissingWidth。
    assert_eq!(typed.width_for(20), Some(400.0));
    assert_eq!(typed.descriptor.missing_width, Some(400.0));
    assert!(typed.name.is_none(), "不写已废弃的 /Name");
    assert!(
        typed.entries().is_empty(),
        "MM 实例字典不得含 Table 111 之外的条目（设计轴/`/RS`/blend 属 Adobe \
         TN 5015，ISO 32000-1 未定义）: {:?}",
        typed.entries()
    );

    // 文本提取经 simple_font_widths → mmtype1_font 取到实例宽度。
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("AB"));
}

/// 写入 → 重新解析：MM 实例的 Type 1 快照按 Table 126（raw 15529–15531）写成
/// `/FontFile`，读端 `mmtype1_font_program` 与 `mmtype1_type1_program` 认领
/// 原样字节；`mmtype1_type1c_program` 不误认领。
#[test]
fn builder_mmtype1_type1_snapshot_roundtrips_as_fontfile() {
    let program = sample_type1_program();
    let font = spec_instance_font().with_font_program(FontProgram::Type1(program.clone()));

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &font, 72.0, 700.0, "AB");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);
    let font_dict = find_font_dict(&reparsed, "MMType1");

    let embedded = reparsed
        .mmtype1_font_program(&font_dict)
        .expect("should resolve program")
        .expect("MM 字典应带 /FontFile");
    assert_eq!(embedded.source, FontProgramSource::FontFile);
    assert_eq!(embedded.data, program, "快照字节应原样嵌入");

    let snapshot = reparsed
        .mmtype1_type1_program(&font_dict)
        .expect("Type 1 快照应可解析")
        .expect("FontFile 快照应被认领");
    assert!(
        snapshot.glyph("A").expect("glyph lookup").is_some(),
        "快照须保留 Type 1 字形（raw 14000–14002 要求嵌入普通 Type 1 程序）"
    );
    assert!(
        reparsed
            .mmtype1_type1c_program(&font_dict)
            .expect("Type1C 入口")
            .is_none(),
        "/FontFile 快照不是 CFF 程序"
    );
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("AB"));
}

/// 写入 → 重新解析：Table 126（raw 15536–15540）为 MMType1 列出
/// `/FontFile3 /Subtype /Type1C`，产出的流字典确实带该 `/Subtype`。
#[test]
fn builder_mmtype1_cff_snapshot_roundtrips_as_fontfile3_type1c() {
    let cff = vec![0x01, 0x00, 0x04, 0x05];
    let font = spec_instance_font().with_font_program(FontProgram::CFF(cff.clone()));

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &font, 72.0, 700.0, "AB");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let text = String::from_utf8_lossy(&buf);
    assert!(
        text.contains("/Subtype /Type1C"),
        "/FontFile3 流字典应带 /Subtype /Type1C: {text}"
    );

    let reparsed = parse_buf(&buf);
    let font_dict = find_font_dict(&reparsed, "MMType1");
    let embedded = reparsed
        .mmtype1_font_program(&font_dict)
        .expect("should resolve program")
        .expect("MM 字典应带 /FontFile3");
    assert_eq!(embedded.source, FontProgramSource::FontFile3);
    assert_eq!(
        embedded.subtype.as_deref(),
        Some("Type1C"),
        "MM 的 CFF 快照须写成 /FontFile3 /Subtype /Type1C"
    );
    assert_eq!(embedded.data, cff);

    let wrong_entry = reparsed
        .mmtype1_type1_program(&font_dict)
        .expect_err("/FontFile3 快照不是 Type 1 程序");
    assert!(
        wrong_entry.to_string().contains("/FontFile"),
        "错用 /FontFile 入口应给出类型化错误: {wrong_entry}"
    );
}

/// Table 111 的 `/Widths` 对 MM 实例必需（标准 14 豁免只覆盖 Type 1 字体，
/// §9.6.2.2 raw 13953–13956）：缺 `/Widths` 时构建期报 `WriteError::InvalidObject`
/// 且**零字节写出**。
#[test]
fn builder_mmtype1_without_widths_fails_with_zero_bytes() {
    let font = Font::mmtype1(12.0, "BuilderMM Bold").with_descriptor(
        FontDescriptor::new()
            .with_font_name("BuilderMM_Bold")
            .with_missing_width(500.0),
    );

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &font, 72.0, 700.0, "A");

    let mut buf = Vec::new();
    let error = builder
        .build(&mut buf)
        .expect_err("MM 实例不享受标准 14 的 /Widths 豁免");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(
        error.to_string().contains("/Widths"),
        "错误应指向缺 /Widths: {error}"
    );
    assert!(buf.is_empty(), "构建失败时不应写出任何字节");

    // 取标准 14 名同样不豁免：§9.6.2.2 的名单只列 Type 1 字体。
    let named = Font::mmtype1(12.0, "Helvetica").with_descriptor(
        FontDescriptor::new()
            .with_font_name("Helvetica")
            .with_missing_width(500.0),
    );
    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &named, 72.0, 700.0, "A");
    let mut buf = Vec::new();
    let error = builder
        .build(&mut buf)
        .expect_err("MM 实例即使取标准 14 名也须给 /Widths");
    assert!(
        error.to_string().contains("MMType1") && error.to_string().contains("/Widths"),
        "错误应同时指出 MMType1 与被拒的 /Widths 缺失: {error}"
    );
    assert!(buf.is_empty(), "构建失败时不应写出任何字节");
}

/// Table 111 的 `/FontDescriptor` 对 MM 实例必需：既无描述符也无嵌入快照程序时
/// 构建期报错且零字节写出，不产出读端会报 `FontError::Invalid` 的文件。
#[test]
fn builder_mmtype1_without_descriptor_fails_with_zero_bytes() {
    let font = Font::mmtype1(12.0, "BuilderMM Bold").with_widths(65, &[500.0]);

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &font, 72.0, 700.0, "A");

    let mut buf = Vec::new();
    let error = builder
        .build(&mut buf)
        .expect_err("MM 实例必须有 /FontDescriptor");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(
        error.to_string().contains("/FontDescriptor"),
        "错误应指向缺 /FontDescriptor: {error}"
    );
    assert!(buf.is_empty(), "构建失败时不应写出任何字节");
}

/// Table 126 只允许 MMType1 用 `/FontFile`（raw 15529–15531）与
/// `/FontFile3 /Subtype /Type1C`（raw 15536–15540）：`/FontFile2` 只列
/// TrueType/CIDFontType2（raw 15532–15535），`/FontFile3 /OpenType` 续表只列
/// Type1/CIDFontType0（raw 15569–15572），故 TrueType 与 OpenType 快照都构建期
/// 拒绝且零字节写出。
#[test]
fn builder_mmtype1_rejects_truetype_and_opentype_snapshots_with_zero_bytes() {
    for (label, phrase) in [
        ("TrueType", "cannot embed a TrueType program"),
        ("OpenType", "cannot embed an OpenType program"),
    ] {
        let program = match label {
            "TrueType" => FontProgram::TrueType(sample_sfnt_font()),
            _ => FontProgram::OpenType(sample_opentype_cff_sfnt()),
        };
        let font = spec_instance_font().with_font_program(program);
        let mut builder = PdfBuilder::new();
        let page = builder.page(PageSize::A4);
        builder.text(page, &font, 72.0, 700.0, "A");

        let mut buf = Vec::new();
        let error = builder
            .build(&mut buf)
            .expect_err("MM 实例不得嵌入 {label} 程序");
        assert!(
            matches!(&error, WriteError::InvalidObject(_)),
            "{label}: {error}"
        );
        assert!(
            error.to_string().contains("MMType1") && error.to_string().contains(phrase),
            "{label}: 错误应同时指出 MMType1 与被拒的程序格式: {error}"
        );
        assert!(buf.is_empty(), "{label} 快照被拒时不应写出任何字节");
    }
}

/// Table 110 里 MMType1 与 Type1 是两种字体：同名、同字号、同编码、同宽度的
/// MM 实例与 Type 1 字体在**同一页**上同时使用时，页内 `/F<n>` 资源与跨页字体
/// 池都不得合并它们，因此产出两个不同对象号的字典。
#[test]
fn builder_mmtype1_and_same_name_type1_get_two_font_objects_on_one_page() {
    let widths = [500.0, 600.0];
    // MM 实例名不含空格时改名规则是恒等的：两者的去重键除 MM 标记外完全一致，
    // 正是需要区分的场景。
    let mm = Font::mmtype1(12.0, "Helvetica")
        .with_widths(65, &widths)
        .with_descriptor(
            FontDescriptor::new()
                .with_font_name("Helvetica")
                .with_ascent(728.0)
                .with_descent(-222.0),
        );
    let type1 = Font::helvetica(12.0)
        .with_widths(65, &widths)
        .with_descriptor(FontDescriptor::new().with_font_name("Helvetica"));
    assert_eq!(mm.name, type1.name);
    assert_eq!(mm.size, type1.size);
    assert_eq!(mm.encoding, type1.encoding);
    assert_eq!(mm.widths, type1.widths);

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &mm, 72.0, 700.0, "A");
    builder.text(page, &type1, 72.0, 680.0, "B");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let objects = mm_font_objects(&reparsed);
    let mm_num = objects
        .iter()
        .find(|(_, subtype)| subtype == "MMType1")
        .expect("文档应恰有一个 MMType1 字典")
        .0;
    let type1_num = objects
        .iter()
        .find(|(_, subtype)| subtype == "Type1")
        .expect("文档应恰有一个 Type1 字典")
        .0;
    assert_eq!(
        objects.len(),
        2,
        "MM 标记须进入页内资源与字体池的去重键，不得合并: {objects:?}"
    );
    assert_ne!(mm_num, type1_num, "两个字体类型须各占一个对象号");

    // 页内 `/Font` 资源字典给出两个资源名，分别指向上述两个对象。
    let page_dict = first_page_dict(&reparsed);
    let resources = page_dict
        .get("Resources")
        .and_then(Value::as_dict)
        .expect("/Resources");
    let fonts = resources
        .get("Font")
        .and_then(Value::as_dict)
        .expect("/Font 资源字典");
    let referenced: Vec<i64> = fonts
        .0
        .values()
        .filter_map(|value| match value {
            Value::Ref(num, _) => Some(*num),
            _ => None,
        })
        .collect();
    assert_eq!(
        referenced.len(),
        2,
        "同页应有两个字体资源条目: {:?}",
        fonts.0.keys()
    );
    assert!(
        referenced.contains(&mm_num) && referenced.contains(&type1_num),
        "两个资源名应分别指向 MM 字典与 Type 1 字典: {referenced:?}"
    );

    // 两个字典都能被各自的读端类型化入口认领，且文本提取照常工作。
    let mm_dict = find_font_dict(&reparsed, "MMType1");
    let typed_mm = reparsed
        .mmtype1_font(&mm_dict)
        .expect("MM 解析")
        .expect("MM 认领");
    assert_eq!(typed_mm.base_font, "Helvetica");
    assert_eq!(typed_mm.descriptor.ascent, Some(728.0));
    assert_eq!(typed_mm.width_for(66), Some(600.0));
    assert!(
        reparsed
            .type1_font(&mm_dict)
            .expect("Type 1 入口")
            .is_none(),
        "/Type1 入口不认领 MMType1 字典"
    );
    let type1_dict = find_font_dict(&reparsed, "Type1");
    let typed_type1 = reparsed
        .type1_font(&type1_dict)
        .expect("Type 1 解析")
        .expect("Type 1 认领");
    assert_eq!(typed_type1.base_font, "Helvetica");
    let type1_descriptor = typed_type1.descriptor.expect("Type 1 描述符");
    assert_eq!(
        type1_descriptor.ascent,
        Some(800.0),
        "Type 1 侧未显式给出的 Table 122 必需项按默认值补齐"
    );
    assert_ne!(
        typed_mm.descriptor.ascent, type1_descriptor.ascent,
        "MM 实例与 Type 1 字体各写各的描述符对象，未共用"
    );
    assert!(
        reparsed
            .mmtype1_font(&type1_dict)
            .expect("MM 入口")
            .is_none(),
        "/MMType1 入口不认领 Type 1 字典"
    );
    // 两段文字各成一个 BT/ET 块，因此提取结果按行分隔：证明 `/F1`（MM 实例）
    // 与 `/F2`（Type 1 字体）都成功解析并渲染。
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("A\nB"));
}
