//! CIDFont 描述符、程序嵌入与内容流写出测试。

use super::*;
use crate::core::reader::font::{RawCffFont, TrueTypeFont};

/// 合成一个最小 CID-keyed CFF：3 个字形（CID 0/7/9）、单个 Font DICT、
/// charset 与 FDSelect 均为 format 0。所有 DICT 操作数用定长 longint
/// 编码，偏移可单趟计算。
fn sample_cid_cff() -> Vec<u8> {
    fn longint(value: i32) -> Vec<u8> {
        let mut out = vec![29];
        out.extend_from_slice(&value.to_be_bytes());
        out
    }
    fn shortint(value: i16) -> Vec<u8> {
        let mut out = vec![28];
        out.extend_from_slice(&value.to_be_bytes());
        out
    }
    fn index(entries: &[Vec<u8>]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&(entries.len() as u16).to_be_bytes());
        if entries.is_empty() {
            return out;
        }
        let mut offsets = vec![1u32];
        let mut total = 1u32;
        for entry in entries {
            total += entry.len() as u32;
            offsets.push(total);
        }
        out.push(1); // offSize：本夹具的偏移都小于 0xFF
        for offset in offsets {
            out.push(offset as u8);
        }
        for entry in entries {
            out.extend_from_slice(entry);
        }
        out
    }

    // charstring：宽度操作数 400 + rmoveto + rlineto + endchar。
    let charstring = {
        let mut cs = shortint(400);
        cs.extend(shortint(100));
        cs.extend(shortint(0));
        cs.push(21); // rmoveto
        cs.extend(shortint(200));
        cs.extend(shortint(0));
        cs.push(5); // rlineto
        cs.push(14); // endchar
        cs
    };
    let private = {
        let mut p = shortint(500);
        p.push(20); // defaultWidthX
        p.extend(shortint(0));
        p.push(21); // nominalWidthX
        p
    };
    let header = [1u8, 0, 4, 1];
    let name_index = index(&[b"B".to_vec()]);
    let string_index = index(&[]);
    let gsubr_index = index(&[]);
    let charset = vec![0, 0, 7, 0, 9]; // format 0：GID 1→CID 7，GID 2→CID 9
    let fd_select = vec![0, 0, 0, 0]; // format 0：全部 FD 0
    let charstrings = index(&[vec![14], charstring.clone(), charstring]);

    let top_dict = |charset_off: i32, fd_array_off: i32, fd_select_off: i32, cs_off: i32| {
        let mut out = Vec::new();
        for value in [391, 392, 0] {
            out.extend(longint(value));
        }
        out.extend([12, 30]); // ROS
        out.extend(longint(100));
        out.extend([12, 34]); // CIDCount
        out.extend(longint(charset_off));
        out.push(15); // charset
        out.extend(longint(fd_array_off));
        out.extend([12, 36]); // FDArray
        out.extend(longint(fd_select_off));
        out.extend([12, 37]); // FDSelect
        out.extend(longint(cs_off));
        out.push(17); // CharStrings
        out
    };
    let top_index_size = index(&[top_dict(0, 0, 0, 0)]).len();
    let fd_array_size = index(&[{
        let mut fd = longint(0);
        fd.extend(longint(0));
        fd.push(18); // Private
        fd
    }])
    .len();

    let prefix =
        header.len() + name_index.len() + top_index_size + string_index.len() + gsubr_index.len();
    let charset_off = prefix;
    let fd_select_off = charset_off + charset.len();
    let fd_array_off = fd_select_off + fd_select.len();
    let private_off = fd_array_off + fd_array_size;
    let cs_off = private_off + private.len();

    let fd_array = index(&[{
        let mut fd = longint(private.len() as i32);
        fd.extend(longint(private_off as i32));
        fd.push(18); // Private
        fd
    }]);
    assert_eq!(fd_array.len(), fd_array_size);

    let mut out = Vec::new();
    out.extend_from_slice(&header);
    out.extend(name_index);
    out.extend(index(&[top_dict(
        charset_off as i32,
        fd_array_off as i32,
        fd_select_off as i32,
        cs_off as i32,
    )]));
    out.extend(string_index);
    out.extend(gsubr_index);
    out.extend(charset);
    out.extend(fd_select);
    out.extend(fd_array);
    out.extend(private);
    out.extend(charstrings);
    out
}

