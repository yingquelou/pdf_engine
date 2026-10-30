//! 页面边界写端测试（ISO 32000-1 14.11.2 Table 30 与 Table 360/361）：
//! 框数组与 `/BoxColorInfo` 的写出、PDF 1.3/1.4 门禁与零字节校验契约。

use super::*;
use crate::core::builder::PageBoundaries;
use crate::core::reader::page_box::{BoxColorInfo, BoxStyle, BoxStyleName, Rectangle};
use crate::core::types::{Array, PdfFeature, PdfVersion};

fn page_dict(doc: &Document, num: i64) -> Dict {
    doc.get_object(num)
        .expect("page object")
        .expect("page exists")
        .value
        .as_dict()
        .expect("page dict")
        .clone()
}

fn rect_value(values: [f64; 4]) -> Value {
    Value::Array(Array(
        values.iter().map(|value| Value::Float(*value)).collect(),
    ))
}

fn build_error(boundaries: PageBoundaries) -> WriteError {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_4);
    let p0 = b.page(PageSize::A4);
    b.set_page_boundaries(p0, boundaries);
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("invalid page boundaries must be rejected");
    assert!(out.is_empty(), "校验失败必须零字节输出");
    error
}

#[test]
fn page_boundaries_write_boxes_and_box_color_info() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_4);
    let p0 = b.page(PageSize::A4);
    b.set_page_boundaries(
        p0,
        PageBoundaries::new(Rectangle::new(0.0, 0.0, 600.0, 800.0))
            .with_crop(Rectangle::new(10.0, 20.0, 590.0, 780.0))
            .with_bleed(Rectangle::new(5.0, 5.0, 595.0, 795.0))
            .with_trim(Rectangle::new(15.0, 25.0, 585.0, 775.0))
            .with_art(Rectangle::new(20.0, 30.0, 580.0, 770.0))
            .with_box_color_info(
                BoxColorInfo::new()
                    .with_crop(
                        BoxStyle::new()
                            .with_color([0.1, 0.2, 0.3])
                            .with_width(2.5)
                            .with_style(BoxStyleName::Dashed)
                            .with_dash(vec![3.0, 2.0]),
                    )
                    .with_bleed(
                        BoxStyle::new().with_style(BoxStyleName::Other("VendorWave".into())),
                    ),
            ),
    );
    let doc = b.assemble_document().expect("assemble");

    let page = page_dict(&doc, 3);
    assert_eq!(
        page.get("MediaBox"),
        Some(&rect_value([0.0, 0.0, 600.0, 800.0])),
        "显式边界取代 PageSize 的 MediaBox"
    );
    assert_eq!(
        page.get("CropBox"),
        Some(&rect_value([10.0, 20.0, 590.0, 780.0]))
    );
    assert_eq!(
        page.get("BleedBox"),
        Some(&rect_value([5.0, 5.0, 595.0, 795.0]))
    );
    assert_eq!(
        page.get("TrimBox"),
        Some(&rect_value([15.0, 25.0, 585.0, 775.0]))
    );
    assert_eq!(
        page.get("ArtBox"),
        Some(&rect_value([20.0, 30.0, 580.0, 770.0]))
    );

    let info = page
        .get("BoxColorInfo")
        .and_then(Value::as_dict)
        .expect("/BoxColorInfo dictionary");
    assert_eq!(info.0.len(), 2, "只写出已声明的框样式条目");
    let crop = info
        .get("CropBox")
        .and_then(Value::as_dict)
        .expect("/CropBox style");
    assert_eq!(
        crop.get("C"),
        Some(&Value::Array(Array(vec![
            Value::Float(0.1),
            Value::Float(0.2),
            Value::Float(0.3),
        ])))
    );
    assert_eq!(crop.get("W"), Some(&Value::Float(2.5)));
    assert_eq!(crop.get("S"), Some(&Value::Name("D".into())));
    assert_eq!(
        crop.get("D"),
        Some(&Value::Array(Array(vec![
            Value::Float(3.0),
            Value::Float(2.0),
        ])))
    );
    let bleed = info
        .get("BleedBox")
        .and_then(Value::as_dict)
        .expect("/BleedBox style");
    assert_eq!(bleed.get("S"), Some(&Value::Name("VendorWave".into())));
    assert_eq!(bleed.0.len(), 1, "未声明的 /C /W /D 条目不物化");
    assert!(info.get("TrimBox").is_none(), "未声明的框样式不写出");
    assert!(info.get("ArtBox").is_none());

    let features = doc.features.borrow();
    assert!(features.contains(PdfFeature::PageBoundaries));
    assert!(features.contains(PdfFeature::PageBoxColorInfo));
}

