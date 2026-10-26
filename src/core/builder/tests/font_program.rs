//! 字体程序嵌入与 TrueType / Type 1 子集化测试。

use super::*;
use crate::core::reader::font::{TrueTypeFont, Type1FontProgram};
use crate::core::types::Value;

#[test]
fn type1_font_program_is_embedded() {
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
fn type1_fontfile_declares_table_127_lengths() {
    let program = sample_type1_program();
    let font = Font::helvetica(12.0).with_font_program(FontProgram::Type1(program.clone()));

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "A");
    let doc = b.assemble_document().expect("assemble");

    let (_, font_dict) = find_font_object(&doc, "Type1");
    let (stream_dict, bytes) = embedded_program_stream(&doc, &font_dict, "FontFile");
    let entry = |key: &str| {
        stream_dict
            .get(key)
            .and_then(Value::as_integer)
            .unwrap_or_else(|| panic!("missing {key}"))
    };
    let length1 = entry("Length1") as usize;
    let length2 = entry("Length2") as usize;
    let length3 = entry("Length3") as usize;

    assert_eq!(length1 + length2 + length3, bytes.len(), "三段应覆盖整个流");
    let marker = b"currentfile eexec\n";
    let expected_length1 = program
        .windows(marker.len())
        .position(|window| window == marker)
        .expect("eexec marker")
        + marker.len();
    assert_eq!(
        length1, expected_length1,
        "Length1 应覆盖明文头与 eexec 分隔符"
    );
    assert!(
        bytes[length1 + length2..]
            .iter()
            .take(512)
            .all(|&byte| byte == b'0'),
        "加密段之后应为 512 零守卫"
    );
    assert!(length3 >= 512, "尾部固定内容应包含零守卫");
}

#[test]
fn truetype_fontfile2_declares_length1_only() {
    let program = sample_truetype_program();
    let font = Font::helvetica(12.0).with_font_program(FontProgram::TrueType(program));

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "A");
    let doc = b.assemble_document().expect("assemble");

    let (_, font_dict) = find_font_object(&doc, "TrueType");
    let (stream_dict, bytes) = embedded_program_stream(&doc, &font_dict, "FontFile2");
    assert_eq!(
        stream_dict.get("Length1").and_then(Value::as_integer),
        Some(bytes.len() as i64),
        "Length1 应为 TrueType 程序字节数"
    );
    assert!(stream_dict.get("Length2").is_none());
    assert!(stream_dict.get("Length3").is_none());
}

#[test]
fn cff_fontfile3_omits_font_program_length_entries() {
    let program = vec![0x01, 0x00, 0x04, 0x05];
    let font = Font::helvetica(12.0).with_font_program(FontProgram::CFF(program.clone()));

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "A");
    let doc = b.assemble_document().expect("assemble");

    let (_, font_dict) = find_font_object(&doc, "Type1");
    let (stream_dict, bytes) = embedded_program_stream(&doc, &font_dict, "FontFile3");
    assert_eq!(bytes, program);
    for key in ["Length1", "Length2", "Length3"] {
        assert!(
            stream_dict.get(key).is_none(),
            "FontFile3 不应写 {key}: {stream_dict:?}"
        );
    }
}

#[test]
fn subset_font_prefixes_base_font_with_six_letters() {
    let program = FontProgram::TrueType(vec![0x00, 0x01, 0x00, 0x00, 0x00, 0x0A]);
    let tag = program.subset_tag_for_name("Helvetica", &[]);
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
    let data = b"%!PS-AdobeFont-1.0: TestFont 001.000\n".to_vec();
    let first = FontProgram::Type1(data.clone()).subset_tag_with(&[]);
    let second = FontProgram::Type1(data).subset_tag_with(&[]);
    assert_eq!(first, second, "同一字体程序应得到同一标签");

    let other = FontProgram::Type1(b"%!PS-AdobeFont-1.0: OtherFont".to_vec()).subset_tag_with(&[]);
    assert_ne!(first, other, "不同字体程序应得到不同标签");
    assert_eq!(other.len(), 6);
}

