//! JBIG2Decode（§7.4.7）段语法层集成测试：ISO 32000-1 worked example
//! 原样落入最小 PDF，走公开 API（`Document::page_image` / `Document::image`）。
//!
//! 字节夹具全部由规范 Example 1/2 的印刷十六进制逐行转录（`tests/fixtures/`
//! 不入库）；Example 2 的 end-of-file 段按 T.88 §7.2.1 补全为 11 字节
//! （规范印刷只有 10 字节，属排版截断）。

use super::*;
use pdf_engine::core::reader::image::{ImageError, Jbig2Finding, Jbig2SegmentKind};

/// Example 1 图像 XObject 流（页信息段 + 立即文本区域段），72 字节。
const EX1_PAGE_STREAM: &[u8] = &[
    0x00, 0x00, 0x00, 0x01, 0x30, 0x00, 0x01, 0x00, 0x00, 0x00, 0x13, 0x00, 0x00, 0x00, 0x34, 0x00,
    0x00, 0x00, 0x42, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x02, 0x06, 0x20, 0x00, 0x01, 0x00, 0x00, 0x00, 0x1E, 0x00, 0x00, 0x00, 0x34, 0x00, 0x00,
    0x00, 0x42, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x10, 0x00, 0x00, 0x00,
    0x02, 0x31, 0xDB, 0x51, 0xCE, 0x51, 0xFF, 0xAC,
];

/// Example 1 全局段流：符号字典段（页关联 0，载荷 50 字节），61 字节。
const EX1_GLOBALS_STREAM: &[u8] = &[
    0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x32, 0x00, 0x00, 0x03, 0xFF, 0xFD,
    0xFF, 0x02, 0xFE, 0xFE, 0xFE, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x2A, 0xE2, 0x25,
    0xAE, 0xA9, 0xA5, 0xA5, 0x38, 0xB4, 0xD9, 0x99, 0x9C, 0x5C, 0x8E, 0x56, 0xEF, 0x0F, 0x87, 0x27,
    0xF2, 0xB5, 0x3D, 0x4E, 0x37, 0xEF, 0x79, 0x5C, 0xC5, 0x50, 0x6D, 0xFF, 0xAC,
];

/// Example 2 的页尾段（49）与文件尾段（51，补全为 11 字节），共 22 字节。
const EX2_TAIL_SEGMENTS: &[u8] = &[
    0x00, 0x00, 0x00, 0x03, 0x31, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04, 0x33,
    0x01, 0x00, 0x00, 0x00, 0x00, 0x00,
];

/// Example 2 文件头（T.88 §6.2：签名 + 旗标 0x01 + 页数 1，共 13 字节）。
const EX2_FILE_HEADER: &[u8] = &[
    0x97, 0x4A, 0x42, 0x32, 0x0D, 0x0A, 0x1A, 0x0A, 0x01, 0x00, 0x00, 0x00, 0x01,
];

