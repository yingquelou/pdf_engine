//! JBIG2 段语法层解析单元测试：全部手写字节向量（含 §7.4.7 worked-example
//! 的标准字节），不依赖外部夹具文件。

use super::*;

/// 短形式段头：`number | flags | rtscarf | referred(1 字节/个) |
/// 页关联（依 bit `0x40` 取 1 或 4 字节）| u32 声明长度`，随后载荷。
/// 声明长度按载荷实际长度写入，除非测试另行拼接字节。
fn segment(
    number: u32,
    flags: u8,
    rtscarf: u8,
    referred: &[u8],
    page_association: u32,
    payload: &[u8],
) -> Vec<u8> {
    let mut bytes = number.to_be_bytes().to_vec();
    bytes.push(flags);
    bytes.push(rtscarf);
    bytes.extend_from_slice(referred);
    if flags & 0x40 != 0 {
        bytes.extend_from_slice(&page_association.to_be_bytes());
    } else {
        bytes.push(page_association as u8);
    }
    bytes.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    bytes.extend_from_slice(payload);
    bytes
}

/// 页面信息段载荷（§7.4.8，19 字节）：4×u32 + BPC + BPIFlags + BPIExtension。
fn page_info_payload(
    width: u32,
    height: u32,
    x_resolution: u32,
    y_resolution: u32,
    flags: u8,
    bpi_flags: u8,
    bpi_extension: u8,
) -> Vec<u8> {
    let mut payload = Vec::new();
    for value in [width, height, x_resolution, y_resolution] {
        payload.extend_from_slice(&value.to_be_bytes());
    }
    payload.extend_from_slice(&[flags, bpi_flags, bpi_extension]);
    payload
}

/// §7.4.7 worked example（Example 2）的 5 个段：符号字典（全局）、页面
/// 信息、立即文本区域、end-of-page、end-of-file。规范正文印刷的 EOF 段
/// 只有 10 字节，而 §7.2.1 语法要求 11 字节，此处按语法补全最后一个
/// `00`（Example 2 的逐段拆解亦印证该缺失属排版截断）。
fn spec_example_segments() -> Vec<u8> {
    let mut bytes = Vec::new();
    // 段 0：符号字典，页关联 0，载荷 50 字节（data_size 0x32）。
    bytes.extend_from_slice(&[
        0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x32, 0x00, 0x00, 0x03, 0xFF,
        0xFD, 0xFF, 0x02, 0xFE, 0xFE, 0xFE, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x2A,
        0xE2, 0x25, 0xAE, 0xA9, 0xA5, 0xA5, 0x38, 0xB4, 0xD9, 0x99, 0x9C, 0x5C, 0x8E, 0x56, 0xEF,
        0x0F, 0x87, 0x27, 0xF2, 0xB5, 0x3D, 0x4E, 0x37, 0xEF, 0x79, 0x5C, 0xC5, 0x50, 0x6D, 0xFF,
        0xAC,
    ]);
    // 段 1：页面信息，页关联 1，52×66。
    bytes.extend_from_slice(&[
        0x00, 0x00, 0x00, 0x01, 0x30, 0x00, 0x01, 0x00, 0x00, 0x00, 0x13, 0x00, 0x00, 0x00, 0x34,
        0x00, 0x00, 0x00, 0x42, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x40, 0x00, 0x00,
    ]);
    // 段 2：立即文本区域，引用段 0，页关联 1，载荷 30 字节。
    bytes.extend_from_slice(&[
        0x00, 0x00, 0x00, 0x02, 0x06, 0x20, 0x00, 0x01, 0x00, 0x00, 0x00, 0x1E, 0x00, 0x00, 0x00,
        0x34, 0x00, 0x00, 0x00, 0x42, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00,
        0x10, 0x00, 0x00, 0x00, 0x02, 0x31, 0xDB, 0x51, 0xCE, 0x51, 0xFF, 0xAC,
    ]);
    // 段 3：end-of-page（PDF 禁用）。
    bytes.extend_from_slice(&[
        0x00, 0x00, 0x00, 0x03, 0x31, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00,
    ]);
    // 段 4：end-of-file（PDF 禁用；按 §7.2.1 补全为 11 字节）。
    bytes.extend_from_slice(&[
        0x00, 0x00, 0x00, 0x04, 0x33, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00,
    ]);
    bytes
}