#[test]
fn subset_tag_with_discriminator_separates_different_subsets() {
    let program = FontProgram::TrueType(vec![0x00, 0x01, 0x00, 0x00]);
    let first = program.subset_tag_with(&[2, 0]);
    let same = program.subset_tag_with(&[2, 0]);
    let other = program.subset_tag_with(&[5, 0]);

    assert_eq!(first, same, "同一程序与同一子集应得到同一标签");
    assert_ne!(first, other, "同一程序的不同子集应得到不同标签");
    assert_ne!(
        first,
        program.subset_tag_with(&[]),
        "带子集判别与未带判别应得到不同标签"
    );
    assert_eq!(first.len(), 6);
}

#[test]
fn subset_tag_includes_font_name() {
    let program = FontProgram::TrueType(vec![0x00, 0x01, 0x00, 0x00]);
    let same_name = program.subset_tag_for_name("Helvetica", &[1, 0, 2]);
    assert_eq!(
        same_name,
        program.subset_tag_for_name("Helvetica", &[1, 0, 2]),
        "同名字与同子集应得到同一标签"
    );
    assert_ne!(
        same_name,
        program.subset_tag_for_name("BuilderSans", &[1, 0, 2]),
        "同一程序与子集在不同 BaseFont 名下应得到不同标签"
    );
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
fn subset_glyphs_rewrite_truetype_fontfile2() {
    let program = sample_truetype_program();
    let font = Font::helvetica(12.0)
        .with_font_program(FontProgram::TrueType(program.clone()))
        .with_subset_glyphs(&[2]);
    assert!(font.subset, "with_subset_glyphs 应同时打开子集标记");

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "Test");
    let doc = b.assemble_document().expect("assemble");

    let (_, font_dict) = find_font_object(&doc, "TrueType");
    let (declared_length, bytes) = embedded_program(&doc, &font_dict, "FontFile2");
    let (stream_dict, _) = embedded_program_stream(&doc, &font_dict, "FontFile2");
    assert_eq!(
        stream_dict.get("Length1").and_then(Value::as_integer),
        Some(bytes.len() as i64),
        "子集程序的 Length1 应与重写后字节一致"
    );
    assert_ne!(bytes, program, "嵌入程序应被重写为子集");
    assert!(
        bytes.len() < program.len(),
        "子集程序应更短: {} >= {}",
        bytes.len(),
        program.len()
    );
    assert_eq!(
        declared_length,
        bytes.len() as i64,
        "/Length 应与子集字节一致"
    );

    let parsed = TrueTypeFont::parse(bytes).expect("子集程序应可解析");
    assert_eq!(parsed.glyph_count(), 2, "应只保留 gid 0 与 2");
    assert_eq!(
        parsed.glyph_id_for_unicode(0x41).expect("cmap lookup"),
        Some(1),
        "U+0041 应重编号为 gid 1"
    );

    // BaseFont 前缀仍由原始程序决定（确定性），与单独计算一致。
    let base_font = font_dict
        .get("BaseFont")
        .and_then(Value::as_name)
        .expect("BaseFont");
    let expected = format!(
        "{}+Helvetica",
        FontProgram::TrueType(program).subset_tag_for_name("Helvetica", &[1, 0, 2])
    );
    assert_eq!(base_font, expected);
}

#[test]
fn subset_glyphs_reject_non_truetype_programs() {
    for (program, kind) in [
        (
            FontProgram::Type1(b"%!PS-AdobeFont-1.0: TestFont".to_vec()),
            "Type1",
        ),
        (FontProgram::CFF(vec![0x01, 0x00, 0x04, 0x05]), "CFF"),
    ] {
        let font = Font::helvetica(12.0)
            .with_font_program(program)
            .with_subset_glyphs(&[1]);

        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        b.text(p, &font, 10.0, 10.0, "Test");

        let mut out = Vec::new();
        let error = b
            .build(&mut out)
            .expect_err("只有 TrueType 程序可以按字形子集化");
        assert!(
            matches!(&error, WriteError::InvalidObject(_)),
            "错误应为 InvalidObject: {error}"
        );
        assert!(
            error.to_string().contains(kind) && error.to_string().contains("subset_glyphs"),
            "错误信息应说明所需程序类型: {error}"
        );
        assert!(out.is_empty(), "构建失败时不应写出任何字节");
    }
}