/// 统计 assemble 产物中指定 Subtype 的字体字典数量。
fn count_font_objects(doc: &Document, subtype: &str) -> usize {
    (1..=64)
        .filter(|num| {
            doc.get_object(*num).ok().flatten().is_some_and(|object| {
                object
                    .value
                    .as_dict()
                    .and_then(|dict| dict.get("Subtype"))
                    .and_then(Value::as_name)
                    == Some(subtype)
            })
        })
        .count()
}

/// 取出 CIDFont 描述符引用的 /CIDSet 流字节。
fn cid_set_bytes(doc: &Document, font_dict: &Dict) -> Vec<u8> {
    let descriptor_num = font_dict
        .get("FontDescriptor")
        .and_then(Value::as_ref)
        .map(|(num, _)| num)
        .expect("font descriptor");
    let descriptor = doc
        .get_object(descriptor_num)
        .expect("descriptor lookup")
        .expect("descriptor object");
    let cid_set_num = descriptor
        .value
        .as_dict()
        .and_then(|dict| dict.get("CIDSet"))
        .and_then(Value::as_ref)
        .map(|(num, _)| num)
        .expect("CIDSet reference");
    let cid_set = doc
        .get_object(cid_set_num)
        .expect("CIDSet lookup")
        .expect("CIDSet object");
    match cid_set.value {
        Value::Stream(_, StreamData::Owned(ref bytes)) => bytes.clone(),
        _ => panic!("CIDSet should be an Owned stream"),
    }
}

#[test]
fn cid_subset_rewrites_fontfile3_with_tagged_base_font() {
    let program = sample_cid_cff();
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType0,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_font_program(FontProgram::CFF(program.clone()))
    .with_subset_cids(&[9]);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_cid(p, idx, 10.0, 10.0, &[9]);
    let doc = b.assemble_document().expect("assemble");

    // Type 0 父字体与 CIDFont 后代的 /BaseFont 都带六字母标签前缀。
    let expected_base = format!(
        "{}+ExampleCIDFont",
        FontProgram::CFF(program.clone()).subset_tag_for_name("ExampleCIDFont", &[1, 0, 9])
    );
    let (_, descendant) = find_font_object(&doc, "CIDFontType0");
    assert_eq!(
        descendant.get("BaseFont").and_then(Value::as_name),
        Some(expected_base.as_str()),
        "CIDFont 后代 /BaseFont 应带标签"
    );
    let (_, type0) = find_font_object(&doc, "Type0");
    assert_eq!(
        type0.get("BaseFont").and_then(Value::as_name),
        Some(expected_base.as_str()),
        "Type 0 父字体 /BaseFont 应带标签"
    );

    // /FontFile3 被裁剪：更短、/Length 一致且可被 raw 解析层重新解析。
    let (declared_length, bytes) = embedded_program(&doc, &descendant, "FontFile3");
    assert_ne!(bytes, program, "嵌入程序应被重写为子集");
    assert!(bytes.len() < program.len(), "子集程序应更短");
    assert_eq!(declared_length, bytes.len() as i64);
    let parsed = RawCffFont::parse(bytes).expect("子集程序应可解析");
    assert!(parsed.is_cid_keyed());
    assert_eq!(parsed.glyph_count(), 2, "应只剩 CID 0 与 CID 9");
    assert_eq!(parsed.glyph_for_cid(9), Some(1));
    assert_eq!(parsed.glyph_for_cid(7), None);
}

#[test]
fn cid_subset_cids_require_matching_program() {
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_font_program(FontProgram::CFF(sample_cid_cff()))
    .with_subset_cids(&[7]);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_cid(p, idx, 10.0, 10.0, &[7]);

    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("CIDFontType2 不能嵌入 CFF 程序");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(error.to_string().contains("TrueType"), "{error}");
    assert!(out.is_empty());
}

#[test]
fn cid_subset_cids_require_cff_program() {
    // 程序类型不匹配：CIDFontType0 + TrueType。
    let mismatched = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType0,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_font_program(FontProgram::TrueType(vec![0x00, 0x01]))
    .with_subset_cids(&[7]);
    // 无嵌入程序。
    let missing = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType0,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_subset_cids(&[7]);

    for (cid_font, expected) in [(mismatched, "CFF"), (missing, "no embedded program")] {
        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        let idx = b.cid_font(p, &cid_font);
        b.text_cid(p, idx, 10.0, 10.0, &[7]);
        let mut out = Vec::new();
        let error = b.build(&mut out).expect_err("子集请求应被拒绝");
        assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
        assert!(error.to_string().contains(expected), "{error}");
        assert!(out.is_empty());
    }
}

