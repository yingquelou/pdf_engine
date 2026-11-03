//! 词法与版本读取子主题（7.5.1 文件头词法、声明/覆盖版本、损坏 xref 回退扫描）。

use super::*;

/// reader 读文件头声明版本，且声明的版本无论高低都能解析。
#[test]
fn reader_parses_declared_version() {
    let doc = sample_doc();
    let mut buf = Vec::new();
    core::writer::write_document(&doc, &mut buf).expect("write");

    let doc2 = parse_buf(&buf);
    // writer 默认写 1.7（输出 XRef 流 + 对象流）
    assert_eq!(doc2.source_version(), PdfVersion::V1_7);
    // 1.7 支持 XRef 流与对象流，按内容解析，无回退/宽读 warning
    assert!(
        doc2.warnings().is_empty(),
        "1.7 xref stream/object stream should produce no warnings: {:?}",
        doc2.warnings()
    );
}

/// xref 损坏时回退到全文扫描，仍能取到对象。
/// 显式用 with_version 1.4 让 writer 输出经典 `xref` 表形态。
#[test]
fn damaged_xref_falls_back_to_full_scan() {
    let doc = sample_doc();
    let mut buf = Vec::new();
    core::writer::DocumentWriter::new()
        .with_version(PdfVersion::V1_4)
        .build(&doc, &mut buf)
        .expect("write");

    // 把 "xref" 关键字改成 "XXXX"，强制经典表与 XRef 流策略均不适用，
    // 链回退到全文扫描。
    let mut corrupted = buf.clone();
    if let Some(pos) = corrupted.windows(4).position(|w| w == b"xref") {
        corrupted[pos..pos + 4].copy_from_slice(b"XXXX");
    }

    let doc2 = parse_buf(&corrupted);
    // 全文扫描应能找到对象 1（Catalog）——这是回退生效的核心证据。
    let cat = doc2.get_object(1).expect("parse").expect("catalog");
    assert_eq!(
        cat.value
            .as_dict()
            .unwrap()
            .get("Type")
            .and_then(|v| v.as_name()),
        Some("Catalog")
    );
}

/// 文件无有效 %PDF 头时，版本回退为 1.0 并 warning。
#[test]
fn missing_header_defaults_version_with_warning() {
    // 构造一个没有 %PDF 头的 PDF（仅对象 + xref + trailer + startxref）
    let doc = sample_doc();
    let mut buf = Vec::new();
    core::writer::write_document(&doc, &mut buf).expect("write");
    // 剥掉头部 "%PDF-x.y\n%....\n"
    let stripped = strip_pdf_header(&buf);

    let doc2 = parse_buf(&stripped);
    assert_eq!(doc2.source_version(), PdfVersion::V1_0);
    assert!(
        doc2.warnings().iter().any(|w| w.contains("header")),
        "expected header warning: {:?}",
        doc2.warnings()
    );
}

/// Catalog /Version 覆盖：头部声明 1.4，根 Catalog 含 /Version /1.5，
/// 有效版本取两者较大者 1.5。
#[test]
fn catalog_version_overrides_header_version() {
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(b"%PDF-1.4\n");
    let off1 = buf.len();
    buf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R /Version /1.5 >>\nendobj\n");
    let off2 = buf.len();
    buf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");
    let off_xref = buf.len();
    buf.extend_from_slice(b"xref\n0 3\n0000000000 65535 f \n");
    buf.extend_from_slice(format!("{off1:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(format!("{off2:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(b"trailer\n<< /Size 3 /Root 1 0 R >>\n");
    buf.extend_from_slice(b"startxref\n");
    buf.extend_from_slice(format!("{off_xref}\n").as_bytes());
    buf.extend_from_slice(b"%%EOF\n");

    let doc = parse_buf(&buf);
    assert_eq!(doc.source_version(), PdfVersion::V1_5);
}

/// 读端宽读 2.0：`%PDF-2.0` 文件不应被当作「无有效头部」而回退 1.0，
/// 应标记为 2.0 且对象可正常取出。
#[test]
fn reader_accepts_pdf_2_0_header() {
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(b"%PDF-2.0\n");
    let off1 = buf.len();
    buf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    let off2 = buf.len();
    buf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");
    let off_xref = buf.len();
    buf.extend_from_slice(b"xref\n0 3\n0000000000 65535 f \n");
    buf.extend_from_slice(format!("{off1:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(format!("{off2:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(b"trailer\n<< /Size 3 /Root 1 0 R >>\n");
    buf.extend_from_slice(b"startxref\n");
    buf.extend_from_slice(format!("{off_xref}\n").as_bytes());
    buf.extend_from_slice(b"%%EOF\n");

    let doc = parse_buf(&buf);
    // 识别为 2.0（而非回退 1.0）
    assert_eq!(doc.source_version(), PdfVersion::V2_0);
    // 不得产生「无有效头部」的 warning
    assert!(
        !doc.warnings().iter().any(|w| w.contains("header")),
        "2.0 header must not be treated as missing: {:?}",
        doc.warnings()
    );
    // 对象仍可按 xref 正常取出
    let cat = doc.get_object(1).expect("parse").expect("catalog present");
    assert_eq!(
        cat.value
            .as_dict()
            .unwrap()
            .get("Type")
            .and_then(|v| v.as_name()),
        Some("Catalog")
    );
    let pages = doc.get_object(2).expect("parse").expect("pages present");
    assert_eq!(
        pages
            .value
            .as_dict()
            .unwrap()
            .get("Type")
            .and_then(|v| v.as_name()),
        Some("Pages")
    );
}