#[test]
fn subset_glyphs_without_font_program_is_invalid() {
    let font = Font::helvetica(12.0).with_subset_glyphs(&[1]);
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "Test");

    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("缺少字体程序时不能子集化");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(error.to_string().contains("subset_glyphs"), "{error}");
    assert!(out.is_empty());
}

#[test]
fn subset_glyphs_invalid_glyph_id_is_invalid_object() {
    let font = Font::helvetica(12.0)
        .with_font_program(FontProgram::TrueType(sample_truetype_program()))
        .with_subset_glyphs(&[99]);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "Test");

    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("越界 gid 应被拒绝");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(
        error.to_string().contains("TrueType subsetting"),
        "错误信息应带子集化上下文: {error}"
    );
    assert!(out.is_empty());
}

#[test]
fn subset_glyph_lists_do_not_deduplicate_fonts() {
    let program = FontProgram::TrueType(sample_truetype_program());
    let wide = Font::helvetica(12.0)
        .with_font_program(program.clone())
        .with_subset_glyphs(&[0, 2]);
    let narrow = Font::helvetica(12.0)
        .with_font_program(program)
        .with_subset_glyphs(&[2]);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &wide, 10.0, 10.0, "A");
    b.text(p, &narrow, 10.0, 20.0, "A");
    let doc = b.assemble_document().expect("assemble");

    let font_count = (1..=64)
        .filter(|num| {
            doc.get_object(*num).ok().flatten().is_some_and(|object| {
                object
                    .value
                    .as_dict()
                    .and_then(|dict| dict.get("Type"))
                    .and_then(Value::as_name)
                    == Some("Font")
            })
        })
        .count();
    assert_eq!(font_count, 2, "不同字形列表应生成两个字体对象");

    let base_fonts: Vec<String> = (1..=64)
        .filter_map(|num| doc.get_object(num).ok().flatten())
        .filter_map(|object| {
            let dict = object.value.as_dict()?;
            if dict.get("Type").and_then(Value::as_name) != Some("Font") {
                return None;
            }
            dict.get("BaseFont")
                .and_then(Value::as_name)
                .map(str::to_owned)
        })
        .collect();
    assert_eq!(base_fonts.len(), 2);
    assert_ne!(
        base_fonts[0], base_fonts[1],
        "不同子集不应共享同一 /BaseFont 标签"
    );
}

#[test]
fn type1_subset_glyphs_rewrite_fontfile() {
    let program = sample_type1_program();
    let original = Type1FontProgram::parse(&program).expect("fixture should parse");
    let font = Font::helvetica(12.0)
        .with_font_program(FontProgram::Type1(program.clone()))
        .with_subset_type1_glyphs(&["A"]);
    assert!(font.subset, "with_subset_type1_glyphs 应同时打开子集标记");

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "A");
    let doc = b.assemble_document().expect("assemble");

    let (_, font_dict) = find_font_object(&doc, "Type1");
    let (declared_length, bytes) = embedded_program(&doc, &font_dict, "FontFile");
    let (stream_dict, _) = embedded_program_stream(&doc, &font_dict, "FontFile");
    let length_sum: i64 = ["Length1", "Length2", "Length3"]
        .iter()
        .map(|key| {
            stream_dict
                .get(key)
                .and_then(Value::as_integer)
                .unwrap_or_else(|| panic!("missing {key}"))
        })
        .sum();
    assert_eq!(
        length_sum,
        bytes.len() as i64,
        "子集程序的 Length1/2/3 应覆盖整个流"
    );
    assert_ne!(bytes, program, "嵌入程序应被重写为子集");
    assert!(
        bytes.len() < program.len(),
        "子集程序应更短: {} >= {}",
        bytes.len(),
        program.len()
    );
    assert_eq!(
        declared_length,
        bytes.len() as i64,
        "/Length 应与子集字节一致"
    );

    let parsed = doc
        .type1_program(&Value::Dict(font_dict.clone()))
        .expect("子集程序应可解析")
        .expect("应含 FontFile 程序");
    let names: Vec<&str> = parsed.charstrings.keys().map(String::as_str).collect();
    assert_eq!(names, [".notdef", "A"], "应只保留 .notdef 与请求字形");
    for name in [".notdef", "A"] {
        assert_eq!(
            parsed.charstrings.get(name),
            original.charstrings.get(name),
            "保留字形 {name} 的 CharString 不应改变"
        );
    }

    // BaseFont 前缀仍由原始程序决定（确定性），与单独计算一致。
    let base_font = font_dict
        .get("BaseFont")
        .and_then(Value::as_name)
        .expect("BaseFont");
    let expected = format!(
        "{}+Helvetica",
        FontProgram::Type1(program).subset_tag_for_name("Helvetica", &[2, 0, 0, 0, 1, b'A'])
    );
    assert_eq!(base_font, expected);
}

