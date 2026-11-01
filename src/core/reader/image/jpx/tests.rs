//! JPX 语法层解析单元测试：全部手写字节向量，不依赖外部夹具文件。

use super::*;

/// 12 字节 JP2 签名框。
fn jp2_signature() -> Vec<u8> {
    vec![
        0x00, 0x00, 0x00, 0x0C, b'j', b'P', b' ', b' ', 0x0D, 0x0A, 0x87, 0x0A,
    ]
}

/// 长度字段自洽的普通框。
fn box_with(kind: &[u8; 4], content: &[u8]) -> Vec<u8> {
    let mut bytes = (content.len() as u32 + 8).to_be_bytes().to_vec();
    bytes.extend_from_slice(kind);
    bytes.extend_from_slice(content);
    bytes
}

/// 可声明任意总长度的框（用于构造畸形长度）。
fn box_with_len(kind: &[u8; 4], declared_len: u32, content: &[u8]) -> Vec<u8> {
    let mut bytes = declared_len.to_be_bytes().to_vec();
    bytes.extend_from_slice(kind);
    bytes.extend_from_slice(content);
    bytes
}

fn ihdr_content(height: u32, width: u32, count: u16, bpc: u8) -> Vec<u8> {
    let mut content = Vec::new();
    content.extend_from_slice(&height.to_be_bytes());
    content.extend_from_slice(&width.to_be_bytes());
    content.extend_from_slice(&count.to_be_bytes());
    content.extend_from_slice(&[bpc, 7, 0, 0]);
    content
}

fn colr_enumerated(code: u32) -> Vec<u8> {
    let mut content = vec![1, 0, 1];
    content.extend_from_slice(&code.to_be_bytes());
    content
}

fn jp2_with(content: &[u8]) -> Vec<u8> {
    let mut bytes = jp2_signature();
    bytes.extend_from_slice(content);
    bytes
}

fn soc() -> Vec<u8> {
    vec![0xFF, 0x4F]
}

/// 完整定长的 SIZ 标记段（内容 = 36 字节定长头 + 每分量 3 字节）。
/// `geometry` 依序为 Xsiz/Ysiz/XOsiz/YOsiz/XTsiz/YTsiz/XTOsiz/YTOsiz。
fn siz(geometry: [u32; 8], components: &[(u8, u8, u8)]) -> Vec<u8> {
    let mut content = Vec::new();
    content.extend_from_slice(&0u16.to_be_bytes());
    for value in geometry {
        content.extend_from_slice(&value.to_be_bytes());
    }
    content.extend_from_slice(&(components.len() as u16).to_be_bytes());
    for (ssiz, xsiz, ysiz) in components {
        content.extend_from_slice(&[*ssiz, *xsiz, *ysiz]);
    }
    let mut bytes = vec![0xFF, 0x51];
    bytes.extend_from_slice(&((content.len() + 2) as u16).to_be_bytes());
    bytes.extend_from_slice(&content);
    bytes
}

/// 头 36 字节合法、但声明分量多于实际三元组数据的 SIZ（畸形降级用）。
fn siz_truncated_components(
    xsiz: u32,
    ysiz: u32,
    declared: u16,
    triples: &[(u8, u8, u8)],
) -> Vec<u8> {
    let mut content = Vec::new();
    content.extend_from_slice(&0u16.to_be_bytes());
    for value in [xsiz, ysiz, 0u32, 0u32, xsiz, ysiz, 0u32, 0u32] {
        content.extend_from_slice(&value.to_be_bytes());
    }
    content.extend_from_slice(&declared.to_be_bytes());
    for (ssiz, xr, yr) in triples {
        content.extend_from_slice(&[*ssiz, *xr, *yr]);
    }
    let mut bytes = vec![0xFF, 0x51];
    bytes.extend_from_slice(&((content.len() + 2) as u16).to_be_bytes());
    bytes.extend_from_slice(&content);
    bytes
}

fn marker_segment(code: u8, content: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0xFF, code];
    bytes.extend_from_slice(&((content.len() + 2) as u16).to_be_bytes());
    bytes.extend_from_slice(content);
    bytes
}

