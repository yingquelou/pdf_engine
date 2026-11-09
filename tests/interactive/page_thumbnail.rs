//! 页面缩略图 `/Thumb` 的写出 → 解析 → 回读往返（12.3.4 Thumbnail
//! Images、页面字典 Table 30）。

use super::*;

/// 写入 → 重新解析：首页声明 2×2 DeviceRGB 缩略图，读端 `page_thumbnail`
/// 还原尺寸、位深、颜色空间与像素数据；未声明缩略图的页面回读 `None`。
#[test]
fn builder_page_thumbnail_roundtrips_through_parser() {
    use pdf_engine::core::builder::{ColorSpace, ImageData, PageSize, PdfBuilder};
    use pdf_engine::core::reader::color_space::ColorSpaceModel;
    use pdf_engine::core::types::PdfVersion;

    let pixels: Vec<u8> = (0..12).map(|value| value * 10).collect();
    let thumbnail = ImageData {
        width: 2,
        height: 2,
        color_space: ColorSpace::DeviceRGB,
        bits: 8,
        data: pixels.clone(),
        decode: None,
        interpolate: false,
        intent: None,
        alternates: Vec::new(),
        oc: None,
        metadata: None,
        matte: None,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    let p0 = builder.page(PageSize::A4);
    builder.page(PageSize::A4);
    builder.set_page_thumbnail(p0, &thumbnail);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build page thumbnail");

    let doc = parse_buf(&buf);
    let read = doc
        .page_thumbnail(0)
        .expect("thumbnail")
        .expect("page 0 declares /Thumb");
    assert_eq!(read.width, 2);
    assert_eq!(read.height, 2);
    assert_eq!(read.bits_per_component, 8);
    assert_eq!(read.color_space, Some(ColorSpaceModel::DeviceRGB));
    assert_eq!(read.decode, None);
    assert_eq!(read.data, pixels);

    assert!(
        doc.page_thumbnail(1).expect("thumbnail").is_none(),
        "second page declares no /Thumb"
    );
}