#[test]
fn type1_subset_glyphs_without_font_program_is_invalid() {
    let font = Font::helvetica(12.0).with_subset_type1_glyphs(&["A"]);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "A");

    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("缺少字体程序时不能按 Type 1 字形子集化");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(
        error
            .to_string()
            .contains("requires an embedded Type 1 font program"),
        "{error}"
    );
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

#[test]
fn type1_subset_glyphs_reject_non_type1_programs() {
    let font = Font::helvetica(12.0)
        .with_font_program(FontProgram::TrueType(sample_truetype_program()))
        .with_subset_type1_glyphs(&["A"]);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "A");

    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("TrueType 程序不能按 Type 1 字形名子集化");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(
        error
            .to_string()
            .contains("requires an embedded Type 1 font program"),
        "{error}"
    );
    assert!(error.to_string().contains("TrueType"), "{error}");
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

#[test]
fn type1_subset_glyphs_unknown_glyph_is_invalid_object() {
    let font = Font::helvetica(12.0)
        .with_font_program(FontProgram::Type1(sample_type1_program()))
        .with_subset_type1_glyphs(&["Z"]);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "A");

    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("未知字形名应被拒绝");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(
        error.to_string().contains("Type 1 subsetting"),
        "错误信息应带子集化上下文: {error}"
    );
    assert!(error.to_string().contains('Z'), "{error}");
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

#[test]
fn subset_glyphs_and_type1_subset_glyphs_cannot_be_combined() {
    let font = Font::helvetica(12.0)
        .with_font_program(FontProgram::Type1(sample_type1_program()))
        .with_subset_glyphs(&[1])
        .with_subset_type1_glyphs(&["A"]);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "A");

    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("两种子集字形列表不能同时使用");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(error.to_string().contains("cannot be combined"), "{error}");
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

#[test]
fn opentype_cff_program_writes_fontfile3_with_opentype_subtype() {
    let program = sample_opentype_sfnt(b"OTTO", b"sample-cff-table");
    let font = Font::helvetica(12.0).with_font_program(FontProgram::OpenType(program.clone()));

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "A");
    let doc = b.assemble_document().expect("assemble");

    // CFF-flavored OpenType 的字体子类型为 Type1（ISO 32000-1 §9.6.2）。
    let (_, font_dict) = find_font_object(&doc, "Type1");
    let (stream_dict, bytes) = embedded_program_stream(&doc, &font_dict, "FontFile3");
    assert_eq!(
        stream_dict.get("Subtype").and_then(Value::as_name),
        Some("OpenType"),
        "FontFile3 流应声明 /Subtype /OpenType"
    );
    assert_eq!(bytes, program, "嵌入程序应与输入字节一致");
    for key in ["Length1", "Length2", "Length3"] {
        assert!(
            stream_dict.get(key).is_none(),
            "FontFile3 不应写 {key}: {stream_dict:?}"
        );
    }
}

