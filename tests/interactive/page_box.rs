//! 页面边界（ISO 32000-1 14.11.2 Table 30）与框颜色信息
//! （Table 360/361）写出 → 解析 → 回读往返。

use super::*;

/// 两个页面：第 0 页保持 `PageSize` 默认边界，第 1 页声明全部框与
/// `/BoxColorInfo`。解析后声明值、有效值（缺省链 + MediaBox 交集）与
/// 框样式条目一致，特性画像含两项页面边界特性。
#[test]
fn builder_page_boundaries_roundtrip_through_parser() {
    use pdf_engine::core::builder::{PageBoundaries, PageSize, PdfBuilder};
    use pdf_engine::core::reader::page_box::{BoxColorInfo, BoxStyle, BoxStyleName, Rectangle};
    use pdf_engine::core::types::{PdfFeature, PdfVersion};

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_4);
    builder.page(PageSize::A4);
    let second = builder.page(PageSize::Custom(300.0, 400.0));
    builder.set_page_boundaries(
        second,
        PageBoundaries::new(Rectangle::new(0.0, 0.0, 300.0, 400.0))
            .with_crop(Rectangle::new(10.0, 20.0, 290.0, 380.0))
            .with_bleed(Rectangle::new(5.0, 5.0, 295.0, 395.0))
            .with_trim(Rectangle::new(15.0, 25.0, 285.0, 375.0))
            .with_art(Rectangle::new(20.0, 30.0, 280.0, 370.0))
            .with_box_color_info(
                BoxColorInfo::new()
                    .with_crop(
                        BoxStyle::new()
                            .with_color([0.25, 0.5, 0.75])
                            .with_width(2.0)
                            .with_style(BoxStyleName::Dashed)
                            .with_dash(vec![3.0, 1.0]),
                    )
                    .with_art(BoxStyle::new().with_style(BoxStyleName::Other("VendorWave".into()))),
            ),
    );
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build page boundaries");
    let doc = parse_buf(&buf);

    // 第 0 页：未设置边界，行为与 PageSize 一致，有效框全部为 MediaBox。
    let defaults = doc.page_bounds(0).expect("default page bounds");
    assert_eq!(defaults.media, Rectangle::new(0.0, 0.0, 595.0, 842.0));
    assert_eq!(defaults.crop, None);
    assert_eq!(defaults.bleed, None);
    assert_eq!(defaults.trim, None);
    assert_eq!(defaults.art, None);
    assert!(defaults.box_color_info.is_none());
    assert_eq!(defaults.effective_crop(), defaults.media);
    assert_eq!(defaults.effective_bleed(), defaults.media);
    assert_eq!(defaults.effective_trim(), defaults.media);
    assert_eq!(defaults.effective_art(), defaults.media);

    // 第 1 页：声明值与有效值（均在 MediaBox 内，交集不变）。
    let bounds = doc.page_bounds(1).expect("declared page bounds");
    assert_eq!(bounds.media, Rectangle::new(0.0, 0.0, 300.0, 400.0));
    assert_eq!(bounds.crop, Some(Rectangle::new(10.0, 20.0, 290.0, 380.0)));
    assert_eq!(bounds.bleed, Some(Rectangle::new(5.0, 5.0, 295.0, 395.0)));
    assert_eq!(bounds.trim, Some(Rectangle::new(15.0, 25.0, 285.0, 375.0)));
    assert_eq!(bounds.art, Some(Rectangle::new(20.0, 30.0, 280.0, 370.0)));
    assert_eq!(
        bounds.effective_crop(),
        Rectangle::new(10.0, 20.0, 290.0, 380.0)
    );
    assert_eq!(
        bounds.effective_bleed(),
        Rectangle::new(5.0, 5.0, 295.0, 395.0)
    );
    assert_eq!(
        bounds.effective_trim(),
        Rectangle::new(15.0, 25.0, 285.0, 375.0)
    );
    assert_eq!(
        bounds.effective_art(),
        Rectangle::new(20.0, 30.0, 280.0, 370.0)
    );

    let info = bounds.box_color_info.as_ref().expect("box color info");
    let crop_style = info.crop.as_ref().expect("/CropBox style");
    assert_eq!(crop_style.color, Some([0.25, 0.5, 0.75]));
    assert_eq!(crop_style.width, Some(2.0));
    assert_eq!(crop_style.style, Some(BoxStyleName::Dashed));
    assert_eq!(crop_style.dash, Some(vec![3.0, 1.0]));
    assert_eq!(info.bleed, None, "未声明的框样式条目回读为 None");
    assert_eq!(
        info.art.as_ref().and_then(|style| style.style.clone()),
        Some(BoxStyleName::Other("VendorWave".into())),
        "扩展样式名原样保留"
    );

    let features = doc.features.borrow();
    assert!(
        features.contains(PdfFeature::PageBoundaries),
        "特性画像应含 page boundaries"
    );
    assert!(
        features.contains(PdfFeature::PageBoxColorInfo),
        "特性画像应含 page box color info"
    );
}