#[test]
fn cid_subset_unknown_cid_is_invalid_object() {
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType0,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_font_program(FontProgram::CFF(sample_cid_cff()))
    .with_subset_cids(&[42]);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_cid(p, idx, 10.0, 10.0, &[42]);

    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("未知 CID 应被拒绝");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(
        error.to_string().contains("CFF subsetting"),
        "错误信息应带子集化上下文: {error}"
    );
    assert!(out.is_empty());
}

#[test]
fn cid_subset_cids_do_not_deduplicate_fonts() {
    let program = FontProgram::CFF(sample_cid_cff());
    let wide = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType0,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_font_program(program.clone())
    .with_subset_cids(&[7, 9]);
    let narrow = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType0,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_font_program(program)
    .with_subset_cids(&[7]);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let wide_idx = b.cid_font(p, &wide);
    let narrow_idx = b.cid_font(p, &narrow);
    assert_ne!(wide_idx, narrow_idx, "CID 列表不同不应页内去重");
    b.text_cid(p, wide_idx, 10.0, 10.0, &[7]);
    b.text_cid(p, narrow_idx, 10.0, 20.0, &[7]);
    let doc = b.assemble_document().expect("assemble");
    assert_eq!(
        count_font_objects(&doc, "Type0"),
        2,
        "不同 CID 列表应生成两个 Type 0 字体对象"
    );
}

#[test]
fn cid_subset_trims_cid_set() {
    // 原位图标记 CID 0、7、9：子集只保留 CID 9（CID 0 恒保留）。
    let descriptor = FontDescriptor::new().with_cid_set(vec![0b1000_0100, 0b0100_0000]);
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType0,
        CidSystemInfo::new("Adobe", "Identity", 0),
        descriptor,
        12.0,
    )
    .with_font_program(FontProgram::CFF(sample_cid_cff()))
    .with_subset_cids(&[9]);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_cid(p, idx, 10.0, 10.0, &[9]);
    let doc = b.assemble_document().expect("assemble");

    let (_, descendant) = find_font_object(&doc, "CIDFontType0");
    assert_eq!(
        cid_set_bytes(&doc, &descendant),
        vec![0b1000_0000, 0b0100_0000],
        "CIDSet 应按子集交集裁剪（CID 7 被清除）"
    );
}

#[test]
fn cid_subset_coexists_with_predefined_encoding() {
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType0,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_font_program(FontProgram::CFF(sample_cid_cff()))
    .with_predefined_encoding("UniGB-UCS2-H")
    .with_subset_cids(&[7]);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_codes(p, idx, 10.0, 10.0, &[7]);
    let s = render(&mut b);
    assert!(s.contains("/Encoding /UniGB-UCS2-H"), "{s}");
    assert!(s.contains("/CIDFontType0C"), "{s}");
}

