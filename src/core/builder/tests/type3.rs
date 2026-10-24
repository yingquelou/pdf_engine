//! Type 3 字体字典与字符过程测试。

use super::*;
use crate::core::types::{Array, StreamData, Value};

#[test]
fn type3_font_rejects_subset_type1_glyphs() {
    let type3 = Type3Font::new().glyph(Type3Glyph::new(65, "A", 500.0, b"500 0 d0"));
    let font = Font::type3(12.0, type3).with_subset_type1_glyphs(&["A"]);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "A");

    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("Type 3 字体不应接受 subset_type1_glyphs");
    assert!(
        matches!(&error, WriteError::InvalidObject(_)),
        "错误应为 InvalidObject: {error}"
    );
    assert!(
        error.to_string().contains("cannot be subset"),
        "错误信息应说明无字体程序可裁剪: {error}"
    );
    assert!(
        error.to_string().contains("subset_type1_glyphs"),
        "错误信息应指出被拒绝的字段: {error}"
    );
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

#[test]
fn type3_font_writes_char_procs_widths_and_encoding() {
    let type3 = Type3Font::new()
        .with_bbox([0.0, -200.0, 1000.0, 900.0])
        .with_matrix([0.002, 0.0, 0.0, 0.002, 0.0, 0.0])
        .glyph(Type3Glyph::new(65, "A", 500.0, b"500 0 d0"))
        .glyph(Type3Glyph::new(67, "C", 700.0, b"700 0 d0"));
    let font = Font::type3(12.0, type3);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "AC");
    let doc = b.assemble_document().expect("assemble");

    let (_, font_dict) = find_font_object(&doc, "Type3");
    assert!(
        font_dict.get("BaseFont").is_none(),
        "Type 3 字体不应写 /BaseFont"
    );
    assert!(
        font_dict.get("FontDescriptor").is_none(),
        "Type 3 字体不应写 /FontDescriptor"
    );
    assert_eq!(
        font_dict.get("FontBBox").map(numbers),
        Some(vec![0.0, -200.0, 1000.0, 900.0])
    );
    assert_eq!(
        font_dict.get("FontMatrix").map(numbers),
        Some(vec![0.002, 0.0, 0.0, 0.002, 0.0, 0.0])
    );
    assert_eq!(
        font_dict.get("FirstChar").and_then(Value::as_integer),
        Some(65)
    );
    assert_eq!(
        font_dict.get("LastChar").and_then(Value::as_integer),
        Some(67)
    );
    // 字符码 66 是空隙，宽度补 0。
    assert_eq!(
        font_dict.get("Widths").map(numbers),
        Some(vec![500.0, 0.0, 700.0])
    );

    // /Encoding 只含按字符码升序的 /Differences（无 /BaseEncoding）。
    let encoding = font_dict
        .get("Encoding")
        .and_then(Value::as_dict)
        .expect("Type 3 字体应写 /Encoding");
    assert!(
        encoding.get("BaseEncoding").is_none(),
        "Font::type3 只写 /Differences"
    );
    assert_eq!(
        encoding.get("Differences"),
        Some(&Value::Array(
            Array::new()
                .push(Value::Integer(65))
                .push(Value::Name("A".into()))
                .push(Value::Integer(67))
                .push(Value::Name("C".into()))
        ))
    );

    // /CharProcs 按字形名索引，流对象号按字形列表顺序连续分配。
    let char_procs = font_dict
        .get("CharProcs")
        .and_then(Value::as_dict)
        .expect("/CharProcs");
    let mut proc_nums = Vec::new();
    for (name, expected) in [("A", b"500 0 d0".as_slice()), ("C", b"700 0 d0".as_slice())] {
        let (proc_num, _) = char_procs
            .get(name)
            .and_then(Value::as_ref)
            .unwrap_or_else(|| panic!("缺少字符过程 {name}"));
        proc_nums.push(proc_num);
        let object = doc
            .get_object(proc_num)
            .expect("字符过程查找")
            .expect("字符过程对象");
        match object.value {
            Value::Stream(ref stream_dict, StreamData::Owned(ref bytes)) => {
                assert_eq!(bytes, expected, "{name} 的字符过程字节");
                assert_eq!(
                    stream_dict.get("Length").and_then(Value::as_integer),
                    Some(expected.len() as i64),
                    "{name} 流的 /Length"
                );
            }
            _ => panic!("字符过程应为 Owned 流"),
        }
    }
    assert_eq!(
        proc_nums[1],
        proc_nums[0] + 1,
        "CharProcs 流应按字形列表顺序连续编号"
    );

    // Type 3 字体不写字体程序（含描述符内的 /FontFile*）。
    assert!(!render(&mut b).contains("/FontFile"), "不应写字体程序");
}

