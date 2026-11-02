//! §7.4.9 JPXDecode：语法层元数据——头部语法解析结果挂
//! [`ImageModel::jpx`]、`/BPC` 忽略、`/ColorSpace` 回退与过滤链透传。

use super::support::{dict, doc_with, document_with_page, image_dict, stream};
use crate::core::reader::color_space::ColorSpaceModel;
use crate::core::reader::image::{ImageError, JpxColorSpace};
use crate::core::types::{Array, Dict, Value};

/// 手写最小 JP2 文件：签名框 + jp2h{ihdr, colr(sRGB)}，可选 jp2c 码流。
/// `ihdr` 声明 `height`×`width`、3 分量、8 位无符号（BPC 7）。
fn sample_jp2(width: u32, height: u32) -> Vec<u8> {
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&3u16.to_be_bytes());
    ihdr.extend_from_slice(&[7u8, 7, 0, 0]);
    let ihdr = jp2_box(b"ihdr", &ihdr);
    let mut colr = vec![1u8, 0, 1];
    colr.extend_from_slice(&16u32.to_be_bytes()); // EnumCS 16 = sRGB
    let colr = jp2_box(b"colr", &colr);
    let jp2h = jp2_box(b"jp2h", &[ihdr, colr].concat());
    let mut bytes = vec![
        0x00, 0x00, 0x00, 0x0C, b'j', b'P', b' ', b' ', 0x0D, 0x0A, 0x87, 0x0A,
    ];
    bytes.extend_from_slice(&jp2h);
    bytes
}

fn jp2_box(kind: &[u8; 4], content: &[u8]) -> Vec<u8> {
    let mut bytes = (content.len() as u32 + 8).to_be_bytes().to_vec();
    bytes.extend_from_slice(kind);
    bytes.extend_from_slice(content);
    bytes
}

fn jpx_image_dict(width: u32, height: u32) -> Dict {
    dict(vec![
        ("Type", Value::Name("XObject".into())),
        ("Subtype", Value::Name("Image".into())),
        ("Width", Value::Integer(width as i64)),
        ("Height", Value::Integer(height as i64)),
        ("Filter", Value::Name("JPXDecode".into())),
    ])
}

#[test]
fn jpx_image_derives_bits_and_color_space_from_header() {
    let jp2 = sample_jp2(8, 6);
    let doc = doc_with(vec![(10, stream(jpx_image_dict(8, 6), jp2.clone()))]);
    let image = doc
        .image(&Value::Ref(10, 0))
        .expect("jpx image without /BPC and /ColorSpace should parse");
    assert_eq!(image.filters, vec!["JPXDecode".to_string()]);
    // JPX 步骤透传：data 是原始 JPX 字节而不是样本。
    assert_eq!(image.data, jp2);
    let header = image.jpx.clone().expect("jp2 header should parse");
    assert_eq!((header.width, header.height), (8, 6));
    assert_eq!(header.component_count(), 3);
    assert_eq!(header.bits_per_component(), Some(8));
    // 缺 /BPC：以头部位深为准；缺 /ColorSpace：枚举 sRGB → DeviceRGB。
    assert_eq!(image.bits_per_component, 8);
    assert_eq!(image.color_space, Some(ColorSpaceModel::DeviceRGB));
    assert_eq!(image.component_count(), Some(3));
    assert!(matches!(
        header.color_space,
        Some(JpxColorSpace::Enumerated { code: 16, .. })
    ));
}

