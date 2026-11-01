use super::*;

/// 写入 → 重新解析：简单字体的 `/FontFile3` `/Subtype /Type1C` 应可解析为
/// name-keyed CFF，按 charset/String INDEX 还原字形名并给出轮廓。
#[test]
fn builder_embeds_name_keyed_cff_and_reader_resolves_glyph_names() {
    use pdf_engine::core::builder::{Font, FontProgram, PageSize, PdfBuilder};

    let cff = sample_name_keyed_cff_program();
    let font = Font::helvetica(12.0).with_font_program(FontProgram::CFF(cff));

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &font, 72.0, 700.0, "A");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let font_dict = find_font_dict(&reparsed, "Type1");
    let program = reparsed
        .type1c_program(&font_dict)
        .expect("should parse Type1C")
        .expect("should have Type1C program");
    assert!(!program.is_cid_keyed());
    assert_eq!(program.glyph_count(), 2);
    assert_eq!(program.glyph_name(0), Some(".notdef"));
    assert_eq!(program.glyph_name(1), Some("Alpha"));
    assert_eq!(program.glyph_for_name("Alpha"), Some(1));
    assert_eq!(program.glyph_for_name("missing"), None);
    assert_eq!(program.units_per_em(), 1000);
    assert_eq!(program.glyph_width(1), Some(600.0));

    let outline = program.outline(1).expect("glyph outline");
    assert_eq!(outline.width, 600.0);
    assert_eq!(
        outline.segments,
        vec![
            CffPathSegment::MoveTo { x: 100.0, y: 0.0 },
            CffPathSegment::LineTo { x: 300.0, y: 0.0 },
            CffPathSegment::ClosePath,
        ]
    );
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("A"));
}

/// 合成一个最小 name-keyed CFF 程序：2 个字形（`.notdef` 与 `Alpha`）、
/// charset format 0（GID 1 → SID 391）以及提供 `Alpha` 的 String INDEX。
/// 所有 DICT 操作数与偏移用定长 longint 编码，偏移可单趟计算。
fn sample_name_keyed_cff_program() -> Vec<u8> {
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

    let charstring = {
        let mut cs = shortint(600); // width 操作数（nominalWidthX 为 0）
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
    let name_index = index(&[b"NKTest".to_vec()]);
    let string_index = index(&[b"Alpha".to_vec()]);
    let gsubr_index = index(&[]);
    let charset = vec![0, 0x01, 0x87]; // format 0：GID 1 → SID 391
    let charstrings = index(&[vec![14], charstring]);

    let top_dict =
        |charset_off: i32, char_strings_off: i32, private_size: i32, private_off: i32| {
            let mut out = Vec::new();
            out.extend(longint(charset_off));
            out.push(15); // charset
            out.extend(longint(char_strings_off));
            out.push(17); // CharStrings
            out.extend(longint(private_size));
            out.extend(longint(private_off));
            out.push(18); // Private
            out
        };
    let placeholder = top_dict(0, 0, private.len() as i32, 0);
    let top_index_size = index(std::slice::from_ref(&placeholder)).len();
    let prefix =
        header.len() + name_index.len() + top_index_size + string_index.len() + gsubr_index.len();
    let charset_offset = prefix as i32;
    let private_offset = (prefix + charset.len()) as i32;
    let char_strings_offset = (prefix + charset.len() + private.len()) as i32;
    let top_dict = top_dict(
        charset_offset,
        char_strings_offset,
        private.len() as i32,
        private_offset,
    );
    assert_eq!(
        index(std::slice::from_ref(&top_dict)).len(),
        top_index_size,
        "Top DICT 长度不应随偏移取值变化"
    );

    let mut out = Vec::new();
    out.extend_from_slice(&header);
    out.extend(name_index);
    out.extend(index(&[top_dict]));
    out.extend(string_index);
    out.extend(gsubr_index);
    out.extend(charset);
    out.extend(private);
    out.extend(charstrings);
    out
}

/// 合成一个最小 CID-keyed CFF 程序：3 个字形（CID 0/7/9）、单个 Font
/// DICT、charset 与 FDSelect 均为 format 0。所有 DICT 操作数用定长
/// longint 编码，偏移可单趟计算。
fn sample_cid_keyed_cff_program() -> Vec<u8> {
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

/// 写入 → 重新解析 → 提取：CIDFontType0 的 CFF 程序按 `with_subset_cids`
/// 裁剪后仍应完整往返——`/BaseFont` 带标签、`/FontFile3` 为可解析的子
/// 集、文本经 `/ToUnicode` 提取一致。
#[test]
fn builder_cid_font_type0_subset_roundtrips_through_reader() {
    use pdf_engine::core::builder::{
        CidFont, CidFontSubtype, CidSystemInfo, FontDescriptor, FontProgram, PageSize, PdfBuilder,
    };
    use pdf_engine::core::reader::font::RawCffFont;

    let cff = sample_cid_keyed_cff_program();
    let cid_font = CidFont::new(
        "ExampleCIDFont",
        CidFontSubtype::CIDFontType0,
        CidSystemInfo::new("Adobe", "Identity", 0),
        FontDescriptor::new(),
        12.0,
    )
    .with_font_program(FontProgram::CFF(cff.clone()))
    .with_subset_cids(&[9]);

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let index = builder.cid_font(page, &cid_font);
    builder.text_cid_unicode(page, index, 72.0, 700.0, &[(9, "乙")]);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    // Type 0 父字体与 CIDFont 后代的 /BaseFont 都带六字母标签前缀。
    let assert_tagged = |base_font: &str| {
        assert_eq!(strip_subset_tag(base_font), "ExampleCIDFont");
        assert_ne!(base_font, "ExampleCIDFont", "应带标签前缀");
        let (tag, rest) = base_font.split_at(7);
        assert!(
            tag[..6].bytes().all(|b| b.is_ascii_uppercase()),
            "{base_font}"
        );
        assert_eq!(tag.as_bytes()[6], b'+');
        assert_eq!(rest, "ExampleCIDFont");
    };
    let type0_dict = find_font_dict(&reparsed, "Type0");
    let type0 = reparsed
        .type0_font(&type0_dict)
        .expect("should parse Type0")
        .expect("should be Type0");
    assert_tagged(type0.base_font());

    let descendant = find_font_dict(&reparsed, "CIDFontType0");
    let cid = reparsed
        .cid_font_type0(&descendant)
        .expect("should parse CIDFontType0")
        .expect("should be CIDFontType0");
    assert_tagged(cid.base_font());

    // /FontFile3 已被裁剪：更短、仍为 CIDFontType0C 且可被 raw 层解析，
    // 只剩 CID 0 与 CID 9（GID 重编号为 0/1）。
    let embedded = reparsed
        .cid_font_type0_program(&descendant)
        .expect("should resolve program")
        .expect("should have program");
    assert_eq!(embedded.source, FontProgramSource::FontFile3);
    assert_eq!(embedded.subtype.as_deref(), Some("CIDFontType0C"));
    assert!(embedded.data.len() < cff.len(), "子集程序应更短");
    let parsed = RawCffFont::parse(embedded.data.clone()).expect("子集程序应可解析");
    assert!(parsed.is_cid_keyed());
    assert_eq!(parsed.glyph_count(), 2, "应只剩 CID 0 与 CID 9");
    assert_eq!(parsed.glyph_for_cid(9), Some(1));
    assert_eq!(parsed.glyph_for_cid(7), None);

    // 文本经 /ToUnicode 提取一致。
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("乙"));
}