#[test]
fn type3_font_requires_glyphs() {
    let font = Font::type3(12.0, Type3Font::new());
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "A");

    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("空字形列表应构建失败");
    assert!(
        matches!(&error, WriteError::InvalidObject(_)),
        "错误应为 InvalidObject: {error}"
    );
    assert!(
        error.to_string().contains("at least one glyph"),
        "错误信息应说明缺少字形: {error}"
    );
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

#[test]
fn type3_font_rejects_duplicate_codes_or_names() {
    for (glyphs, needle) in [
        (
            vec![
                Type3Glyph::new(65, "A", 500.0, b"500 0 d0"),
                Type3Glyph::new(65, "B", 600.0, b"600 0 d0"),
            ],
            "duplicate character code 65",
        ),
        (
            vec![
                Type3Glyph::new(65, "A", 500.0, b"500 0 d0"),
                Type3Glyph::new(66, "A", 600.0, b"600 0 d0"),
            ],
            "duplicate glyph name",
        ),
    ] {
        let mut type3 = Type3Font::new();
        for glyph in glyphs {
            type3 = type3.glyph(glyph);
        }
        let font = Font::type3(12.0, type3);

        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        b.text(p, &font, 10.0, 10.0, "AB");

        let mut out = Vec::new();
        let error = b.build(&mut out).expect_err("重复定义应构建失败");
        assert!(
            matches!(&error, WriteError::InvalidObject(_)),
            "错误应为 InvalidObject: {error}"
        );
        assert!(
            error.to_string().contains(needle),
            "错误信息应包含 {needle:?}: {error}"
        );
        assert!(out.is_empty(), "构建失败时不应写出任何字节");
    }
}

#[test]
fn type3_font_rejects_embedded_font_program() {
    let type3 = Type3Font::new().glyph(Type3Glyph::new(65, "A", 500.0, b"500 0 d0"));
    let font = Font::type3(12.0, type3)
        .with_font_program(FontProgram::Type1(b"%!PS-AdobeFont-1.0: TestFont".to_vec()));

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "A");

    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("Type 3 字体不应接受嵌入字体程序");
    assert!(
        matches!(&error, WriteError::InvalidObject(_)),
        "错误应为 InvalidObject: {error}"
    );
    assert!(
        error.to_string().contains("must not embed a font program"),
        "错误信息应说明不允许字体程序: {error}"
    );
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

#[test]
fn type3_font_rejects_subset_glyphs() {
    let type3 = Type3Font::new().glyph(Type3Glyph::new(65, "A", 500.0, b"500 0 d0"));
    let font = Font::type3(12.0, type3).with_subset_glyphs(&[0]);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.text(p, &font, 10.0, 10.0, "A");

    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("Type 3 字体不应接受 subset_glyphs");
    assert!(
        matches!(&error, WriteError::InvalidObject(_)),
        "错误应为 InvalidObject: {error}"
    );
    assert!(
        error.to_string().contains("cannot be subset"),
        "错误信息应说明无字体程序可裁剪: {error}"
    );
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}
