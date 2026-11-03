//! 流过滤器与 Name 转义子主题（7.4 过滤器解码去留、内容特性版本校验）。

use super::*;

// ── 问题 7：写端 Name `#xx` 转义（严写） ──────────────────────────

/// Name 转义回读（roundtrip）：含空格、分隔符、`#` 与 Latin-1 非 ASCII 的
/// Name（既作字典键又作 Name 值）写出后应能被 reader 精确读回。
///
/// 分别在 1.4（经典 xref，对象顶层写出）与 1.5（XRef 流 + 对象流打包）
/// 两种输出形态下验证，保证转义不受布局影响。
#[test]
fn name_escaping_roundtrips() {
    // 值含：空格 / 圆括号 / `#` / `%` / Latin-1 非 ASCII（é=U+00E9、ÿ=U+00FF）
    // 键含：圆括号 / `#`
    let key = "K(1)#x";
    let val = "a b(c)#d%é/ÿ";
    let expected_escape = "/a#20b#28c#29#23d#25#E9#2F#FF";
    let expected_key_escape = "/K#281#29#23x";

    let mut doc = Document::new();
    doc.set_object(Object::new(
        1,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Catalog".into()))
                .insert(key, Value::Name(val.into())),
        ),
    ));
    doc.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(1, 0)),
    };

    for version in [PdfVersion::V1_4, PdfVersion::V1_5] {
        let buf = try_write(&doc, version).expect("write");
        let out = String::from_utf8_lossy(&buf);
        assert!(
            out.contains(expected_escape),
            "version {version}: 转义后的 Name 值应出现在输出中"
        );
        assert!(
            out.contains(expected_key_escape),
            "version {version}: 转义后的字典键应出现在输出中"
        );

        let doc2 = parse_buf(&buf);
        let cat = doc2.get_object(1).expect("parse").expect("catalog");
        let d = cat.value.as_dict().unwrap();
        assert_eq!(
            d.get(key).and_then(|v| v.as_name()),
            Some(val),
            "version {version}: Name 值应精确回读"
        );
    }
}

/// 内容含 `/Filter /JPXDecode`（需 1.5）：目标 1.4 → UnsupportedVersion
/// （信息含特性名与最低版本 1.5）；目标 1.5 → 成功。
#[test]
fn content_version_check_rejects_jpx_below_1_5() {
    let doc = filter_stream_doc("JPXDecode");
    let mut buf = Vec::new();
    let e = core::writer::DocumentWriter::new()
        .with_version(PdfVersion::V1_4)
        .build(&doc, &mut buf)
        .expect_err("1.4 目标下 JPXDecode 应被拒");
    let msg = match e {
        core::writer::WriteError::UnsupportedVersion(m) => m,
        other => panic!("expected UnsupportedVersion, got {other:?}"),
    };
    assert!(msg.contains("JPXDecode"), "信息应含特性名：{msg}");
    assert!(msg.contains("1.5"), "信息应含最低版本 1.5：{msg}");

    // 目标 1.5：通过，且 writer 自身选择的 XRef 流/对象流不会被误判为越版。
    let buf2 = try_write(&doc, PdfVersion::V1_5).expect("1.5 应成功");
    let out = String::from_utf8_lossy(&buf2);
    assert!(out.contains("%PDF-1.5"));
    assert!(out.contains("/Type /XRef"));
}

