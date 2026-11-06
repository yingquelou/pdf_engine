//! CIDFontType2 子集化的 `/FontFile2` 重写与 `/CIDToGIDMap` 重排及错误路径。

use super::super::*;
use crate::core::reader::font::TrueTypeFont;

/// 取出 CIDFont 字典引用的 `/CIDToGIDMap` 流字节。
fn cid_to_gid_map_bytes(doc: &Document, descendant: &Dict) -> Vec<u8> {
    let map_num = descendant
        .get("CIDToGIDMap")
        .and_then(Value::as_ref)
        .map(|(num, _)| num)
        .expect("CIDToGIDMap reference");
    let map = doc
        .get_object(map_num)
        .expect("CIDToGIDMap lookup")
        .expect("CIDToGIDMap object");
    match map.value {
        Value::Stream(_, StreamData::Owned(ref bytes)) => bytes.clone(),
        _ => panic!("CIDToGIDMap 应为 Owned 流"),
    }
}

#[test]
fn cid_font_type2_subset_rewrites_fontfile2_and_cid_to_gid_map() {
    let program = sample_truetype_program();
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_font_program(FontProgram::TrueType(program.clone()))
    .with_cid_to_gid_map(&[0, 1, 2])
    .with_subset_cids(&[2]);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_cid(p, idx, 10.0, 10.0, &[2]);
    let doc = b.assemble_document().expect("assemble");

    let (_, descendant) = find_font_object(&doc, "CIDFontType2");
    let expected_base = format!(
        "{}+ExampleCIDFont",
        FontProgram::TrueType(program.clone()).subset_tag_for_name("ExampleCIDFont", &[1, 0, 2])
    );
    assert_eq!(
        descendant.get("BaseFont").and_then(Value::as_name),
        Some(expected_base.as_str()),
        "CIDFont 后代 /BaseFont 应带标签"
    );

    // FontFile2 被裁剪为 glyph 0 + 原 gid 2（新 gid 1）。
    let (stream_dict, bytes) = embedded_program_stream(&doc, &descendant, "FontFile2");
    assert_ne!(bytes, program, "嵌入程序应被重写为子集");
    assert!(bytes.len() < program.len(), "子集程序应更短");
    assert_eq!(
        stream_dict.get("Length1").and_then(Value::as_integer),
        Some(bytes.len() as i64),
        "Length1 应与子集字节一致"
    );
    let subset = TrueTypeFont::parse(bytes).expect("子集程序应可解析");
    assert_eq!(subset.glyph_count(), 2, "应只保留 glyph 0 与原 gid 2");
    assert_eq!(subset.horizontal_advance_width(1), Some(600));

    // 原 gid 2 变成新 gid 1；CID 0 与原映射中未请求的 CID 1 归零。
    assert_eq!(
        cid_to_gid_map_bytes(&doc, &descendant),
        vec![0, 0, 0, 0, 0, 1],
        "/CIDToGIDMap 应按子集重排后的 gid 重建"
    );
}

#[test]
fn cid_font_type2_subset_rewrites_identity_glyph_mapping() {
    let program = sample_truetype_program();
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_font_program(FontProgram::TrueType(program))
    .with_subset_cids(&[2]);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_cid(p, idx, 10.0, 10.0, &[2]);
    let doc = b.assemble_document().expect("assemble");

    let (_, descendant) = find_font_object(&doc, "CIDFontType2");
    // 源字体按 Identity 解释 CID→GID；子集重排后不能再省略映射。
    assert_eq!(
        cid_to_gid_map_bytes(&doc, &descendant),
        vec![0, 0, 0, 0, 0, 1],
        "Identity 源字体子集化后仍需写出显式 /CIDToGIDMap"
    );
}

#[test]
fn cid_font_type2_subset_rejects_cid_outside_explicit_map() {
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_font_program(FontProgram::TrueType(sample_truetype_program()))
    .with_cid_to_gid_map(&[0, 1, 2])
    .with_subset_cids(&[5]);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_cid(p, idx, 10.0, 10.0, &[5]);
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("超出 /CIDToGIDMap 长度的 CID 应被拒绝");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(
        error.to_string().contains("outside its 3 entry"),
        "错误信息应指出映射越界: {error}"
    );
    assert!(out.is_empty());
}

#[test]
fn cid_font_type2_subset_rejects_glyph_outside_font() {
    // 显式 /CIDToGIDMap 把 CID 2 指向源程序不存在的 gid 9：
    // 子集化必须拒绝，而不是静默裁掉该字形。
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_font_program(FontProgram::TrueType(sample_truetype_program()))
    .with_cid_to_gid_map(&[0, 1, 9])
    .with_subset_cids(&[2]);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_cid(p, idx, 10.0, 10.0, &[2]);
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("映射到字体范围外的 gid 应导致子集化失败");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(
        error.to_string().contains("TrueType subsetting"),
        "错误信息应带子集化上下文: {error}"
    );
    assert!(
        error.to_string().contains("outside the font"),
        "错误信息应指出 gid 越界: {error}"
    );
    assert!(out.is_empty());
}

#[test]
fn cid_font_type2_subset_reports_truetype_errors() {
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_font_program(FontProgram::TrueType(vec![0x00, 0x01, 0x00]))
    .with_subset_cids(&[1]);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_cid(p, idx, 10.0, 10.0, &[1]);
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("损坏的 TrueType 程序应导致子集化失败");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(
        error.to_string().contains("TrueType subsetting"),
        "错误信息应带子集化上下文: {error}"
    );
    assert!(out.is_empty());
}
