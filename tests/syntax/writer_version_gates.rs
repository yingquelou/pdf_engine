//! 附录 F 写出布局与版本门控子主题（目标版本缺省、XRef 流/特性校验写出）。

use super::*;

/// writer 不指定版本时用自身 default（1.7），**不再继承** doc.source_version。
/// （架构修复：读端版本与写端版本彻底解耦。）
#[test]
fn writer_defaults_to_1_7_not_doc_source_version() {
    let mut doc = sample_doc();
    // 设 source_version 为低版本（1.3），但 writer 应忽略
    doc.source_version = PdfVersion::V1_3;
    let mut buf = Vec::new();
    core::writer::write_document(&doc, &mut buf).expect("write");
    // 头部应是 "%PDF-1.7" 而非 "%PDF-1.3"
    assert!(
        buf.starts_with(b"%PDF-1.7\n"),
        "writer 应输出自身 default 1.7，而非 doc.source_version；head was: {:?}",
        &buf[..10.min(buf.len())]
    );
    // 回读版本应为 writer 输出的 1.7
    let doc2 = parse_buf(&buf);
    assert_eq!(doc2.source_version(), PdfVersion::V1_7);
}

/// writer 拒绝不支持的版本（2.0）——通过 with_version 显式指定。
/// （架构修复：writer 不再查看 doc.source_version，
/// 版本校验只针对 writer 自身的 with_version 设置。）
#[test]
fn writer_rejects_unsupported_version() {
    let doc = sample_doc();
    let mut buf = Vec::new();
    let e = core::writer::DocumentWriter::new()
        .with_version(PdfVersion::V2_0)
        .build(&doc, &mut buf)
        .unwrap_err();
    assert!(matches!(e, core::writer::WriteError::UnsupportedVersion(_)));
}

/// DocumentWriter Builder 可指定版本（等价于 writer 的目标版本设定）。
#[test]
fn writer_builder_with_version() {
    let doc = sample_doc(); // 默认 1.7
    let mut buf = Vec::new();
    core::writer::DocumentWriter::new()
        .with_version(PdfVersion::V1_3)
        .build(&doc, &mut buf)
        .expect("build");
    assert!(buf.starts_with(b"%PDF-1.3"), "head: {:?}", &buf[..9]);

    // 回读版本应为 1.3（来自写入头部，而非 doc.version）
    let doc2 = parse_buf(&buf);
    assert_eq!(doc2.source_version(), PdfVersion::V1_3);
}

// ── 问题 3：≥1.5 输出 XRef 流 + 对象流（优先较新特性） ─────────────

/// 目标版本 1.5：输出 XRef 流，不再有经典 `xref` 段；头部 `%PDF-1.5`，
/// `startxref` 指向 XRef 流对象；回读能取到对象。
#[test]
fn xref_stream_written_at_1_5() {
    let doc = sample_doc();
    let buf = try_write(&doc, PdfVersion::V1_5).expect("write");
    let out = String::from_utf8_lossy(&buf);

    assert!(out.contains("%PDF-1.5"), "头部应为 1.5");
    assert!(out.contains("/Type /XRef"), "应输出 XRef 流对象");
    assert!(!out.contains("\nxref\n"), "1.5 不应再有经典 xref 表：{out}");
    assert!(out.contains("%%EOF"));

    // startxref 指向的对象应是 XRef 流自身（以数字开头且含 /Type /XRef）。
    let sx = out.rfind("startxref").expect("startxref 关键字");
    let off: usize = out[sx + "startxref".len()..]
        .trim_start()
        .split(['\n', '\r'])
        .next()
        .unwrap()
        .trim()
        .parse()
        .expect("startxref 偏移应可解析");
    let seg = String::from_utf8_lossy(&buf[off..(off + 96).min(buf.len())]);
    assert!(
        seg.starts_with(|c: char| c.is_ascii_digit()),
        "startxref 应指向间接对象：{seg}"
    );
    assert!(
        seg.contains("/Type /XRef"),
        "startxref 应指向 XRef 流：{seg}"
    );

    // 回读：内容流对象（3）作为顶层对象仍可直接定位并取到字节。
    let doc2 = parse_buf(&buf);
    assert_eq!(doc2.source_version(), PdfVersion::V1_5);
    let cat = doc2.get_object(1).expect("parse").expect("catalog");
    assert_eq!(
        cat.value
            .as_dict()
            .unwrap()
            .get("Type")
            .and_then(|v| v.as_name()),
        Some("Catalog")
    );
    assert!(matches!(
        doc2.xref().get(3).map(|e| &e.kind),
        Some(XrefKind::Normal)
    ));
}

/// writer 默认版本（1.7）与 doc.source_version 彻底解耦：
/// 不指定 with_version 时输出 1.7，不受 doc.source_version 影响。
#[test]
fn writer_default_target_version_is_1_7_independent_of_source() {
    // 构造一个 source_version=1.3 的写入端 Document
    let mut doc = sample_doc();
    doc.source_version = PdfVersion::V1_3;
    assert_eq!(doc.source_version(), PdfVersion::V1_3);

    // write_document 不传 version → 输出 writer default 1.7
    let mut buf = Vec::new();
    core::writer::write_document(&doc, &mut buf).expect("write");
    let text = String::from_utf8_lossy(&buf);
    assert!(text.contains("%PDF-1.7"), "默认输出应为 1.7 而非 1.3");

    // 显式传 1.5 仍然可用
    let mut buf2 = Vec::new();
    core::writer::DocumentWriter::new()
        .with_version(PdfVersion::V1_5)
        .build(&doc, &mut buf2)
        .expect("write with_version");
    let text2 = String::from_utf8_lossy(&buf2);
    assert!(text2.contains("%PDF-1.5"));
}

/// FeatureProfile 一致性：含 JPXDecode 时
/// doc.features.contains(JPXDecode)=true，且 validate_content_version
/// 在目标 1.4 下拒绝（不可自动降级的内容特性）。
#[test]
fn feature_profile_matches_content_validation() {
    let doc = filter_stream_doc("JPXDecode");

    // FeatureProfile 应含 JPXDecode（filter_stream_doc 里 set_object 时
    // 会调 scan_value_features，把 JPXDecode 加进 features）
    let feats = doc.features.borrow();
    assert!(
        feats.contains(PdfFeature::JPXDecode),
        "set_object 应自动填充 JPXDecode 到 FeatureProfile"
    );
    drop(feats);

    // 目标 1.4 应因 JPXDecode 被拒（不可自动降级）
    let mut buf = Vec::new();
    let e = core::writer::DocumentWriter::new()
        .with_version(PdfVersion::V1_4)
        .build(&doc, &mut buf)
        .expect_err("1.4 应拒绝 JPXDecode");
    match e {
        core::writer::WriteError::UnsupportedVersion(msg) => {
            assert!(msg.contains("JPXDecode"));
            assert!(msg.contains("1.5"));
        }
        other => panic!("预期 UnsupportedVersion，得到 {other:?}"),
    }

    // 目标 1.5 通过，不报错
    core::writer::DocumentWriter::new()
        .with_version(PdfVersion::V1_5)
        .build(&doc, &mut Vec::new())
        .expect("1.5 应接受 JPXDecode");
}