/// 把字节流排成每行 64 个十六进制字符的 ASCIIHex 文本（不含 `>`）。
fn hex_lines(data: &[u8]) -> String {
    let hex: String = data.iter().map(|byte| format!("{byte:02X}")).collect();
    hex.as_bytes()
        .chunks(64)
        .map(|chunk| String::from_utf8_lossy(chunk).into_owned())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Example 1 的最小 PDF：对象 5 为 `/Filter [ /ASCIIHexDecode /JBIG2Decode ]`、
/// `/DecodeParms [ null << /JBIG2Globals 6 0 R >> ]` 的图像 XObject，对象 6 为
/// 全局段流；对象 4 留空位，与规范印刷的对象号一致。
fn build_example1_pdf() -> Vec<u8> {
    let image_content = format!("{}\n>", hex_lines(EX1_PAGE_STREAM));
    let globals_content = format!("{}\n>", hex_lines(EX1_GLOBALS_STREAM));
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let off1 = pdf.len();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    let off2 = pdf.len();
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");
    let off3 = pdf.len();
    pdf.extend_from_slice(
        b"3 0 obj\n<< /Type /Page /Parent 2 0 R /Resources << /XObject << /Im1 5 0 R >> >> >>\nendobj\n",
    );
    let off5 = pdf.len();
    pdf.extend_from_slice(
        format!(
            "5 0 obj\n<< /Type /XObject /Subtype /Image /Width 52 /Height 66 \
             /ColorSpace /DeviceGray /BitsPerComponent 1 \
             /Filter [ /ASCIIHexDecode /JBIG2Decode ] \
             /DecodeParms [ null << /JBIG2Globals 6 0 R >> ] /Length {} >>\nstream\n",
            image_content.len()
        )
        .as_bytes(),
    );
    pdf.extend_from_slice(image_content.as_bytes());
    pdf.extend_from_slice(b"\nendstream\nendobj\n");
    let off6 = pdf.len();
    pdf.extend_from_slice(
        format!(
            "6 0 obj\n<< /Filter /ASCIIHexDecode /Length {} >>\nstream\n",
            globals_content.len()
        )
        .as_bytes(),
    );
    pdf.extend_from_slice(globals_content.as_bytes());
    pdf.extend_from_slice(b"\nendstream\nendobj\n");
    let xref_offset = pdf.len();
    pdf.extend_from_slice(b"xref\n0 7\n0000000000 65535 f \n");
    for offset in [off1, off2, off3] {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    for offset in [off5, off6] {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!("trailer\n<< /Size 7 /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n").as_bytes(),
    );
    pdf
}

/// Example 1 原样 PDF：`DecodeParms` 数组的 null 占位不影响
/// `/JBIG2Globals 6 0 R` 的解析，页面信息段几何经公开 API 暴露。
#[test]
fn jbig2_spec_example1_pdf_resolves_globals_through_decode_parms_array() {
    let doc = parse_buf(&build_example1_pdf());
    let image = doc
        .page_image(0, "Im1")
        .expect("page image lookup")
        .expect("Im1 exists");
    // ASCIIHex 步骤照常解码；JBIG2 步骤透传原始段字节。
    assert_eq!(
        image.filters,
        vec!["ASCIIHexDecode".to_string(), "JBIG2Decode".to_string()]
    );
    assert_eq!(image.data, EX1_PAGE_STREAM);
    let jbig2 = image.jbig2.as_ref().expect("jbig2 view present");
    assert_eq!(jbig2.globals_reference, Some((6, 0)));
    let info = jbig2.page_info().expect("page information parsed");
    assert_eq!((info.width, info.height), (52, 66));
    assert_eq!(info.combination_operator_name(), "OR");
    let kinds: Vec<Jbig2SegmentKind> = jbig2.page.segments.iter().map(|s| s.kind).collect();
    assert_eq!(
        kinds,
        vec![
            Jbig2SegmentKind::PageInformation,
            Jbig2SegmentKind::ImmediateTextRegion,
        ]
    );
    let globals = jbig2.globals.as_ref().expect("globals stream resolved");
    assert_eq!(globals.segments.len(), 1);
    assert_eq!(globals.segments[0].kind, Jbig2SegmentKind::SymbolDictionary);
    assert!(globals.segments[0].is_global());
    // 文本区域段的引用由全局段流满足：无任何一致性发现。
    assert_eq!(jbig2.page.segments[1].referred_to_segments, vec![0]);
    assert!(jbig2.findings.is_empty());
    assert!(jbig2.page.findings.is_empty());
    assert_eq!(jbig2.global_segment_count(), 1);
    assert_eq!(jbig2.page_segment_count(), 2);
}

/// Example 2（完整 JBIG2 文件流混置进图像 XObject，非 PDF 组织）：
/// 文件头、页尾段、文件尾段与内嵌全局段全部「存在但上报」。
#[test]
fn jbig2_full_file_stream_reports_forbidden_segments_as_findings() {
    let mut data = EX2_FILE_HEADER.to_vec();
    data.extend_from_slice(EX1_GLOBALS_STREAM);
    data.extend_from_slice(EX1_PAGE_STREAM);
    data.extend_from_slice(EX2_TAIL_SEGMENTS);
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let off1 = pdf.len();
    pdf.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    let off2 = pdf.len();
    pdf.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");
    let off3 = pdf.len();
    pdf.extend_from_slice(
        b"3 0 obj\n<< /Type /Page /Parent 2 0 R /Resources << /XObject << /Im1 4 0 R >> >> >>\nendobj\n",
    );
    let off4 = pdf.len();
    pdf.extend_from_slice(
        format!(
            "4 0 obj\n<< /Type /XObject /Subtype /Image /Width 52 /Height 66 \
             /ColorSpace /DeviceGray /BitsPerComponent 1 /Filter /JBIG2Decode /Length {} >>\nstream\n",
            data.len()
        )
        .as_bytes(),
    );
    pdf.extend_from_slice(&data);
    pdf.extend_from_slice(b"\nendstream\nendobj\n");
    let xref_offset = pdf.len();
    pdf.extend_from_slice(b"xref\n0 5\n0000000000 65535 f \n");
    for offset in [off1, off2, off3, off4] {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!("trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n").as_bytes(),
    );

    let doc = parse_buf(&pdf);
    let image = doc
        .page_image(0, "Im1")
        .expect("page image lookup")
        .expect("Im1 exists");
    let jbig2 = image.jbig2.as_ref().expect("jbig2 view present");
    // 段链 5 段：符号字典 0、页信息 1、文本区域 2、页尾 3、文件尾 4。
    let kinds: Vec<Jbig2SegmentKind> = jbig2.page.segments.iter().map(|s| s.kind).collect();
    assert_eq!(
        kinds,
        vec![
            Jbig2SegmentKind::SymbolDictionary,
            Jbig2SegmentKind::PageInformation,
            Jbig2SegmentKind::ImmediateTextRegion,
            Jbig2SegmentKind::EndOfPage,
            Jbig2SegmentKind::EndOfFile,
        ]
    );
    assert!(
        jbig2
            .page
            .findings
            .contains(&Jbig2Finding::FileHeaderPresent {
                position: 0,
                flags: Some(0x01),
                random_access: Some(false),
                declared_pages: Some(1),
            }),
        "文件头应识别并记录：{:?}",
        jbig2.page.findings
    );
    assert!(
        jbig2
            .page
            .findings
            .contains(&Jbig2Finding::EndOfPageSegmentPresent { segment_number: 3 })
    );
    assert!(
        jbig2
            .page
            .findings
            .contains(&Jbig2Finding::EndOfFileSegmentPresent { segment_number: 4 })
    );
    // 页关联 0 的段（符号字典与补全后的文件尾段）混置在图像流内。
    assert!(
        jbig2
            .findings
            .contains(&Jbig2Finding::GlobalSegmentInImageStream { segment_number: 0 })
    );
    assert!(
        jbig2
            .findings
            .contains(&Jbig2Finding::GlobalSegmentInImageStream { segment_number: 4 })
    );
    // 文件头旗标 0x01 → 顺序式（randomAccess=false）、页数 1。
    assert_eq!(jbig2.global_segment_count(), 2);
    assert_eq!(jbig2.page_segment_count(), 3);
    let info = jbig2.page_info().expect("page information parsed");
    assert_eq!((info.width, info.height), (52, 66));
    // 语法层不承诺像素：samples() 返回类型化 Unsupported。
    let error = image
        .samples()
        .expect_err("JBIG2 samples are not decoded by the syntax layer");
    assert!(matches!(error, ImageError::Unsupported(_)), "{error}");
}