#[test]
fn segment_chain_with_short_headers_parses_all_entries() {
    let mut data = Vec::new();
    data.extend(segment(0, 0x00, 0x00, &[], 0, &[]));
    data.extend(segment(
        1,
        0x30,
        0x00,
        &[],
        1,
        &page_info_payload(52, 66, 0, 0, 0x00, 0x00, 0x00),
    ));
    data.extend(segment(2, 0x33, 0x00, &[], 1, b"region"));
    let stream = parse_jbig2_segments(&data).expect("chain should parse");
    assert_eq!(stream.segments.len(), 3);
    assert_eq!(stream.segments[0].kind, Jbig2SegmentKind::SymbolDictionary);
    assert_eq!(stream.segments[1].kind, Jbig2SegmentKind::PageInformation);
    assert_eq!(stream.segments[2].kind, Jbig2SegmentKind::EndOfFile);
    // 短形式头：1 字节页关联、无引用表，恒为最小 11 字节（载荷另计）。
    for segment in &stream.segments {
        assert_eq!(segment.header_len, 11);
        assert!(!segment.page_association_long);
    }
    assert_eq!(
        stream.segments.iter().map(|s| s.number).collect::<Vec<_>>(),
        vec![0, 1, 2]
    );
    assert_eq!(stream.global_segment_count(), 1);
    assert_eq!(stream.page_segment_count(), 2);
    // end-of-file「存在但禁用」必须成为发现，段本身也不得丢弃。
    assert!(
        stream
            .findings
            .contains(&Jbig2Finding::EndOfFileSegmentPresent { segment_number: 2 })
    );
    assert_eq!(stream.segments[2].data, b"region");
}

#[test]
fn long_form_page_association_is_read_as_four_bytes() {
    // 旗标 bit 0x40：页关联取 4 字节大端。
    let data = segment(1, 0x30 | 0x40, 0x00, &[], 0x0102_0304, &[]);
    let stream = parse_jbig2_segments(&data).expect("segment should parse");
    let segment = &stream.segments[0];
    assert!(segment.page_association_long);
    assert_eq!(segment.page_association, 0x0102_0304);
    assert_eq!(segment.header_len, 14);
    assert_eq!(
        stream.unexpected_page_associations(),
        vec![(1, 0x0102_0304)]
    );
}

#[test]
fn truncated_segment_header_returns_error_without_panicking() {
    // 空输入：干净的空段链。
    let stream = parse_jbig2_segments(&[]).expect("empty stream parses");
    assert!(stream.segments.is_empty());
    // 段头只剩 10 字节：返回类型化截断错误而不是 panic。
    let mut data = segment(0, 0x00, 0x00, &[], 0, b"x");
    data.extend_from_slice(&[0u8; 10]);
    let error = parse_jbig2_segments(&data).expect_err("short tail must error");
    assert_eq!(
        error,
        Jbig2Error::TruncatedSegmentHeader {
            position: 12,
            remaining: 10,
            required: 11,
        }
    );
    // 3 个引用段 + 页关联 + u32 长度共需 14 字节的段头，只剩 11 字节：
    // required 反映真实需求而不是最小 11。
    let mut partial = 0u32.to_be_bytes().to_vec();
    partial.push(0x00);
    partial.push(0x60); // rtscarf：TP=3，3 个 1 字节引用
    partial.extend_from_slice(&[0x00, 0x01, 0xAA, 0xBB, 0xCC]);
    let error = parse_jbig2_segments(&partial).expect_err("missing referred bytes");
    assert_eq!(
        error,
        Jbig2Error::TruncatedSegmentHeader {
            position: 0,
            remaining: 11,
            required: 14,
        }
    );
    // 过长的头（TP=7 计数 0x1FFFFFFF → 保留位图 256MB）：不分配、直接报错。
    let absurd = [
        0x00, 0x00, 0x00, 0x01, 0x30, 0xFF, 0xFF, 0xFF, 0xFF, 0x00, 0x00,
    ];
    parse_jbig2_segments(&absurd).expect_err("over-long header must error");
}