#[test]
fn jp2_ihdr_and_enumerated_colr_yield_geometry_and_colorspace() {
    let ihdr = box_with(b"ihdr", &ihdr_content(6, 8, 3, 7));
    let colr = box_with(b"colr", &colr_enumerated(16));
    let jp2h = box_with(b"jp2h", &[ihdr.as_slice(), colr.as_slice()].concat());
    let data = jp2_with(&jp2h);
    let header = parse_jpx_header(&data).expect("parsable jp2 header");
    assert_eq!(header.container, JpxContainer::Jp2File);
    assert_eq!((header.width, header.height), (8, 6));
    assert_eq!(header.component_count(), 3);
    assert_eq!(header.bits_per_component(), Some(8));
    assert_eq!(
        header.color_space,
        Some(JpxColorSpace::Enumerated {
            method: 1,
            precedence: 0,
            approximation: 1,
            code: 16,
        })
    );
    assert_eq!(header.color_space.unwrap().enumerated_name(), Some("sRGB"));
    assert_eq!(header.pdf_color_space_hint(), Some("DeviceRGB"));
}

#[test]
fn colr_icc_method_keeps_profile_length_only() {
    let mut colr_content = vec![2, 0, 0];
    colr_content.extend_from_slice(&[0xAA; 12]);
    let ihdr = box_with(b"ihdr", &ihdr_content(4, 4, 1, 0x87));
    let colr = box_with(b"colr", &colr_content);
    let jp2h = box_with(b"jp2h", &[ihdr.as_slice(), colr.as_slice()].concat());
    let data = jp2_with(&jp2h);
    let header = parse_jpx_header(&data).expect("parsable jp2 header");
    assert_eq!(
        header.color_space,
        Some(JpxColorSpace::IccProfile {
            method: 2,
            precedence: 0,
            approximation: 0,
            profile_len: 12,
        })
    );
    assert_eq!(header.color_space.unwrap().enumerated_name(), None);
    // 无枚举空间时按分量数回退（§7.4.9 Table 89）。
    assert_eq!(header.pdf_color_space_hint(), Some("DeviceGray"));
    // BPC 0x87：0x80 置位为有符号，低 7 位加一为 8 位。
    let component = header.components[0];
    assert_eq!(
        component,
        JpxComponent {
            depth: 8,
            signed: true,
            x_subsampling: 1,
            y_subsampling: 1,
        }
    );
}

#[test]
fn raw_codestream_siz_extracts_all_geometry_fields() {
    let mut data = soc();
    data.extend_from_slice(&siz(
        [100, 50, 3, 4, 32, 16, 1, 2],
        &[(7, 1, 1), (0x8F, 2, 4)],
    ));
    let header = parse_jpx_header(&data).expect("parsable raw codestream");
    assert_eq!(header.container, JpxContainer::RawCodestream);
    assert_eq!((header.width, header.height), (97, 46));
    assert_eq!(header.reference_offset, Some((3, 4)));
    assert_eq!(header.tile_width, Some(32));
    assert_eq!(header.tile_height, Some(16));
    assert_eq!(header.tile_grid_offset, Some((1, 2)));
    assert_eq!(header.component_count(), 2);
    assert_eq!(header.bits_per_component(), Some(16));
    assert_eq!(
        header.components[1],
        JpxComponent {
            depth: 16,
            signed: true,
            x_subsampling: 2,
            y_subsampling: 4,
        }
    );
    assert_eq!(header.color_space, None);
    // 两分量：既非 Gray 也非 RGB/CMYK，无提示。
    assert_eq!(header.pdf_color_space_hint(), None);
}

