use super::*;

/// 写入 → 重新解析： builder 嵌入的 Type 1 程序应作为 `/FontFile`
/// 保存原始字节，并可继续按 Type 1 格式解析。
#[test]
fn builder_embeds_type1_font_program_roundtrip() {
    use pdf_engine::core::builder::{Font, FontDescriptor, FontProgram, PageSize, PdfBuilder};

    let program = sample_type1_program();
    let font = Font::helvetica(12.0)
        .with_descriptor(FontDescriptor::new().with_font_name("Helvetica"))
        .with_font_program(FontProgram::Type1(program.clone()));

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &font, 72.0, 700.0, "Test");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let font_dict = find_font_dict(&reparsed, "Type1");
    let embedded = reparsed
        .type1_font_program(&font_dict)
        .expect("should resolve program")
        .expect("should have program");
    assert_eq!(embedded.source, FontProgramSource::FontFile);
    assert_eq!(embedded.data, program);

    let parsed = reparsed
        .type1_program(&font_dict)
        .expect("should parse program")
        .expect("should have program");
    assert!(parsed.glyph("A").expect("glyph lookup").is_some());
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("Test"));
}

/// 写入 → 重新解析： builder 按 Type 1 字形名子集化嵌入的 `/FontFile`，
/// BaseFont 带标签前缀，文本提取与子集后的程序都只保留 `.notdef`/`A`。
#[test]
fn builder_type1_subset_roundtrips_through_parser() {
    use pdf_engine::core::builder::{Font, FontEncoding, FontProgram, PageSize, PdfBuilder};

    let program = sample_type1_program();
    let font = Font::helvetica(12.0)
        .with_encoding(FontEncoding::named("WinAnsiEncoding"))
        .with_font_program(FontProgram::Type1(program.clone()))
        .with_subset_type1_glyphs(&["A"]);

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &font, 72.0, 700.0, "A");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let font_dict = find_font_dict(&reparsed, "Type1");
    let base_font = font_dict
        .as_dict()
        .and_then(|dict| dict.get("BaseFont"))
        .and_then(Value::as_name)
        .expect("BaseFont");
    let (tag, name) = base_font
        .split_once('+')
        .expect("子集 BaseFont 应带标签前缀");
    assert_eq!(tag.len(), 6, "标签应为六个字母: {tag}");
    assert_eq!(name, "Helvetica");

    let embedded = reparsed
        .type1_font_program(&font_dict)
        .expect("should resolve program")
        .expect("should have program");
    assert_eq!(embedded.source, FontProgramSource::FontFile);
    assert_ne!(embedded.data, program, "嵌入程序应被重写为子集");

    let parsed = reparsed
        .type1_program(&font_dict)
        .expect("should parse subset program")
        .expect("should have program");
    let names: Vec<&str> = parsed.charstrings.keys().map(String::as_str).collect();
    assert_eq!(names, [".notdef", "A"], "子集程序应只含 .notdef 与请求字形");
    assert!(parsed.glyph("A").expect("glyph lookup").is_some());
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("A"));
}

/// 写入 → 重新解析： builder 嵌入的 TrueType 程序应作为 `/FontFile2`
/// 保存，并让字体字典使用 `/Subtype /TrueType`。
#[test]
fn builder_embeds_truetype_font_program_roundtrip() {
    use pdf_engine::core::builder::{Font, FontProgram, PageSize, PdfBuilder};

    let sfnt = sample_sfnt_font();
    let font = Font::helvetica(12.0).with_font_program(FontProgram::TrueType(sfnt.clone()));

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &font, 72.0, 700.0, "Test");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let font_dict = find_font_dict(&reparsed, "TrueType");
    let embedded = reparsed
        .truetype_font_program(&font_dict)
        .expect("should resolve program")
        .expect("should have program");
    assert_eq!(embedded.source, FontProgramSource::FontFile2);
    assert_eq!(embedded.data, sfnt);

    let parsed = reparsed
        .truetype_font(&font_dict)
        .expect("should parse program")
        .expect("should have program");
    assert_eq!(parsed.units_per_em(), 1000);
    assert_eq!(parsed.glyph_count(), 3);
    assert_eq!(parsed.glyph_id_for_unicode(0x41).unwrap(), Some(2));
}

/// 写入 → 重新解析：TrueType-flavored OpenType 程序按 ISO 32000-1 §9.9
/// 写入 `/FontFile2`，字体保持 `/Subtype /TrueType`，文本可提取。
#[test]
fn builder_opentype_truetype_program_roundtrips_through_parser() {
    use pdf_engine::core::builder::{Font, FontProgram, PageSize, PdfBuilder};

    let sfnt = sample_sfnt_font(); // 版本标签 0x00010000
    let font = Font::helvetica(12.0).with_font_program(FontProgram::OpenType(sfnt.clone()));

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &font, 72.0, 700.0, "Test");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let font_dict = find_font_dict(&reparsed, "TrueType");
    let embedded = reparsed
        .truetype_font_program(&font_dict)
        .expect("should resolve program")
        .expect("should have program");
    assert_eq!(embedded.source, FontProgramSource::FontFile2);
    assert_eq!(embedded.data, sfnt);

    let parsed = reparsed
        .truetype_font(&font_dict)
        .expect("should parse program")
        .expect("should have program");
    assert_eq!(parsed.glyph_id_for_unicode(0x41).unwrap(), Some(2));
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("Test"));
}

