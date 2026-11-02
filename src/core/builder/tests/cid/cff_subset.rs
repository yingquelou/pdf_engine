//! CID 子集请求下的 CFF 程序重写、CIDSet 裁剪与配套校验（原 `cid.rs` 主题拆分）。

use super::super::*;
use crate::core::reader::font::RawCffFont;

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