#[test]
fn cid_font_descriptor_is_written_when_provided() {
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);

    // CIDSet：CID 0, 2, 15 存在 → 位数组 [0b10100000, 0b00000001] = [0xA0, 0x01]
    let descriptor = FontDescriptor::new()
        .with_font_name("ExampleCIDFont")
        .with_family("Example Family")
        .with_ascent(800.0)
        .with_descent(-200.0)
        .with_cap_height(700.0)
        .with_stem_v(80.0)
        .with_flags(4)
        .with_bbox([-10.0, -200.0, 1000.0, 900.0])
        .with_cid_set(vec![0xA0, 0x01]);

    let system_info = CidSystemInfo::new("Adobe", "Identity", 0);
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        system_info,
        descriptor,
        12.0,
    )
    .with_default_width(1000.0)
    .with_widths(vec![
        CidWidth::Single(65, 500.0),
        CidWidth::Range(100, 102, 600.0),
    ]);

    let cf_idx = b.cid_font(p, &cid_font);
    b.text_cid(p, cf_idx, 10.0, 10.0, &[65, 66]);
    let s = render(&mut b);

    // Type 0 父字体
    assert!(s.contains("/Subtype /Type0"), "应含 Type0 父字体: {s}");
    assert!(
        s.contains("/Encoding /Identity-H"),
        "应含 Identity-H 编码: {s}"
    );
    assert!(s.contains("/DescendantFonts"), "应含 DescendantFonts: {s}");

    // CIDFont 后代字典
    assert!(
        s.contains("/Subtype /CIDFontType2"),
        "应含 CIDFontType2: {s}"
    );
    assert!(
        s.contains("/BaseFont /ExampleCIDFont"),
        "应含 BaseFont: {s}"
    );
    assert!(s.contains("/CIDSystemInfo"), "应含 CIDSystemInfo: {s}");
    assert!(s.contains("/Registry (Adobe)"), "应含 Registry: {s}");
    assert!(s.contains("/Ordering (Identity)"), "应含 Ordering: {s}");

    // FontDescriptor
    assert!(
        s.contains("/Type /FontDescriptor"),
        "应含 FontDescriptor 类型: {s}"
    );
    assert!(
        s.contains("/FontName /ExampleCIDFont"),
        "应含 FontName: {s}"
    );
    assert!(
        s.contains("/FontFamily (Example Family)"),
        "应含 FontFamily: {s}"
    );
    assert!(s.contains("/Ascent 800"), "应含 Ascent: {s}");
    assert!(s.contains("/Descent -200"), "应含 Descent: {s}");
    assert!(s.contains("/CapHeight 700"), "应含 CapHeight: {s}");
    assert!(s.contains("/StemV 80"), "应含 StemV: {s}");
    assert!(s.contains("/Flags 4"), "应含 Flags: {s}");

    // CIDSet 流
    assert!(s.contains("/CIDSet"), "应含 CIDSet 引用: {s}");
}

#[test]
fn cid_font_descriptor_defaults_fill_table_122_required_entries() {
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    );

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_cid(p, idx, 10.0, 10.0, &[1]);
    let doc = b.assemble_document().expect("assemble");

    let (_, descendant) = find_font_object(&doc, "CIDFontType2");
    let descriptor = font_descriptor(&doc, &descendant);
    assert_eq!(
        descriptor.get("FontName").and_then(Value::as_name),
        Some("ExampleCIDFont")
    );
    assert_eq!(descriptor.get("Flags").and_then(Value::as_integer), Some(4));
    assert_eq!(
        descriptor.get("FontBBox").map(numbers),
        Some(vec![0.0, -200.0, 1000.0, 900.0])
    );
    assert_eq!(scalar(descriptor.get("ItalicAngle")), Some(0.0));
    assert_eq!(scalar(descriptor.get("Ascent")), Some(800.0));
    assert_eq!(scalar(descriptor.get("Descent")), Some(-200.0));
    assert_eq!(scalar(descriptor.get("CapHeight")), Some(700.0));
    assert_eq!(scalar(descriptor.get("StemV")), Some(80.0));
}

#[test]
fn cid_font_type2_embeds_truetype_program() {
    let program = vec![0x00, 0x01, 0x00, 0x00, 0x00, 0x0A];
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_font_program(FontProgram::TrueType(program.clone()));

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_cid(p, idx, 10.0, 10.0, &[1]);
    let s = render(&mut b);

    assert!(s.contains("/FontFile2"), "应含 /FontFile2 引用: {s}");
    assert!(
        s.contains(&format!("/Length {}", program.len())),
        "FontFile2 流长度应与程序字节一致: {s}"
    );
    assert!(!s.contains("/FontFile3"), "不应写 /FontFile3: {s}");
}

#[test]
fn cid_font_type2_fontfile2_declares_length1() {
    let program = vec![0x00, 0x01, 0x00, 0x00, 0x00, 0x0A];
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_font_program(FontProgram::TrueType(program));

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_cid(p, idx, 10.0, 10.0, &[1]);
    let doc = b.assemble_document().expect("assemble");

    let (_, descendant) = find_font_object(&doc, "CIDFontType2");
    let (stream_dict, bytes) = embedded_program_stream(&doc, &descendant, "FontFile2");
    assert_eq!(
        stream_dict.get("Length1").and_then(Value::as_integer),
        Some(bytes.len() as i64),
        "Length1 应为 TrueType 程序字节数"
    );
    assert!(stream_dict.get("Length2").is_none());
    assert!(stream_dict.get("Length3").is_none());
}