/// 内容-版本校验覆盖其余特性：JBIG2Decode(1.4)、透明度（/SMask 与
/// ExtGState 的 /ca）、可选内容（Catalog /OCProperties，1.5）、
/// 加密（trailer /Encrypt，1.1）。
#[test]
fn content_feature_checks_cover_jbig2_transparency_oc_and_encryption() {
    // JBIG2Decode 需 1.4：1.3 拒绝，1.4 通过。
    let jbig2 = filter_stream_doc("JBIG2Decode");
    assert!(matches!(
        try_write(&jbig2, PdfVersion::V1_3),
        Err(core::writer::WriteError::UnsupportedVersion(_))
    ));
    assert!(try_write(&jbig2, PdfVersion::V1_4).is_ok());

    // 透明度 /SMask 需 1.4：1.3 拒绝，1.4 通过。
    let smask = sample_doc();
    smask.set_object(Object::new(
        4,
        0,
        Value::Dict(Dict::new().insert("SMask", Value::Ref(3, 0))),
    ));
    assert!(matches!(
        try_write(&smask, PdfVersion::V1_3),
        Err(core::writer::WriteError::UnsupportedVersion(_))
    ));
    assert!(try_write(&smask, PdfVersion::V1_4).is_ok());

    // ExtGState 的 /ca（软掩码/混合）同上。
    let gstate = sample_doc();
    gstate.set_object(Object::new(
        4,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("ExtGState".into()))
                .insert("ca", Value::Float(0.5)),
        ),
    ));
    assert!(matches!(
        try_write(&gstate, PdfVersion::V1_3),
        Err(core::writer::WriteError::UnsupportedVersion(_))
    ));
    assert!(try_write(&gstate, PdfVersion::V1_4).is_ok());

    // Catalog /OCProperties 需 1.5：1.4 拒绝，1.5 通过。
    let oc = sample_doc();
    oc.set_object(Object::new(
        1,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Catalog".into()))
                .insert("Pages", Value::Ref(2, 0))
                .insert("OCProperties", Value::Dict(Dict::new())),
        ),
    ));
    assert!(matches!(
        try_write(&oc, PdfVersion::V1_4),
        Err(core::writer::WriteError::UnsupportedVersion(_))
    ));
    assert!(try_write(&oc, PdfVersion::V1_5).is_ok());

    // trailer /Encrypt 需 1.1：1.0 拒绝，1.1 通过。
    let mut enc = sample_doc();
    let enc_dict = enc
        .trailer
        .dict
        .clone()
        .insert("Encrypt", Value::Dict(Dict::new()));
    enc.trailer = Trailer { dict: enc_dict };
    assert!(matches!(
        try_write(&enc, PdfVersion::V1_0),
        Err(core::writer::WriteError::UnsupportedVersion(_))
    ));
    assert!(try_write(&enc, PdfVersion::V1_1).is_ok());
}

/// 读端解压后的流字典 /Filter /FlateDecode 应被 writer 自动移除，
/// 输出的流字典不含压缩过滤器、/Length 对应未压缩字节长度。
///
/// 这验证「宽进（读端解压）→ 严出（writer 按目标版本重新表达）」
/// 的关键协调逻辑。
#[test]
fn writer_strips_compression_filter_after_reader_decompression() {
    // 构造一个手动 Flate 压缩的流，手动模拟「读端解压后 Dict 仍保留 Filter」场景
    // （测试用手动压缩/解压绕过 reader，因为 sample_doc / filter_stream_doc
    // 里的 Owned 流本就未被压缩）
    let flate_bytes = {
        use flate2::Compression;
        use flate2::write::ZlibEncoder;
        let raw = b"BT 72 720 Td (compressed content) Tj ET";
        let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
        enc.write_all(raw).unwrap();
        enc.finish().unwrap()
    };

    // 模拟读端解压后的状态：Owned 存解压后字节，但 dict 仍保留 /Filter /FlateDecode
    // （实际读端会做这个，此处手动构造验证 writer 的修正逻辑）
    let mut doc = Document::new();
    doc.set_object(Object::new(
        1,
        0,
        Value::Dict(Dict::new().insert("Type", Value::Name("Catalog".into()))),
    ));
    doc.set_object(Object::new(
        2,
        0,
        Value::Stream(
            Dict::new().insert("Filter", Value::Name("FlateDecode".into())),
            StreamData::Owned(b"BT 72 720 Td (compressed content) Tj ET".to_vec()),
        ),
    ));
    doc.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(1, 0)),
    };

    // writer 应自动去掉 /Filter /FlateDecode，输出未压缩内容
    let mut out = Vec::new();
    core::writer::write_document(&doc, &mut out).expect("write");
    let text = String::from_utf8_lossy(&out);

    // 输出流字典不应再有 /Filter /FlateDecode
    let no_filter = !text.contains("FlateDecode");
    assert!(
        no_filter,
        "writer 应移除 /Filter /FlateDecode（字节已解压）"
    );

    // 输出流字节应是未压缩的（因为 Owned 里存的就是解压后字节）
    assert!(
        text.contains("BT 72 720 Td (compressed content) Tj ET"),
        "输出流应直接包含未压缩的 PDF 操作符"
    );

    // 回读：内容应完全一致
    let doc2 = parse_buf(&out);
    let stream_obj = doc2.get_object(2).expect("回读").expect("流对象");
    let bytes = doc2.stream_bytes(&stream_obj.value).expect("流字节");
    assert_eq!(
        bytes, b"BT 72 720 Td (compressed content) Tj ET",
        "回读的流字节应等于未压缩内容"
    );

    // 确认：flate_bytes 变量在这个场景里没用了，但保留它展示「原始压缩→解压→输出未压缩」的链路
    let _ = flate_bytes;
}