#[test]
fn page_information_segment_yields_width_and_height() {
    // BPC 0x48：无损=0、调精=0、默认像素=0、算子=(0x48>>3)&3=1=XNOR、
    // bit6=1 → 合成算子覆盖；条带：BPIFlags 0x84 + 0x01 → 高度 0x401。
    let payload = page_info_payload(52, 66, 300, 300, 0x48, 0x84, 0x01);
    let data = segment(1, 0x30, 0x00, &[], 1, &payload);
    let stream = parse_jbig2_segments(&data).expect("page info should parse");
    let info = stream.page_info().expect("page information present");
    assert_eq!((info.width, info.height), (52, 66));
    assert_eq!((info.x_resolution, info.y_resolution), (300, 300));
    assert_eq!(info.combination_operator, 1);
    assert_eq!(info.combination_operator_name(), "XNOR");
    assert!(info.combination_operator_override);
    assert!(!info.lossless && !info.refinement && !info.colored);
    assert_eq!(info.default_pixel, 0);
    assert!(!info.requires_buffer);
    assert!(info.striped);
    assert_eq!(info.stripe_height, Some(0x401));
    assert!(stream.findings.is_empty());
}

#[test]
fn page_information_shorter_than_19_bytes_records_finding() {
    let mut payload = page_info_payload(8, 8, 0, 0, 0x00, 0x00, 0x00);
    payload.truncate(18);
    let data = segment(1, 0x30, 0x00, &[], 1, &payload);
    let stream = parse_jbig2_segments(&data).expect("18-byte page info still parses");
    assert!(
        stream
            .findings
            .contains(&Jbig2Finding::PageInformationSegmentTooShort {
                segment_number: 1,
                available: 18,
            })
    );
    let info = stream.page_info().expect("core fields still readable");
    assert_eq!((info.width, info.height), (8, 8));
    assert_eq!(info.bpi_extension, None);
    assert!(!info.striped);
}

#[test]
fn segment_data_truncated_stops_cleanly_with_finding() {
    let mut data = 0u32.to_be_bytes().to_vec();
    data.push(0x00);
    data.push(0x00);
    data.push(0x00); // 页关联 0
    data.extend_from_slice(&50u32.to_be_bytes()); // 声明 50 字节
    data.extend_from_slice(b"only ten bytes!!"); // 实际给 16 字节
    let stream = parse_jbig2_segments(&data).expect("truncated payload is a finding, not error");
    assert_eq!(stream.segments.len(), 1);
    assert_eq!(stream.segments[0].data_size, 50);
    assert_eq!(stream.segments[0].data.len(), 16);
    assert_eq!(
        stream.findings,
        vec![Jbig2Finding::SegmentDataTruncated {
            segment_number: 0,
            declared: 50,
            available: 16,
        }]
    );
}

#[test]
fn unspecified_segment_length_stops_with_finding() {
    let mut data = 0u32.to_be_bytes().to_vec();
    data.push(0x00);
    data.push(0x00);
    data.push(0x00);
    data.extend_from_slice(&u32::MAX.to_be_bytes()); // 0xFFFFFFFF 未指定长度
    data.extend_from_slice(b"tail bytes");
    let stream = parse_jbig2_segments(&data).expect("unspecified length stops cleanly");
    assert_eq!(stream.segments.len(), 1);
    assert_eq!(
        stream.findings,
        vec![Jbig2Finding::SegmentLengthUnspecified { segment_number: 0 }]
    );
}

