//! XRef 流读取子主题（7.5.8 XRef 流、混合引用 `/XRefStm`、压缩与预测器流）。

use super::*;

/// XRef 流 /W 含 0 宽字段时按规范取缺省值：type（w1=0）缺省为 1
/// （in-use）。修复前 type 恒为 0（free），条目不可用。
#[test]
fn xref_stream_zero_type_width_defaults_in_use() {
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(b"%PDF-1.5\n");
    let off1 = buf.len();
    buf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    let off2 = buf.len();
    buf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");
    // XRef 流对象 3：/W [0 2 2]，type 字段宽度为 0（缺省 1）。
    // 每行 4 字节：offset(2) + generation(2)。
    let off_stm = buf.len();
    let mut stream: Vec<u8> = Vec::new();
    stream.extend_from_slice(&(off1 as u16).to_be_bytes());
    stream.extend_from_slice(&0u16.to_be_bytes());
    stream.extend_from_slice(&(off2 as u16).to_be_bytes());
    stream.extend_from_slice(&0u16.to_be_bytes());
    let hdr = format!(
        "3 0 obj\n<< /Type /XRef /Size 3 /Index [1 2] /W [0 2 2] /Length {} /Root 1 0 R >>\nstream\n",
        stream.len()
    );
    buf.extend_from_slice(hdr.as_bytes());
    buf.extend_from_slice(&stream);
    buf.extend_from_slice(b"\nendstream\nendobj\nstartxref\n");
    buf.extend_from_slice(format!("{off_stm}\n").as_bytes());
    buf.extend_from_slice(b"%%EOF\n");

    let doc = parse_buf(&buf);
    assert_eq!(doc.source_version(), PdfVersion::V1_5);
    // 缺省 type=1 → 对象 1、2 为 in-use，能按偏移取到。
    let cat = doc.get_object(1).expect("parse").expect("obj 1 present");
    assert_eq!(
        cat.value
            .as_dict()
            .unwrap()
            .get("Type")
            .and_then(|v| v.as_name()),
        Some("Catalog")
    );
    let pages = doc.get_object(2).expect("parse").expect("obj 2 present");
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

/// 混合引用文件：经典 xref 表 + trailer 含 /XRefStm 指向补充 XRef 流。
/// 流中对象 3 的条目对经典表不可见，应与经典表合并后可见。
#[test]
fn hybrid_reference_xref_stm_merges_entries() {
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(b"%PDF-1.5\n");
    let off1 = buf.len();
    buf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    let off2 = buf.len();
    buf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [] /Count 0 >>\nendobj\n");
    let off3 = buf.len();
    buf.extend_from_slice(
        b"3 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n",
    );
    // 经典 xref 表：仅覆盖对象 0..2，对象 3 不在其中。
    let off_xref = buf.len();
    buf.extend_from_slice(b"xref\n0 3\n0000000000 65535 f \n");
    buf.extend_from_slice(format!("{off1:010} 00000 n \n").as_bytes());
    buf.extend_from_slice(format!("{off2:010} 00000 n \n").as_bytes());
    // 补充 XRef 流对象 4：只给出对象 3 的条目，/W [1 2 1]。
    let off_stm = buf.len();
    let mut stream: Vec<u8> = Vec::new();
    stream.push(1u8); // type = 1（in-use）
    stream.extend_from_slice(&(off3 as u16).to_be_bytes());
    stream.push(0u8); // generation = 0
    let hdr = format!(
        "4 0 obj\n<< /Type /XRef /Size 5 /Index [3 1] /W [1 2 1] /Length {} /Root 1 0 R >>\nstream\n",
        stream.len()
    );
    buf.extend_from_slice(hdr.as_bytes());
    buf.extend_from_slice(&stream);
    buf.extend_from_slice(b"\nendstream\nendobj\n");
    // 经典 trailer 同时含 /XRefStm。
    buf.extend_from_slice(
        format!("trailer\n<< /Size 5 /Root 1 0 R /XRefStm {off_stm} >>\n").as_bytes(),
    );
    buf.extend_from_slice(b"startxref\n");
    buf.extend_from_slice(format!("{off_xref}\n").as_bytes());
    buf.extend_from_slice(b"%%EOF\n");

    let doc = parse_buf(&buf);
    // 对象 3 只存在于 XRef 流，须由 /XRefStm 合并后才可定位。
    let font = doc
        .get_object(3)
        .expect("parse")
        .expect("obj 3 present via /XRefStm");
    assert_eq!(
        font.value
            .as_dict()
            .unwrap()
            .get("Type")
            .and_then(|v| v.as_name()),
        Some("Font")
    );
    // 经典表条目仍可见。
    assert!(doc.get_object(1).expect("parse").is_some());
}

/// 去版本门控：头部声明 `%PDF-1.3`，但实际用 XRef 流（未压缩）。
/// 修复前受声明版本门控会跳过 XRef 流策略、退化到全文扫描；修复后
/// 应按内容解析出对象，并留一条「宽读」说明性 warning。
#[test]
fn xref_stream_parsed_despite_low_declared_version() {
    let buf = xref_stream_pdf("%PDF-1.3\n", "", false);
    let doc = parse_buf(&buf);
    assert_eq!(doc.source_version(), PdfVersion::V1_3);
    assert_catalog_and_pages(&doc);
    assert!(
        doc.warnings()
            .iter()
            .any(|w| w.contains("wide read") && w.contains("1.3")),
        "expected a wide-read note for declared 1.3, got: {:?}",
        doc.warnings()
    );
}

/// 压缩 XRef 流：`/Filter /FlateDecode` 的 XRef 流应能解码并解析出对象。
#[test]
fn flate_compressed_xref_stream_is_readable() {
    let buf = xref_stream_pdf("%PDF-1.5\n", "FlateDecode", false);
    let doc = parse_buf(&buf);
    assert_eq!(doc.source_version(), PdfVersion::V1_5);
    assert_catalog_and_pages(&doc);
    assert!(
        doc.warnings().is_empty(),
        "flate xref stream should parse cleanly: {:?}",
        doc.warnings()
    );
}

/// 带 Predictor 的 XRef 流：`/FlateDecode` + `/DecodeParms << /Predictor 12
/// /Columns 4 >>`（PNG Up）。这是真实 XRef 流的常见形态。
#[test]
fn flate_xref_stream_with_png_up_predictor_is_readable() {
    let buf = xref_stream_pdf("%PDF-1.5\n", "FlateDecode", true);
    let doc = parse_buf(&buf);
    assert_catalog_and_pages(&doc);
    assert!(
        doc.warnings().is_empty(),
        "predictor xref stream should parse cleanly: {:?}",
        doc.warnings()
    );
}

/// LZW 压缩的 XRef 流：`/Filter /LZWDecode` 应解码并解析出对象，
/// 且不产生「unsupported filter」类 warning。
#[test]
fn lzw_xref_stream_is_readable() {
    let buf = xref_stream_pdf("%PDF-1.5\n", "LZWDecode", false);
    let doc = parse_buf(&buf);
    assert_eq!(doc.source_version(), PdfVersion::V1_5);
    assert_catalog_and_pages(&doc);
    assert!(
        doc.warnings().is_empty(),
        "lzw xref stream should parse cleanly: {:?}",
        doc.warnings()
    );
}

/// LZW 压缩 + PNG Up Predictor 的 XRef 流：验证「先 LZW 解码、后预测器」
/// 的串联顺序在集成路径上成立。
#[test]
fn lzw_xref_stream_with_png_up_predictor_is_readable() {
    let buf = xref_stream_pdf("%PDF-1.5\n", "LZWDecode", true);
    let doc = parse_buf(&buf);
    assert_catalog_and_pages(&doc);
    assert!(
        doc.warnings().is_empty(),
        "lzw predictor xref stream should parse cleanly: {:?}",
        doc.warnings()
    );
}