#[test]
fn cid_font_type0_embeds_cff_program_with_cidfonttype0c_subtype() {
    let program = vec![0x01, 0x00, 0x04, 0x05];
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType0,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_font_program(FontProgram::CFF(program.clone()));

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_cid(p, idx, 10.0, 10.0, &[1]);
    let s = render(&mut b);

    assert!(s.contains("/FontFile3"), "应含 /FontFile3 引用: {s}");
    assert!(
        s.contains("/Subtype /CIDFontType0C"),
        "FontFile3 流应声明 /CIDFontType0C: {s}"
    );
    assert!(!s.contains("/FontFile2"), "不应写 /FontFile2: {s}");
}

#[test]
fn cid_font_rejects_mismatched_program_type() {
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType0,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_font_program(FontProgram::TrueType(vec![0x00]));

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_cid(p, idx, 10.0, 10.0, &[1]);

    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("CIDFontType0 不应接受 TrueType 程序");
    assert!(
        error.to_string().contains("CIDFontType0"),
        "错误信息应指出子类型: {error}"
    );
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

#[test]
fn cid_font_writes_dw2_and_w2_vertical_metrics() {
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_default_vertical_metrics(900.0, -800.0)
    .with_vertical_metrics(vec![
        CidVerticalMetrics::Single(1, -500.0, 250.0, 700.0),
        CidVerticalMetrics::Range(5, 6, -600.0, 300.0, 800.0),
    ]);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_cid(p, idx, 10.0, 10.0, &[1]);
    let doc = b.assemble_document().expect("assemble");

    let (_, descendant) = find_font_object(&doc, "CIDFontType2");
    let dw2 = descendant.get("DW2").expect("/DW2");
    assert_eq!(numbers(dw2), vec![900.0, -800.0], "/DW2 为 [v_y w1_y]");

    let w2 = descendant.get("W2").and_then(Value::as_array).expect("/W2");
    assert_eq!(w2.len(), 7, "两项 /W2 条目共 2 + 5 个元素");
    assert_eq!(scalar(Some(&w2.0[0])), Some(1.0));
    assert_eq!(numbers(&w2.0[1]), vec![-500.0, 250.0, 700.0]);
    assert_eq!(scalar(Some(&w2.0[2])), Some(5.0));
    assert_eq!(scalar(Some(&w2.0[3])), Some(6.0));
    assert_eq!(
        w2.0[4..]
            .iter()
            .map(|value| scalar(Some(value)).expect("number"))
            .collect::<Vec<_>>(),
        vec![-600.0, 300.0, 800.0]
    );
}

#[test]
fn cid_font_descriptor_writes_panose_lang_and_fd_overrides() {
    let mut overrides = std::collections::BTreeMap::new();
    overrides.insert(
        "Proportional".to_string(),
        FontDescriptor::new()
            .with_bbox([0.0, -250.0, 1000.0, 900.0])
            .with_ascent(750.0),
    );
    let descriptor = FontDescriptor::new()
        .with_style_panose([
            0x02, 0x0B, 0x05, 0x02, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ])
        .with_lang("zh-CN")
        .with_fd(overrides);
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        descriptor,
        12.0,
    );

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_cid(p, idx, 10.0, 10.0, &[1]);
    let doc = b.assemble_document().expect("assemble");

    let (_, descendant) = find_font_object(&doc, "CIDFontType2");
    let descriptor_num = descendant
        .get("FontDescriptor")
        .and_then(Value::as_ref)
        .map(|(num, _)| num)
        .expect("font descriptor");
    let descriptor = doc
        .get_object(descriptor_num)
        .expect("descriptor lookup")
        .expect("descriptor object")
        .value
        .as_dict()
        .expect("descriptor dict")
        .clone();

    let style = descriptor
        .get("Style")
        .and_then(Value::as_dict)
        .expect("/Style");
    assert_eq!(
        style.get("Panose"),
        Some(&Value::HexString("020B05020300000000000000".into())),
        "/Style /Panose 应写成 12 字节十六进制"
    );
    assert_eq!(
        descriptor.get("Lang").and_then(Value::as_name),
        Some("zh-CN"),
        "/Lang 应写成名称对象"
    );
    let fd = descriptor.get("FD").and_then(Value::as_dict).expect("/FD");
    let proportional = fd
        .get("Proportional")
        .and_then(Value::as_dict)
        .expect("Proportional 覆盖");
    assert_eq!(
        numbers(proportional.get("FontBBox").expect("FontBBox")),
        vec![0.0, -250.0, 1000.0, 900.0]
    );
    assert_eq!(scalar(proportional.get("Ascent")), Some(750.0));
    assert!(proportional.get("FD").is_none(), "字形类覆盖不应再嵌套 /FD");
    assert!(
        proportional.get("Type").and_then(Value::as_name) == Some("FontDescriptor"),
        "覆盖项应保留 FontDescriptor 类型"
    );
}