#[test]
fn jp2c_codestream_siz_overrides_ihdr_but_colr_survives() {
    let ihdr = box_with(b"ihdr", &ihdr_content(6, 8, 3, 7));
    let colr = box_with(b"colr", &colr_enumerated(16));
    let jp2h = box_with(b"jp2h", &[ihdr.as_slice(), colr.as_slice()].concat());
    let mut codestream = soc();
    codestream.extend_from_slice(&siz([10, 4, 0, 0, 10, 4, 0, 0], &[(7, 1, 1)]));
    let jp2c = box_with(b"jp2c", &codestream);
    let data = jp2_with(&[jp2h, jp2c].concat());
    let header = parse_jpx_header(&data).expect("parsable jp2 header");
    // SIZ 几何（10×4）优先于 ihdr 摘要（8×6）。
    assert_eq!((header.width, header.height), (10, 4));
    assert_eq!(header.reference_offset, Some((0, 0)));
    assert_eq!(header.component_count(), 1);
    assert_eq!(header.color_space.unwrap().enumerated_name(), Some("sRGB"));
    // 枚举 sRGB 优先于 1 分量的 DeviceGray 回退。
    assert_eq!(header.pdf_color_space_hint(), Some("DeviceRGB"));
}

#[test]
fn truncated_ihdr_without_codestream_returns_none() {
    let ihdr = box_with(b"ihdr", &[0, 0, 0, 4, 0, 0, 0, 4, 0, 1]);
    let jp2h = box_with(b"jp2h", &ihdr);
    let data = jp2_with(&jp2h);
    assert_eq!(parse_jpx_header(&data), None);
}

#[test]
fn length_bearing_unknown_markers_before_siz_are_skipped() {
    let mut data = soc();
    data.extend_from_slice(&marker_segment(0x50, &[0, 0, 0, 0x02])); // CAP
    data.extend_from_slice(&marker_segment(0x64, b"comment payload")); // COM
    data.extend_from_slice(&siz([2, 2, 0, 0, 2, 2, 0, 0], &[(7, 1, 1)]));
    let header = parse_jpx_header(&data).expect("markers skipped, siz parsed");
    assert_eq!((header.width, header.height), (2, 2));
}

#[test]
fn inconsistent_box_length_stops_walk_but_keeps_metadata() {
    let ihdr = box_with(b"ihdr", &ihdr_content(6, 8, 3, 7));
    let colr = box_with(b"colr", &colr_enumerated(16));
    let jp2h = box_with(b"jp2h", &[ihdr.as_slice(), colr.as_slice()].concat());
    // 长度字段（4）小于 8 字节框头的畸形框：停止遍历，但前面的
    // jp2h 元数据保留。
    let broken = box_with_len(b"junk", 4, b"whatever");
    let data = jp2_with(&[jp2h, broken].concat());
    let header = parse_jpx_header(&data).expect("metadata kept");
    assert_eq!((header.width, header.height), (8, 6));
    assert_eq!(header.color_space.unwrap().enumerated_name(), Some("sRGB"));
}

#[test]
fn truncated_component_table_keeps_geometry_with_partial_components() {
    // 声明 3 个分量但只给出 1 个三元组。
    let mut data = soc();
    data.extend_from_slice(&siz_truncated_components(10, 10, 3, &[(7, 1, 1)]));
    let header = parse_jpx_header(&data);
    // 「宽进」：截断的分量表按可读个数降级保留几何信息。
    let header = header.expect("geometry still parsed from lenient siz");
    assert_eq!((header.width, header.height), (10, 10));
    assert_eq!(header.component_count(), 1);
    assert_eq!(header.bits_per_component(), Some(8));
}

#[test]
fn non_jpx_inputs_return_none() {
    assert_eq!(parse_jpx_header(&[]), None);
    assert_eq!(parse_jpx_header(&[0xFF]), None);
    // JPEG（DCTDecode）载荷不是 JPX。
    let mut jpeg = vec![0xFF, 0xD8, 0xFF, 0xE0];
    jpeg.extend_from_slice(b"JFIF some jpeg bytes");
    assert_eq!(parse_jpx_header(&jpeg), None);
    // 随机数据。
    assert_eq!(parse_jpx_header(b"not a jpx payload at all"), None);
    // SOC 后直接是 SOD：主头不完整。
    let mut sod_first = soc();
    sod_first.extend_from_slice(&[0xFF, 0x93]);
    assert_eq!(parse_jpx_header(&sod_first), None);
}
