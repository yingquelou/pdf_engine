use super::*;

#[test]
fn indirect_stream_length_handles_embedded_endstream_and_cycles() {
    let payload = b"before endstream after";
    let mut pdf = b"%PDF-1.7\n".to_vec();
    let offset1 = pdf.len();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog >>\nendobj\n");
    let offset2 = pdf.len();
    pdf.extend_from_slice(b"2 0 obj\n<< /Length 3 0 R >>\nstream\n");
    pdf.extend_from_slice(payload);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");
    let offset3 = pdf.len();
    pdf.extend_from_slice(format!("3 0 obj\n{}\nendobj\n", payload.len()).as_bytes());
    let xref_offset = pdf.len();
    pdf.extend_from_slice(b"xref\n0 4\n0000000000 65535 f \n");
    pdf.extend_from_slice(format!("{offset1:010} 00000 n \n").as_bytes());
    pdf.extend_from_slice(format!("{offset2:010} 00000 n \n").as_bytes());
    pdf.extend_from_slice(format!("{offset3:010} 00000 n \n").as_bytes());
    pdf.extend_from_slice(
        format!("trailer\n<< /Size 4 /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n").as_bytes(),
    );

    let document = parse_buf(&pdf);
    let stream = document.get_object(2).unwrap().unwrap();
    assert_eq!(document.try_stream_bytes(&stream.value).unwrap(), payload);

    let mut cyclic = b"%PDF-1.7\n".to_vec();
    let offset1 = cyclic.len();
    cyclic.extend_from_slice(b"1 0 obj\n<< /Type /Catalog >>\nendobj\n");
    let offset2 = cyclic.len();
    cyclic.extend_from_slice(b"2 0 obj\n<< /Length 2 0 R >>\nstream\ncycle\nendstream\nendobj\n");
    let xref_offset = cyclic.len();
    cyclic.extend_from_slice(b"xref\n0 3\n0000000000 65535 f \n");
    cyclic.extend_from_slice(format!("{offset1:010} 00000 n \n").as_bytes());
    cyclic.extend_from_slice(format!("{offset2:010} 00000 n \n").as_bytes());
    cyclic.extend_from_slice(
        format!("trailer\n<< /Size 3 /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n").as_bytes(),
    );
    let document = parse_buf(&cyclic);
    let error = document.get_object(2).unwrap_err();
    assert!(error.message.contains("cyclic indirect stream /Length"));
}

/// 环形 `/Prev` 链（xref A 的 `/Prev` 指向 B，B 又指回 A）：解析必须
/// 终止（防死循环护栏生效）、记 revisited warning，且链上对象仍可解析。
#[test]
fn circular_prev_chain_terminates_with_warning() {
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(b"%PDF-1.4\n");
    let off1 = buf.len();
    buf.extend_from_slice(b"1 0 obj\n<< /Marker /InRing >>\nendobj\n");
    let off_xref_a = buf.len();

    // 修订 A：覆盖对象 1，/Prev 指向修订 B（B 在文件更后处）。/Prev 用
    // 固定 10 位宽度占位，使 A 段长度与占位值无关，可一次算出 B 的偏移。
    let section_a = |prev: u64| {
        let mut s = String::from("xref\n0 2\n0000000000 65535 f \n");
        s.push_str(&format!("{off1:010} 00000 n \n"));
        s.push_str(&format!(
            "trailer\n<< /Size 2 /Root 1 0 R /Prev {prev:010} >>\nstartxref\n{off_xref_a}\n%%EOF\n"
        ));
        s
    };
    let off_xref_b = off_xref_a + section_a(0).len();
    buf.extend_from_slice(section_a(off_xref_b as u64).as_bytes());

    // 修订 B：/Prev 指回修订 A，构成 A→B→A 环；startxref 指向 B（最新修订）。
    buf.extend_from_slice(b"xref\n0 1\n0000000000 65535 f \n");
    buf.extend_from_slice(
        format!("trailer\n<< /Size 2 /Root 1 0 R /Prev {off_xref_a} >>\nstartxref\n{off_xref_b}\n%%EOF\n")
            .as_bytes(),
    );

    let doc = parse_buf(&buf);
    assert!(
        doc.warnings().iter().any(|w| w.contains("revisited")),
        "环形 /Prev 应记 revisited warning：{:?}",
        doc.warnings()
    );
    let o1 = doc.get_object(1).expect("parse").expect("obj 1 present");
    assert_eq!(
        o1.value
            .as_dict()
            .unwrap()
            .get("Marker")
            .and_then(|v| v.as_name()),
        Some("InRing")
    );
}