#[test]
fn page_boundaries_require_pdf_1_3_for_processing_boxes() {
    for boundaries in [
        PageBoundaries::new(Rectangle::new(0.0, 0.0, 100.0, 100.0))
            .with_bleed(Rectangle::new(0.0, 0.0, 50.0, 50.0)),
        PageBoundaries::new(Rectangle::new(0.0, 0.0, 100.0, 100.0))
            .with_trim(Rectangle::new(0.0, 0.0, 50.0, 50.0)),
        PageBoundaries::new(Rectangle::new(0.0, 0.0, 100.0, 100.0))
            .with_art(Rectangle::new(0.0, 0.0, 50.0, 50.0)),
    ] {
        let mut b = PdfBuilder::with_version(PdfVersion::V1_2);
        let p0 = b.page(PageSize::A4);
        b.set_page_boundaries(p0, boundaries);
        let mut out = Vec::new();
        let error = b
            .build(&mut out)
            .expect_err("PDF 1.2 must reject processing boxes");
        assert!(
            matches!(&error, WriteError::UnsupportedVersion(_)),
            "unexpected error {error:?}"
        );
        assert!(error.to_string().contains("page boundaries"), "{error}");
        assert!(error.to_string().contains("1.3"), "{error}");
        assert!(out.is_empty(), "版本门禁失败必须零字节");
    }

    // CropBox 不引入 PDF 1.3 特性：仅 MediaBox + CropBox 在 1.2 可用。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_2);
    let p0 = b.page(PageSize::A4);
    b.set_page_boundaries(
        p0,
        PageBoundaries::new(Rectangle::new(0.0, 0.0, 600.0, 800.0))
            .with_crop(Rectangle::new(10.0, 10.0, 590.0, 790.0)),
    );
    let doc = b.assemble_document().expect("crop box works at PDF 1.2");
    let page = page_dict(&doc, 3);
    assert_eq!(
        page.get("CropBox"),
        Some(&rect_value([10.0, 10.0, 590.0, 790.0]))
    );
    assert!(!doc.features.borrow().contains(PdfFeature::PageBoundaries));
}

#[test]
fn page_box_color_info_requires_pdf_1_4() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_3);
    let p0 = b.page(PageSize::A4);
    b.set_page_boundaries(
        p0,
        PageBoundaries::new(Rectangle::new(0.0, 0.0, 100.0, 100.0))
            .with_box_color_info(BoxColorInfo::new().with_crop(BoxStyle::new())),
    );
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("PDF 1.3 must reject /BoxColorInfo");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(_)),
        "unexpected error {error:?}"
    );
    assert!(error.to_string().contains("page box color info"), "{error}");
    assert!(error.to_string().contains("1.4"), "{error}");
    assert!(out.is_empty(), "版本门禁失败必须零字节");
}

#[test]
fn invalid_page_boundaries_are_rejected_without_output() {
    // 非有限坐标。
    let error = build_error(PageBoundaries::new(Rectangle::new(
        0.0,
        0.0,
        f64::NAN,
        100.0,
    )));
    assert!(
        matches!(&error, WriteError::InvalidObject(_)),
        "unexpected error {error:?}"
    );
    assert!(error.to_string().contains("/MediaBox"), "{error}");

    // 零宽（本库选择：拒绝零面积框）。
    let error = build_error(
        PageBoundaries::new(Rectangle::new(0.0, 0.0, 100.0, 100.0))
            .with_crop(Rectangle::new(10.0, 10.0, 10.0, 20.0)),
    );
    assert!(error.to_string().contains("/CropBox"), "{error}");
    assert!(
        error.to_string().contains("positive width and height"),
        "{error}"
    );

    // /C 分量越界。
    let error = build_error(
        PageBoundaries::new(Rectangle::new(0.0, 0.0, 100.0, 100.0)).with_box_color_info(
            BoxColorInfo::new().with_crop(BoxStyle::new().with_color([1.2, 0.0, 0.0])),
        ),
    );
    assert!(error.to_string().contains("between 0 and 1"), "{error}");

    // /W 为负。
    let error = build_error(
        PageBoundaries::new(Rectangle::new(0.0, 0.0, 100.0, 100.0))
            .with_box_color_info(BoxColorInfo::new().with_crop(BoxStyle::new().with_width(-1.0))),
    );
    assert!(
        error.to_string().contains("non-negative finite number"),
        "{error}"
    );

    // /D 只允许与 /S /D 同用（Table 361）。
    let error = build_error(
        PageBoundaries::new(Rectangle::new(0.0, 0.0, 100.0, 100.0)).with_box_color_info(
            BoxColorInfo::new().with_crop(BoxStyle::new().with_dash(vec![3.0])),
        ),
    );
    assert!(
        error.to_string().contains("only allowed when /S is D"),
        "{error}"
    );
    assert!(error.to_string().contains("Table 361"), "{error}");

    // /D 全零、负数、空数组。
    for dash in [vec![0.0, 0.0], vec![3.0, -1.0], Vec::new()] {
        let error = build_error(
            PageBoundaries::new(Rectangle::new(0.0, 0.0, 100.0, 100.0)).with_box_color_info(
                BoxColorInfo::new().with_crop(
                    BoxStyle::new()
                        .with_style(BoxStyleName::Dashed)
                        .with_dash(dash),
                ),
            ),
        );
        assert!(
            error.to_string().contains("/D"),
            "unexpected dash error {error}"
        );
    }

    // 扩展 /S 名含 PDF 名字定界符。
    let error = build_error(
        PageBoundaries::new(Rectangle::new(0.0, 0.0, 100.0, 100.0)).with_box_color_info(
            BoxColorInfo::new()
                .with_crop(BoxStyle::new().with_style(BoxStyleName::Other("Bad Name".into()))),
        ),
    );
    assert!(error.to_string().contains("name delimiters"), "{error}");
}
