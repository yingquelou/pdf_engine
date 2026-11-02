//! 版本门控：`with_version` 的头部版本覆写与图像 SMask/`/Mask` 的 1.3/1.4 门禁。

use super::*;
use crate::core::types::PdfVersion;

#[test]
fn version_override_in_header() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_4);
    let p = b.page(PageSize::A4);
    b.text(p, &Font::helvetica(12.0), 10.0, 10.0, "hi");
    let s = render(&mut b);
    assert!(s.contains("%PDF-1.4"));
}

#[test]
fn smask_requires_pdf_1_4() {
    let mask = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 1,
        height: 1,
        color_space: ColorSpace::DeviceGray,
        bits: 8,
        data: vec![0],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };
    let img = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 1,
        height: 1,
        color_space: ColorSpace::DeviceGray,
        bits: 8,
        data: vec![0],
        decode: None,
        interpolate: false,
        smask: Some(Box::new(mask)),
        image_mask: false,
        mask: None,
        opi: None,
    };

    let mut b = PdfBuilder::with_version(PdfVersion::V1_3);
    let p = b.page(PageSize::A4);
    b.image(p, 0.0, 0.0, 10.0, 10.0, &img);
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("PDF 1.3 must reject SMask");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(_)),
        "错误应为 UnsupportedVersion: {error}"
    );
    assert!(
        error.to_string().contains("transparency"),
        "错误信息应说明透明度特性: {error}"
    );
    assert!(out.is_empty(), "构建失败时不应写出任何字节");

    let mut b = PdfBuilder::with_version(PdfVersion::V1_4);
    let p = b.page(PageSize::A4);
    b.image(p, 0.0, 0.0, 10.0, 10.0, &img);
    let mut out = Vec::new();
    b.build(&mut out).expect("PDF 1.4 supports SMask");
    assert!(String::from_utf8_lossy(&out).contains("/SMask "));
}

#[test]
fn image_mask_entry_requires_pdf_1_3() {
    let img = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 2,
        height: 1,
        color_space: ColorSpace::DeviceRGB,
        bits: 8,
        data: vec![0; 6],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: Some(ImageMask::color_key(vec![0, 255, 0, 255, 0, 255])),
        opi: None,
    };
    let mut b = PdfBuilder::with_version(PdfVersion::V1_2);
    let p = b.page(PageSize::A4);
    b.image(p, 0.0, 0.0, 10.0, 10.0, &img);
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("PDF 1.2 must reject /Mask");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(_)),
        "错误应为 UnsupportedVersion: {error}"
    );
    assert!(
        error.to_string().contains("image Mask entry"),
        "错误信息应说明 /Mask 特性: {error}"
    );
    assert!(out.is_empty(), "构建失败时不应写出任何字节");

    let mut b = PdfBuilder::with_version(PdfVersion::V1_3);
    let p = b.page(PageSize::A4);
    b.image(p, 0.0, 0.0, 10.0, 10.0, &img);
    let mut out = Vec::new();
    b.build(&mut out).expect("PDF 1.3 supports /Mask");
    assert!(String::from_utf8_lossy(&out).contains("/Mask ["));

    // `/ImageMask true` 本身自 PDF 1.0 起可用，不触发 1.3 门控。
    let stencil = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 8,
        height: 1,
        color_space: ColorSpace::DeviceGray,
        bits: 1,
        data: vec![0],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: true,
        mask: None,
        opi: None,
    };
    let mut b = PdfBuilder::with_version(PdfVersion::V1_2);
    let p = b.page(PageSize::A4);
    b.image(p, 0.0, 0.0, 10.0, 10.0, &stencil);
    let mut out = Vec::new();
    b.build(&mut out).expect("PDF 1.2 supports stencil masks");
    assert!(String::from_utf8_lossy(&out).contains("/ImageMask true"));
}