#[test]
fn cid_font_omits_dw2_and_w2_by_default() {
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType0,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    );

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_cid(p, idx, 10.0, 10.0, &[1]);
    let doc = b.assemble_document().expect("assemble");

    let (_, descendant) = find_font_object(&doc, "CIDFontType0");
    assert!(descendant.get("DW2").is_none(), "默认 /DW2 应省略");
    assert!(descendant.get("W2").is_none(), "无条目时不应写 /W2");
}

#[test]
fn cid_font_rejects_invalid_vertical_metrics() {
    let build_error = |cid_font: &CidFont| {
        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        let idx = b.cid_font(p, cid_font);
        b.text_cid(p, idx, 10.0, 10.0, &[1]);
        let mut out = Vec::new();
        b.build(&mut out).expect_err("invalid vertical metrics")
    };

    let nan_dw2 = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_default_vertical_metrics(f64::NAN, -1000.0);
    let error = build_error(&nan_dw2);
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(error.to_string().contains("/DW2"), "{error}");

    let infinite_w2 = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_vertical_metrics(vec![CidVerticalMetrics::Single(1, f64::INFINITY, 0.0, 0.0)]);
    let error = build_error(&infinite_w2);
    assert!(error.to_string().contains("/W2"), "{error}");

    let reversed_range = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_vertical_metrics(vec![CidVerticalMetrics::Range(6, 5, -500.0, 250.0, 700.0)]);
    let error = build_error(&reversed_range);
    assert!(
        error.to_string().contains("ends before it starts"),
        "{error}"
    );
}

#[test]
fn cid_font_without_unicode_mappings_omits_to_unicode() {
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    );

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_cid(p, idx, 10.0, 10.0, &[65]);
    let s = render(&mut b);

    assert!(
        !s.contains("/ToUnicode"),
        "未登记映射时不应写 /ToUnicode: {s}"
    );
}

#[test]
fn text_codes_writes_two_byte_big_endian_hex() {
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    );

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_codes(p, idx, 10.0, 10.0, &[0x1234, 0xABCD]);
    let s = render(&mut b);

    assert!(
        s.contains("<1234ABCD> Tj"),
        "字符码应按 2 字节大端序写出: {s}"
    );
    // 未配置自定义编码时仍走 Identity-H。
    assert!(
        s.contains("/Encoding /Identity-H"),
        "默认仍为 Identity-H: {s}"
    );
}

#[test]
fn cid_font_type2_writes_cid_to_gid_map_stream() {
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_font_program(FontProgram::TrueType(vec![
        0x00, 0x01, 0x00, 0x00, 0x00, 0x0A,
    ]))
    .with_cid_to_gid_map(&[0, 5, 9]);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_cid(p, idx, 10.0, 10.0, &[1]);
    let doc = b.assemble_document().expect("assemble");

    let (_, descendant) = find_font_object(&doc, "CIDFontType2");
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
        Value::Stream(ref stream_dict, StreamData::Owned(ref bytes)) => {
            assert_eq!(
                stream_dict.get("Length").and_then(Value::as_integer),
                Some(6),
                "/Length 应为 3 个 gid 的 2 字节数组"
            );
            assert_eq!(bytes, &[0, 0, 0, 5, 0, 9], "gid 应按大端序写出");
        }
        _ => panic!("CIDToGIDMap 应为 Owned 流"),
    }
}

#[test]
fn cid_to_gid_map_requires_cidfonttype2() {
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType0,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_cid_to_gid_map(&[0, 1]);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_cid(p, idx, 10.0, 10.0, &[1]);
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("CIDFontType0 不应写 CIDToGIDMap");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(error.to_string().contains("cid_to_gid"), "{error}");
    assert!(out.is_empty());
}