/// 写端对 `StreamData::Ref`（读端未解压的流，如 DCTDecode 图像）按原样
/// 透传：`/Filter` 保留、流字节不变（不得按 Owned 流的剥离规则误删过滤器）。
#[test]
fn ref_stream_passthrough_keeps_filter_and_bytes() {
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(b"%PDF-1.4\n");
    let off1 = buf.len();
    buf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog >>\nendobj\n");
    let off2 = buf.len();
    buf.extend_from_slice(
        b"2 0 obj\n<< /Filter /DCTDecode /Length 4 >>\nstream\n\xFF\xD9\xAB\xCD\nendstream\nendobj\n",
    );
    let off_xref = buf.len();
    buf.extend_from_slice(b"xref\n0 3\n0000000000 65535 f \n");
    buf.extend_from_slice(format!("{off1:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(format!("{off2:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(
        format!("trailer\n<< /Size 3 /Root 1 0 R >>\nstartxref\n{off_xref}\n%%EOF\n").as_bytes(),
    );

    let doc = parse_buf(&buf);
    let out = try_write(&doc, PdfVersion::V1_4).expect("write");
    let doc2 = parse_buf(&out);
    let s = doc2.get_object(2).expect("parse").expect("stream present");
    let kept = s
        .value
        .as_dict()
        .and_then(|d| d.get("Filter"))
        .and_then(|v| v.as_name());
    assert_eq!(kept, Some("DCTDecode"), "Ref 流的过滤器应原样保留");
    assert_eq!(
        doc2.stream_bytes(&s.value).expect("stream bytes"),
        [0xFF, 0xD9, 0xAB, 0xCD],
        "Ref 流字节应原样透传"
    );
}

/// xref 条目的 generation 与对象头的 generation 不一致时：以对象头为准
/// （xref gen 仅为建议值），对象仍可解析并进入缓存。
#[test]
fn xref_generation_mismatch_defers_to_object_header() {
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(b"%PDF-1.4\n");
    let off1 = buf.len();
    buf.extend_from_slice(b"1 0 obj\n<< /Marker /Gen0 >>\nendobj\n");
    let off_xref = buf.len();
    // 条目声明 gen=1，但对象头是 `1 0 obj`（gen=0）——刻意不一致。
    buf.extend_from_slice(b"xref\n0 2\n0000000000 65535 f \n");
    buf.extend_from_slice(format!("{off1:010} 00001 n \n").as_bytes());
    buf.extend_from_slice(
        format!("trailer\n<< /Size 2 /Root 1 0 R >>\nstartxref\n{off_xref}\n%%EOF\n").as_bytes(),
    );

    let doc = parse_buf(&buf);
    let o = doc.get_object(1).expect("parse").expect("obj 1 present");
    assert_eq!(o.generation, 0, "对象头的 generation 应优先于 xref 条目");
    assert_eq!(
        o.value
            .as_dict()
            .unwrap()
            .get("Marker")
            .and_then(|v| v.as_name()),
        Some("Gen0")
    );
    assert_eq!(doc.cached_object_count(), 1, "解析结果应进入缓存");
}

/// 写入 → 重新解析：启用 D4 压缩策略后，图片流应带 PNG Up 预测器地写成
/// FlateDecode，内容流写成无 DecodeParms 的 FlateDecode；读端解码字节与
/// 原始样本一致，文本仍可提取，且整体输出严格小于同内容明文版本。
#[test]
fn builder_flate_stream_compression_roundtrips_through_parser() {
    use pdf_engine::core::builder::{ColorSpace, Font, ImageData, PageSize, PdfBuilder};
    use pdf_engine::core::writer::{ImagePredictor, StreamCompression};

    let mut pixels = Vec::with_capacity(16 * 4 * 3);
    for y in 0..4u8 {
        for x in 0..16u8 {
            pixels.extend_from_slice(&[x * 15, y * 60, (x + y) * 14]);
        }
    }
    let img = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 16,
        height: 4,
        color_space: ColorSpace::DeviceRGB,
        bits: 8,
        data: pixels.clone(),
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };

    let build = |compress: bool| -> Vec<u8> {
        let mut builder = PdfBuilder::new();
        if compress {
            builder.compress_streams(
                StreamCompression::flate().with_image_predictor(ImagePredictor::PngUp),
            );
        }
        let page = builder.page(PageSize::A4);
        builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "Flate");
        // 重复填充文本把内容流推到 ~450 B，确保 zlib 收益超过头部开销。
        builder.text(
            page,
            &Font::helvetica(9.0),
            72.0,
            700.0,
            &"padding ".repeat(40),
        );
        builder.image(page, 72.0, 600.0, 96.0, 24.0, &img);
        let mut buf = Vec::new();
        builder.build(&mut buf).expect("build");
        buf
    };

    let plain = build(false);
    let compressed = build(true);
    assert!(
        compressed.len() < plain.len(),
        "压缩输出应小于明文输出: plain {} B, compressed {} B",
        plain.len(),
        compressed.len()
    );
    let text = String::from_utf8_lossy(&compressed).into_owned();
    assert!(text.contains("/Filter /FlateDecode"), "{text}");
    assert!(text.contains("/Predictor 15"), "{text}");

    let reparsed = parse_buf(&compressed);
    let page_object = (3..=64)
        .filter_map(|num| reparsed.get_object(num).ok().flatten())
        .find(|object| {
            object
                .value
                .as_dict()
                .and_then(|dict| dict.get("Type"))
                .and_then(Value::as_name)
                == Some("Page")
        })
        .expect("should find page object");
    let page_dict = page_object.value.as_dict().expect("page is a dict");
    let xobjects = page_dict
        .get("Resources")
        .and_then(Value::as_dict)
        .and_then(|resources| resources.get("XObject"))
        .and_then(Value::as_dict)
        .expect("page should have XObject resources");
    let Value::Ref(image_num, _) = xobjects.get("Im1").expect("Im1 resource") else {
        panic!("Im1 should be an indirect reference");
    };
    let image_object = reparsed
        .get_object(*image_num)
        .expect("get object")
        .expect("image object should exist");
    let Value::Stream(image_dict, _) = &image_object.value else {
        panic!("image should be a stream");
    };
    assert_eq!(
        image_dict.get("Filter").and_then(Value::as_name),
        Some("FlateDecode")
    );
    let parms = image_dict
        .get("DecodeParms")
        .and_then(Value::as_dict)
        .expect("image should carry DecodeParms");
    assert_eq!(parms.get("Predictor").and_then(Value::as_integer), Some(15));
    assert_eq!(parms.get("Colors").and_then(Value::as_integer), Some(3));
    assert_eq!(parms.get("Columns").and_then(Value::as_integer), Some(16));
    assert_eq!(
        reparsed
            .decode_stream_bytes(&image_object.value)
            .expect("decode image"),
        pixels
    );

    let Value::Ref(content_num, _) = page_dict.get("Contents").expect("Contents") else {
        panic!("Contents should be an indirect reference");
    };
    let content_object = reparsed
        .get_object(*content_num)
        .expect("get content object")
        .expect("content stream should exist");
    let Value::Stream(content_dict, _) = &content_object.value else {
        panic!("content should be a stream");
    };
    assert_eq!(
        content_dict.get("Filter").and_then(Value::as_name),
        Some("FlateDecode")
    );
    assert!(
        content_dict.get("DecodeParms").is_none(),
        "非图像流不应带预测器参数"
    );
    let content = reparsed
        .decode_stream_bytes(&content_object.value)
        .expect("decode content");
    let content_text = String::from_utf8_lossy(&content);
    assert!(content_text.contains("/Im1 Do"), "{content_text}");
    let extracted = reparsed.extract_page_text(0).expect("compressed text");
    assert!(extracted.contains("Flate"), "提取文本: {extracted:?}");
}