/// 写入 → 重新解析：CFF-flavored OpenType 程序写到 `/FontFile3`
/// `/Subtype /OpenType`，字体子类型为 Type1，文本可提取。
#[test]
fn builder_opentype_cff_program_roundtrips_through_parser() {
    use pdf_engine::core::builder::{Font, FontEncoding, FontProgram, PageSize, PdfBuilder};

    let program = sample_opentype_cff_sfnt();
    let font = Font::helvetica(12.0)
        .with_encoding(FontEncoding::named("WinAnsiEncoding"))
        .with_font_program(FontProgram::OpenType(program.clone()));

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &font, 72.0, 700.0, "Open");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let font_dict = find_font_dict(&reparsed, "Type1");
    let embedded = reparsed
        .type1_font_program(&font_dict)
        .expect("should resolve program")
        .expect("should have program");
    assert_eq!(embedded.source, FontProgramSource::FontFile3);
    assert_eq!(embedded.subtype.as_deref(), Some("OpenType"));
    assert_eq!(embedded.data, program);
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("Open"));
}

/// 写入 → 重新解析： 子集字体应写出 `TAG+Name` 形式的 BaseFont，
/// 读端剥离标签后仍能解析嵌入程序并提取文本。
#[test]
fn subset_font_roundtrips_through_parser() {
    use pdf_engine::core::builder::{Font, FontProgram, PageSize, PdfBuilder};

    let sfnt = sample_sfnt_font();
    let font = Font::helvetica(12.0)
        .with_font_program(FontProgram::TrueType(sfnt.clone()))
        .subset();

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &font, 72.0, 700.0, "Test");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let font_dict = find_font_dict(&reparsed, "TrueType");
    let base_font = font_dict
        .as_dict()
        .and_then(|dict| dict.get("BaseFont"))
        .and_then(Value::as_name)
        .expect("BaseFont");
    let (tag, name) = base_font.split_once('+').expect("应含子集标签前缀");
    assert_eq!(tag.len(), 6, "标签应为六个字母: {tag}");
    assert!(
        tag.bytes().all(|byte| byte.is_ascii_uppercase()),
        "标签应为大写 ASCII 字母: {tag}"
    );
    assert_eq!(name, "Helvetica");
    assert_eq!(strip_subset_tag(base_font), "Helvetica");

    let parsed = reparsed
        .truetype_font(&font_dict)
        .expect("should parse program")
        .expect("should have program");
    assert_eq!(parsed.glyph_id_for_unicode(0x41).unwrap(), Some(2));
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("Test"));
}

/// 写入 → 重新解析： `with_subset_glyphs` 应把嵌入的 TrueType 程序重写为
/// 只含请求字形的子集（字形数减少、cmap 重编号），/BaseFont 仍带确定性
/// 六字母标签，且页面文本仍可提取。
#[test]
fn builder_truetype_subset_roundtrips_through_parser() {
    use pdf_engine::core::builder::{Font, FontProgram, PageSize, PdfBuilder};

    let sfnt = sample_sfnt_font();
    let font = Font::helvetica(12.0)
        .with_font_program(FontProgram::TrueType(sfnt.clone()))
        .with_subset_glyphs(&[2]);

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &font, 72.0, 700.0, "Test");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let font_dict = find_font_dict(&reparsed, "TrueType");
    let parsed = reparsed
        .truetype_font(&font_dict)
        .expect("should parse program")
        .expect("should have program");
    assert_eq!(parsed.glyph_count(), 2, "子集应只保留 gid 0 与 2");
    assert_eq!(
        parsed.glyph_id_for_unicode(0x41).expect("cmap lookup"),
        Some(1),
        "U+0041 应在子集中重编号为 gid 1"
    );

    let embedded = reparsed
        .truetype_font_program(&font_dict)
        .expect("should resolve program")
        .expect("should have program");
    assert_eq!(embedded.source, FontProgramSource::FontFile2);
    assert_ne!(embedded.data, sfnt, "嵌入程序应为重写后的子集");
    assert!(
        embedded.data.len() < sfnt.len(),
        "子集程序应短于原始程序: {} >= {}",
        embedded.data.len(),
        sfnt.len()
    );

    let base_font = font_dict
        .as_dict()
        .and_then(|dict| dict.get("BaseFont"))
        .and_then(Value::as_name)
        .expect("BaseFont");
    let (tag, name) = base_font.split_once('+').expect("应含子集标签前缀");
    assert_eq!(tag.len(), 6, "标签应为六个字母: {tag}");
    assert!(
        tag.bytes().all(|byte| byte.is_ascii_uppercase()),
        "标签应为大写 ASCII 字母: {tag}"
    );
    assert_eq!(name, "Helvetica");
    assert_eq!(strip_subset_tag(base_font), "Helvetica");

    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("Test"));
}

/// 写入 → 重新解析： builder 嵌入的 CFF 程序应作为 `/FontFile3`
/// 保存，并带 `/Subtype /Type1C`。
#[test]
fn builder_embeds_cff_font_program_roundtrip() {
    use pdf_engine::core::builder::{Font, FontProgram, PageSize, PdfBuilder};

    let cff = vec![0x01, 0x00, 0x04, 0x05];
    let font = Font::helvetica(12.0).with_font_program(FontProgram::CFF(cff.clone()));

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &font, 72.0, 700.0, "Test");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let font_dict = find_font_dict(&reparsed, "Type1");
    let embedded = reparsed
        .type1_font_program(&font_dict)
        .expect("should resolve program")
        .expect("should have program");
    assert_eq!(embedded.source, FontProgramSource::FontFile3);
    assert_eq!(embedded.subtype.as_deref(), Some("Type1C"));
    assert_eq!(embedded.data, cff);
}