#[test]
fn cid_to_gid_map_requires_embedded_truetype_program() {
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_cid_to_gid_map(&[0, 1]);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_cid(p, idx, 10.0, 10.0, &[1]);
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("未嵌入 TrueType 程序时 CIDToGIDMap 不可解释");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(
        error.to_string().contains("embedded TrueType"),
        "错误信息应说明需要嵌入 TrueType 程序: {error}"
    );
    assert!(out.is_empty());
}

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

#[test]
fn opentype_cff_program_writes_opentype_fontfile3_for_cid_type0() {
    let program = sample_opentype_sfnt(b"OTTO", b"sample-cff-table");
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType0,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_font_program(FontProgram::OpenType(program.clone()));

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_cid(p, idx, 10.0, 10.0, &[7]);
    let doc = b.assemble_document().expect("assemble");

    let (_, descendant) = find_font_object(&doc, "CIDFontType0");
    let (stream_dict, bytes) = embedded_program_stream(&doc, &descendant, "FontFile3");
    assert_eq!(
        stream_dict.get("Subtype").and_then(Value::as_name),
        Some("OpenType"),
        "CIDFontType0 的 CFF-flavored OpenType 应写 /Subtype /OpenType"
    );
    assert_eq!(bytes, program);
    for key in ["Length1", "Length2", "Length3"] {
        assert!(
            stream_dict.get(key).is_none(),
            "FontFile3 不应写 {key}: {stream_dict:?}"
        );
    }

    let (_, type0) = find_font_object(&doc, "Type0");
    assert_eq!(
        type0.get("Encoding").and_then(Value::as_name),
        Some("Identity-H"),
        "Type 0 父字体保持默认 Identity-H 编码"
    );
}

#[test]
fn opentype_truetype_program_writes_fontfile2_for_cid_type2() {
    let program = sample_truetype_program();
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType2,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_font_program(FontProgram::OpenType(program.clone()))
    .with_cid_to_gid_map(&[0, 2]);

    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    let idx = b.cid_font(p, &cid_font);
    b.text_cid(p, idx, 10.0, 10.0, &[1]);
    let doc = b.assemble_document().expect("assemble");

    let (_, descendant) = find_font_object(&doc, "CIDFontType2");
    let (stream_dict, bytes) = embedded_program_stream(&doc, &descendant, "FontFile2");
    assert_eq!(bytes, program);
    assert_eq!(
        stream_dict.get("Length").and_then(Value::as_integer),
        Some(bytes.len() as i64)
    );
    assert_eq!(
        stream_dict.get("Length1").and_then(Value::as_integer),
        Some(bytes.len() as i64),
        "FontFile2 应写 Table 127 的 Length1"
    );
    assert!(descendant.get("CIDToGIDMap").is_some());
}

#[test]
fn opentype_cid_font_rejects_mismatched_flavor() {
    let cff_flavored = sample_opentype_sfnt(b"OTTO", b"sample-cff-table");
    let truetype_flavored = sample_truetype_program();
    let cases = [
        (
            CidFont::new(
                "ExampleCIDFont",
                CidFontSubtype::CIDFontType0,
                CidSystemInfo::new("Adobe", "Identity", 0),
                FontDescriptor::new(),
                12.0,
            )
            .with_font_program(FontProgram::OpenType(truetype_flavored)),
            "CFF-flavored",
        ),
        (
            CidFont::new(
                "ExampleCIDFont",
                CidFontSubtype::CIDFontType2,
                CidSystemInfo::new("Adobe", "Identity", 0),
                FontDescriptor::new(),
                12.0,
            )
            .with_font_program(FontProgram::OpenType(cff_flavored)),
            "TrueType-flavored",
        ),
    ];
    for (cid_font, expected) in cases {
        let mut b = PdfBuilder::new();
        let p = b.page(PageSize::A4);
        let idx = b.cid_font(p, &cid_font);
        b.text_cid(p, idx, 10.0, 10.0, &[1]);

        let mut out = Vec::new();
        let error = b.build(&mut out).expect_err("子类型与 OpenType 轮廓不匹配");
        assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
        assert!(error.to_string().contains(expected), "{error}");
        assert!(out.is_empty(), "构建失败时不应写出任何字节");
    }
}
