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