/// 回归（#10）：写端的过滤器处理与 `filters.rs::canonical_filter`（唯一事实
/// 源）对齐——读端**已解压**的过滤器（FlateDecode/LZWDecode/CCITTFaxDecode/
/// DCTDecode，含缩写 Fl/LZW/CCF/DCT）应被剥离；读端**不解压**的过滤器（如
/// JBIG2Decode，含缩写）必须原样保留，否则会把压缩态字节当明文写出。
#[test]
fn writer_filter_handling_matches_reader_decoding() {
    let mk = |filter: &str| {
        let mut doc = Document::new();
        doc.set_object(Object::new(
            1,
            0,
            Value::Dict(Dict::new().insert("Type", Value::Name("Catalog".into()))),
        ));
        doc.set_object(Object::new(
            2,
            0,
            Value::Stream(
                Dict::new().insert("Filter", Value::Name(filter.into())),
                StreamData::Owned(b"\x00\x01\x02".to_vec()),
            ),
        ));
        doc.trailer = Trailer {
            dict: Dict::new().insert("Root", Value::Ref(1, 0)),
        };
        doc
    };

    // 读端已解压：FlateDecode/LZWDecode/CCITTFaxDecode/DCTDecode（及缩写
    // Fl/LZW/CCF/DCT）应被剥离——本就已是明文字节。
    for f in [
        "FlateDecode",
        "Fl",
        "LZWDecode",
        "LZW",
        "CCITTFaxDecode",
        "CCF",
        "DCTDecode",
        "DCT",
    ] {
        let out = try_write(&mk(f), PdfVersion::V1_4).expect("write");
        let doc2 = parse_buf(&out);
        let s = doc2.get_object(2).expect("parse").expect("stream");
        let kept = s.value.as_dict().and_then(|d| d.get("Filter"));
        assert!(
            kept.is_none(),
            "读端已解压的过滤器应被剥离（{f}），实际: {kept:?}"
        );
    }

    // 读端不解压：JBIG2Decode（仍未实现）应原样保留。
    for f in ["JBIG2Decode"] {
        let out = try_write(&mk(f), PdfVersion::V1_4).expect("write");
        let doc2 = parse_buf(&out);
        let s = doc2.get_object(2).expect("parse").expect("stream");
        let kept = s
            .value
            .as_dict()
            .and_then(|d| d.get("Filter"))
            .and_then(|v| v.as_name());
        assert_eq!(kept, Some(f), "读端不解压的过滤器应原样保留（{f}）");
    }
}