#[test]
fn jpx_ignores_declared_bits_and_keeps_declared_color_space() {
    let mut jp2 = sample_jp2(4, 4);
    // 追加一段裸码流 jp2c（SIZ 4×4、8 位单分量），验证 SIZ 优先路径。
    let mut codestream = vec![0xFF, 0x4F, 0xFF, 0x51];
    codestream.extend_from_slice(&41u16.to_be_bytes());
    codestream.extend_from_slice(&0u16.to_be_bytes());
    for value in [4u32, 4, 0, 0, 4, 4, 0, 0] {
        codestream.extend_from_slice(&value.to_be_bytes());
    }
    codestream.extend_from_slice(&1u16.to_be_bytes());
    codestream.extend_from_slice(&[7u8, 1, 1]);
    jp2.extend_from_slice(&jp2_box(b"jp2c", &codestream));

    let doc = doc_with(vec![(
        10,
        stream(
            jpx_image_dict(4, 4)
                .insert("BitsPerComponent", Value::Integer(1))
                .insert("ColorSpace", Value::Name("DeviceGray".into())),
            jp2,
        ),
    )]);
    let image = doc.image(&Value::Ref(10, 0)).expect("jpx image parses");
    // Table 89：/BPC「存在时必须忽略」；/ColorSpace 声明值仍然生效。
    assert_eq!(image.bits_per_component, 8);
    assert_eq!(image.color_space, Some(ColorSpaceModel::DeviceGray));
    let header = image.jpx.expect("codestream parsed");
    assert_eq!((header.width, header.height), (4, 4));
    assert_eq!(header.reference_offset, Some((0, 0)));
    // 声明空间与头部提示互不覆盖：头部仍记录枚举 sRGB。
    assert_eq!(header.pdf_color_space_hint(), Some("DeviceRGB"));
}

#[test]
fn jpx_payload_without_parsable_header_falls_back_to_dict_entries() {
    let doc = doc_with(vec![(
        10,
        stream(
            image_dict(2, 2, "DeviceRGB", 8).insert("Filter", Value::Name("JPX".into())),
            b"definitely not a jpx payload",
        ),
    )]);
    let image = doc
        .image(&Value::Ref(10, 0))
        .expect("unparsable jpx payload must not fail image parsing");
    assert!(image.jpx.is_none());
    assert_eq!(image.bits_per_component, 8);
    assert_eq!(image.color_space, Some(ColorSpaceModel::DeviceRGB));
    // 旧名 /JPX 同样命中透传路径。
    assert_eq!(image.filters, vec!["JPX".to_string()]);
}

#[test]
fn jpx_samples_are_rejected_as_unsupported() {
    let doc = doc_with(vec![(10, stream(jpx_image_dict(8, 6), sample_jp2(8, 6)))]);
    let image = doc.image(&Value::Ref(10, 0)).expect("jpx image parses");
    let error = image.samples().expect_err("jp2c bytes are not raw samples");
    assert!(
        matches!(error, ImageError::Unsupported(_)),
        "unexpected error: {error}"
    );
    assert!(
        error.to_string().contains("JPXDecode"),
        "unexpected error: {error}"
    );
}

#[test]
fn flated_then_jpx_passes_through_the_raw_jpx_bytes() {
    use flate2::write::ZlibEncoder;
    use std::io::Write;
    let jp2 = sample_jp2(8, 6);
    let mut enc = ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    enc.write_all(&jp2).expect("compress");
    let compressed = enc.finish().expect("finish");
    let filter = Value::Array(Array(vec![
        Value::Name("FlateDecode".into()),
        Value::Name("JPXDecode".into()),
    ]));
    let doc = doc_with(vec![(
        10,
        stream(jpx_image_dict(8, 6).insert("Filter", filter), compressed),
    )]);
    let image = doc.image(&Value::Ref(10, 0)).expect("chain parses");
    // 链上 Flate 步骤照常解码，JPX 步骤保留原始字节。
    assert_eq!(image.data, jp2);
    assert_eq!(
        image.filters,
        vec!["FlateDecode".to_string(), "JPXDecode".to_string()]
    );
    assert_eq!(
        image.jpx.map(|header| (header.width, header.height)),
        Some((8, 6))
    );
}

#[test]
fn page_image_exposes_jpx_geometry_through_public_api() {
    let doc = document_with_page(
        Some(Value::Dict(dict(vec![(
            "XObject",
            Value::Dict(dict(vec![("Im1", Value::Ref(10, 0))])),
        )]))),
        vec![(10, stream(jpx_image_dict(10, 5), sample_jp2(10, 5)))],
    );
    let image = doc
        .page_image(0, "Im1")
        .expect("lookup")
        .expect("image exists");
    let header = image.jpx.expect("jp2 header parsed");
    assert_eq!((image.width, image.height), (10, 5));
    assert_eq!((header.width, header.height), (10, 5));
    assert_eq!(header.bits_per_component(), Some(8));
    assert_eq!(image.bits_per_component, 8);
    assert_eq!(image.color_space, Some(ColorSpaceModel::DeviceRGB));
}