#[test]
fn extended_referred_count_reads_retained_bitmap_bytes() {
    // TP=7 长形式：u32（0xE0000009，低 29 位 = 9）+ ceil(9/8)=2 字节保留
    // 位图 + 9 个 1 字节引用（段号 100 ≤ 256）。
    let mut data = 100u32.to_be_bytes().to_vec();
    data.push(0x00);
    data.extend_from_slice(&0xE000_0009u32.to_be_bytes());
    data.extend_from_slice(&[0xAB, 0xCD]);
    data.extend_from_slice(&[0, 1, 2, 3, 4, 5, 6, 7, 8]);
    data.push(0x01); // 页关联 1
    data.extend_from_slice(&0u32.to_be_bytes());
    let stream = parse_jbig2_segments(&data).expect("long-form referred header should parse");
    let segment = &stream.segments[0];
    assert!(segment.referred_count_extended);
    assert_eq!(
        segment.referred_to_segments,
        vec![0, 1, 2, 3, 4, 5, 6, 7, 8]
    );
    assert_eq!(segment.retained_bytes, vec![0xAB, 0xCD]);
    assert_eq!(segment.retained_bits_indicator, 0xAB);
    // 头长 = 5（号+旗标）+ 4（重叠的 32 位个数字段）+ 2（保留位图）
    // + 9（引用表）+ 1（页关联）+ 4（长度）= 25。
    assert_eq!(segment.header_len, 25);
}

#[test]
fn file_header_is_detected_and_skipped() {
    let mut data = vec![0x97, 0x4A, 0x42, 0x32, 0x0D, 0x0A, 0x1A, 0x0A, 0x01];
    data.extend_from_slice(&1u32.to_be_bytes()); // 顺序组织：4 字节页数 = 1
    data.extend(segment(0, 0x00, 0x00, &[], 0, b"sym"));
    let stream = parse_jbig2_segments(&data).expect("file header then one segment");
    assert_eq!(stream.segments.len(), 1);
    assert_eq!(
        stream.findings,
        vec![Jbig2Finding::FileHeaderPresent {
            position: 0,
            flags: Some(0x01),
            random_access: Some(false),
            declared_pages: Some(1),
        }]
    );
}

#[test]
fn spec_worked_example_stream_splits_global_page_and_forbidden_segments() {
    let data = spec_example_segments();
    let stream = parse_jbig2_segments(&data).expect("spec stream should parse");
    assert_eq!(stream.segments.len(), 5);
    let kinds: Vec<_> = stream.segments.iter().map(|s| s.kind).collect();
    use Jbig2SegmentKind as K;
    assert_eq!(
        kinds,
        vec![
            K::SymbolDictionary,
            K::PageInformation,
            K::ImmediateTextRegion,
            K::EndOfPage,
            K::EndOfFile,
        ]
    );
    // 全局段 vs 页段：符号字典页关联 0；Example 2 印刷的 end-of-file 段
    // 页关联字段同样为 0（其载荷为空），故按原始字节计入全局段。
    assert_eq!(stream.global_segment_count(), 2);
    assert_eq!(stream.page_segment_count(), 3);
    assert!(stream.segments[0].is_global());
    assert!(stream.unexpected_page_associations().is_empty());
    // 文本区域段以 1 字节引用表指向符号字典（rtscarf 0x20 → TP=1）。
    let text = &stream.segments[2];
    assert_eq!(text.referred_to_segments, vec![0]);
    assert_eq!(text.retained_bits_indicator, 0);
    assert_eq!(text.data_size, 0x1E);
    assert_eq!(text.header_len, 12);
    // 页面信息段元数据（Example 2：52×66，BPC 0x40，无条带）。
    let info = stream.page_info().expect("page information parsed");
    assert_eq!((info.width, info.height), (52, 66));
    assert_eq!((info.x_resolution, info.y_resolution), (0, 0));
    assert!(info.combination_operator_override);
    assert!(!info.striped);
    // end-of-page / end-of-file：报告为「存在但禁用」，段本身保留。
    assert!(kinds[3].is_forbidden_in_pdf() && kinds[4].is_forbidden_in_pdf());
    assert!(
        stream
            .findings
            .contains(&Jbig2Finding::EndOfPageSegmentPresent { segment_number: 3 })
    );
    assert!(
        stream
            .findings
            .contains(&Jbig2Finding::EndOfFileSegmentPresent { segment_number: 4 })
    );
    // 符号字典段头 11 字节、data_size 0x32 = 50 字节载荷。
    let symbol = &stream.segments[0];
    assert_eq!(symbol.header_len, 11);
    assert_eq!(symbol.data_size, 50);
    assert_eq!(symbol.data.len(), 50);
}
