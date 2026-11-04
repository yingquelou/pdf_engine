//! 字体程序嵌入写出与 Table 127 长度条目：`/FontFile`（Type 1 三段长度覆盖
//! 整流、`Length1` 到 eexec 分隔符、加密段后的 512 零守卫）、`/FontFile2`
//! （TrueType 仅写 `/Length1`）与 `/FontFile3`（CFF 不写任何程序长度条目）。

use super::super::*;
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

/// §9.6.2.3（raw 14000–14002）要求嵌入的 MM 实例程序是「an ordinary Type 1 font
/// program, not a multiple master font program」，Table 126（raw 15529–15531）
/// 允许 MMType1 用 `/FontFile`：快照按既有 Type 1 通路写成 `/FontFile`，
/// Table 127 的 `/Length1/2/3`（raw 15569 起）同样沿用，读端
/// `mmtype1_type1_program` 认领同一字典。
#[test]
fn mmtype1_embeds_type1_snapshot_as_fontfile() {
    use crate::core::reader::font::FontProgramSource;

    let program = sample_type1_program();
    let font = Font::mmtype1(12.0, "BuilderMM Bold")
        .with_widths(65, &[500.0, 600.0])
        .with_descriptor(
            FontDescriptor::new()
                .with_font_name("BuilderMM Bold")
                .with_missing_width(500.0),
        )
        .with_font_program(FontProgram::Type1(program.clone()));

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "AB");
    let doc = b.assemble_document().expect("assemble");

    let (_, font_dict) = find_font_object(&doc, "MMType1");
    assert_eq!(
        font_dict.get("BaseFont").and_then(Value::as_name),
        Some("BuilderMM_Bold"),
        "实例名的空格按 raw 13975–13977 换成下划线"
    );
    let (stream_dict, bytes) = embedded_program_stream(&doc, &font_dict, "FontFile");
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
        "MM 快照的 Table 127 三段长度应覆盖整个流"
    );
    assert_eq!(bytes, program, "快照字节应原样嵌入");

    let value = Value::Dict(font_dict.clone());
    let embedded = doc
        .mmtype1_font_program(&value)
        .expect("程序可读")
        .expect("MM 字典应带 /FontFile");
    assert_eq!(embedded.source, FontProgramSource::FontFile);
    assert_eq!(embedded.data, program);
    let snapshot = doc
        .mmtype1_type1_program(&value)
        .expect("Type 1 快照可解析")
        .expect("FontFile 快照应被认领");
    assert_eq!(
        snapshot
            .charstrings
            .keys()
            .map(|name| name.as_str())
            .collect::<Vec<&str>>(),
        vec![".notdef", "A", "B"]
    );
    assert!(
        doc.mmtype1_type1c_program(&value)
            .expect("Type1C 入口")
            .is_none(),
        "/FontFile 快照不是 CFF 程序"
    );
}

/// Table 126（raw 15536–15540）为 MMType1 列出 `/FontFile3 /Subtype /Type1C`：
/// CFF 快照写成 `/FontFile3`，流字典带 `/Subtype /Type1C`，读端
/// `mmtype1_type1c_program` 认领同一个 MM 实例。
#[test]
fn mmtype1_embeds_cff_snapshot_as_type1c() {
    use crate::core::reader::font::fixtures;
    use crate::core::reader::font::{FontError, FontProgramSource};

    let program = fixtures::name_keyed_cff_custom_charset(0);
    let font = Font::mmtype1(12.0, "MinionMM 366 465 11 ")
        .with_widths(32, &[187.0; 4])
        .with_descriptor(FontDescriptor::new().with_missing_width(500.0))
        .with_font_program(FontProgram::CFF(program.clone()));

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "AB");
    let doc = b.assemble_document().expect("assemble");

    let (_, font_dict) = find_font_object(&doc, "MMType1");
    let (stream_dict, bytes) = embedded_program_stream(&doc, &font_dict, "FontFile3");
    assert_eq!(
        stream_dict.get("Subtype").and_then(Value::as_name),
        Some("Type1C"),
        "MM 的 CFF 快照必须写成 /Subtype /Type1C（raw 15536–15540）"
    );
    assert_eq!(bytes, program);
    for key in ["Length1", "Length2", "Length3"] {
        assert!(
            stream_dict.get(key).is_none(),
            "FontFile3 不应写 {key}: {stream_dict:?}"
        );
    }

    let value = Value::Dict(font_dict.clone());
    let embedded = doc
        .mmtype1_font_program(&value)
        .expect("程序入口")
        .expect("MM 字典应带 /FontFile3");
    assert_eq!(embedded.source, FontProgramSource::FontFile3);
    assert_eq!(embedded.subtype.as_deref(), Some("Type1C"));
    assert_eq!(embedded.data, program);
    let snapshot = doc
        .mmtype1_type1c_program(&value)
        .expect("Type1C 快照可解析")
        .expect("MM 字典应带 CFF 快照");
    assert!(
        !snapshot.is_cid_keyed(),
        "MM 实例是简单字体，CFF 快照不得为 CID-keyed"
    );

    // 读端不把手上的 Type1C 快照当作 Type 1 程序解析（raw 15529–15531）。
    let error = doc
        .mmtype1_type1_program(&value)
        .expect_err("Type1C 不是 Type 1 程序");
    assert!(
        matches!(&error, FontError::Invalid(message) if message.contains("/FontFile")),
        "{error}"
    );
}

/// Table 126 的 `/FontFile2` 行只列 TrueType/CIDFontType2（raw 15532–15535），
/// MM 实例不得嵌入 TrueType 程序：构建期 `InvalidObject` + 零字节。
#[test]
fn mmtype1_rejects_truetype_program() {
    let font = Font::mmtype1(12.0, "BuilderMM Bold")
        .with_widths(65, &[500.0])
        .with_font_program(FontProgram::TrueType(sample_truetype_program()));

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "A");
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("MM 实例不得嵌入 TrueType 程序");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(
        error.to_string().contains("/FontFile2"),
        "错误应指出 /FontFile2 不合法: {error}"
    );
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

/// Table 126 的 `/FontFile3 /Subtype /OpenType` 续表只列 `Type1` 与
/// `CIDFontType0`（raw 15569–15572），不含 MMType1：即便 `OTTO` 容器对 Type 1
/// 合法，MM 实例也应在构建期被拒。
#[test]
fn mmtype1_rejects_opentype_program() {
    let program = sample_opentype_sfnt(b"OTTO", &[0x01, 0x00, 0x04, 0x05]);
    let font = Font::mmtype1(12.0, "BuilderMM Bold")
        .with_widths(65, &[500.0])
        .with_font_program(FontProgram::OpenType(program));

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "A");
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("MM 实例不得嵌入 OpenType 程序");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(
        error.to_string().contains("OpenType"),
        "错误应指出 OpenType 容器不合法: {error}"
    );
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}