#[test]
fn opentype_truetype_program_writes_fontfile2_with_length1() {
    let program = sample_truetype_program(); // 版本标签 0x00010000
    let font = Font::helvetica(12.0).with_font_program(FontProgram::OpenType(program.clone()));

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "A");
    let doc = b.assemble_document().expect("assemble");

    let (_, font_dict) = find_font_object(&doc, "TrueType");
    let (stream_dict, bytes) = embedded_program_stream(&doc, &font_dict, "FontFile2");
    assert_eq!(bytes, program);
    assert_eq!(
        stream_dict.get("Length1").and_then(Value::as_integer),
        Some(bytes.len() as i64),
        "FontFile2 应写 Table 127 的 Length1"
    );
    assert!(stream_dict.get("Length2").is_none());
    assert!(stream_dict.get("Length3").is_none());
}

#[test]
fn opentype_program_accepts_apple_true_flavor() {
    let program = sample_opentype_sfnt(b"true", b"sample-cff-table");
    let font = Font::helvetica(12.0).with_font_program(FontProgram::OpenType(program.clone()));

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "A");
    let doc = b.assemble_document().expect("assemble");

    let (_, font_dict) = find_font_object(&doc, "TrueType");
    let (_, bytes) = embedded_program_stream(&doc, &font_dict, "FontFile2");
    assert_eq!(bytes, program);
}

#[test]
fn opentype_program_rejects_unsupported_sfnt_flavor() {
    for program in [
        sample_opentype_sfnt(b"OTF ", b"payload"),
        sample_opentype_sfnt(b"typ1", b"payload"),
        vec![0x00, 0x01, 0x00],
    ] {
        let font = Font::helvetica(12.0).with_font_program(FontProgram::OpenType(program));

        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        b.text(p, &font, 10.0, 10.0, "A");

        let mut out = Vec::new();
        let error = b.build(&mut out).expect_err("非法 SFNT 版本标签应被拒绝");
        assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
        assert!(
            error.to_string().contains("unsupported SFNT flavor"),
            "{error}"
        );
        assert!(out.is_empty(), "构建失败时不应写出任何字节");
    }
}

#[test]
fn opentype_program_rejects_subset_requests() {
    let program = sample_opentype_sfnt(b"OTTO", b"sample-cff-table");
    let cases = [
        (
            Font::helvetica(12.0)
                .with_font_program(FontProgram::OpenType(program.clone()))
                .with_subset_glyphs(&[1]),
            "subset_glyphs",
        ),
        (
            Font::helvetica(12.0)
                .with_font_program(FontProgram::OpenType(program.clone()))
                .with_subset_type1_glyphs(&["A"]),
            "subset_type1_glyphs",
        ),
        (
            Font::helvetica(12.0)
                .with_font_program(FontProgram::OpenType(program.clone()))
                .with_subset_cff_glyphs(&["A"]),
            "subset_cff_glyphs",
        ),
    ];
    for (font, expected) in cases {
        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        b.text(p, &font, 10.0, 10.0, "A");

        let mut out = Vec::new();
        let error = b.build(&mut out).expect_err("OpenType 程序不支持子集化");
        assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
        assert!(error.to_string().contains(expected), "{error}");
        assert!(error.to_string().contains("OpenType"), "{error}");
        assert!(out.is_empty(), "构建失败时不应写出任何字节");
    }
}

#[test]
fn type1_subset_glyph_lists_do_not_deduplicate_fonts() {
    let program = FontProgram::Type1(sample_type1_program());
    let wide = Font::helvetica(12.0)
        .with_font_program(program.clone())
        .with_subset_type1_glyphs(&["A", "B"]);
    let narrow = Font::helvetica(12.0)
        .with_font_program(program)
        .with_subset_type1_glyphs(&["A"]);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &wide, 10.0, 10.0, "AB");
    b.text(p, &narrow, 10.0, 20.0, "A");
    let doc = b.assemble_document().expect("assemble");

    let font_count = (1..=64)
        .filter(|num| {
            doc.get_object(*num).ok().flatten().is_some_and(|object| {
                object
                    .value
                    .as_dict()
                    .and_then(|dict| dict.get("Type"))
                    .and_then(Value::as_name)
                    == Some("Font")
            })
        })
        .count();
    assert_eq!(font_count, 2, "不同 Type 1 字形列表应生成两个字体对象");
}
