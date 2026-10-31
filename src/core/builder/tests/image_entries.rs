//! 图像字典补全条目：`/Intent`、`/Alternates`、`/Matte`。

use super::*;
use crate::core::types::PdfVersion;

fn rgb(width: u32, height: u32, fill: u8) -> ImageData {
    ImageData {
        width,
        height,
        color_space: ColorSpace::DeviceRGB,
        bits: 8,
        data: vec![fill; (width * height * 3) as usize],
        decode: None,
        interpolate: false,
        intent: None,
        alternates: Vec::new(),
        matte: None,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    }
}

fn gray(width: u32, height: u32, fill: u8) -> ImageData {
    ImageData {
        width,
        height,
        color_space: ColorSpace::DeviceGray,
        bits: 8,
        data: vec![fill; (width * height) as usize],
        decode: None,
        interpolate: false,
        intent: None,
        alternates: Vec::new(),
        matte: None,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    }
}

fn stencil() -> ImageData {
    ImageData {
        width: 1,
        height: 1,
        color_space: ColorSpace::DeviceGray,
        bits: 1,
        data: vec![0b1000_0000],
        decode: None,
        interpolate: false,
        intent: None,
        alternates: Vec::new(),
        matte: None,
        smask: None,
        image_mask: true,
        mask: None,
        opi: None,
    }
}

fn assert_version_error(b: &mut PdfBuilder, expected: &str) {
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("构建应失败");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(_)),
        "{error}"
    );
    assert!(error.to_string().contains(expected), "{error}");
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

#[test]
fn image_writes_intent_alternates_and_matte() {
    let alternate = rgb(1, 1, 64);
    let img = rgb(2, 2, 255)
        .with_intent(ImageIntent::Perceptual)
        .with_alternates(vec![alternate])
        .with_smask(gray(2, 2, 128))
        .with_matte([0.25, 0.5, 0.75]);
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.image(p, 0.0, 0.0, 10.0, 10.0, &img);
    let s = render(&mut b);

    assert!(s.contains("/Intent /Perceptual"), "{s}");
    assert!(
        s.contains("/Alternates [<< /Image "),
        "替代图像应写成 Table 90 字典数组: {s}"
    );
    assert!(s.contains("/Matte [0.25 0.5 0.75]"), "{s}");
    assert_eq!(
        s.matches("/Subtype /Image").count(),
        3,
        "基图 + 软蒙版 + 替代图像各一个流对象: {s}"
    );
    assert_eq!(s.matches("/Im1 Do").count(), 1, "{s}");
    assert!(
        !s.contains("/Im2 "),
        "软蒙版与替代图像不应登记为页面资源: {s}"
    );
    assert_eq!(s.matches("/SMask ").count(), 1, "{s}");
}

#[test]
fn image_intent_requires_pdf_1_4() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_3);
    let p = b.page(PageSize::A4);
    b.image(
        p,
        0.0,
        0.0,
        10.0,
        10.0,
        &rgb(1, 1, 0).with_intent(ImageIntent::Saturation),
    );
    assert_version_error(&mut b, "image rendering intent");
}

#[test]
fn image_alternates_require_pdf_1_3() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_2);
    let p = b.page(PageSize::A4);
    b.image(
        p,
        0.0,
        0.0,
        10.0,
        10.0,
        &rgb(1, 1, 0).with_alternates(vec![rgb(1, 1, 9)]),
    );
    assert_version_error(&mut b, "alternate images");
}

#[test]
fn image_matte_requires_pdf_1_6() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_5);
    let p = b.page(PageSize::A4);
    b.image(
        p,
        0.0,
        0.0,
        10.0,
        10.0,
        &rgb(1, 1, 0)
            .with_smask(gray(1, 1, 128))
            .with_matte([0.25, 0.5, 0.75]),
    );
    assert_version_error(&mut b, "soft mask matte color");
}

#[test]
fn invalid_matte_parameters_fail_the_build() {
    assert_image_rejected(
        rgb(1, 1, 0).with_smask(gray(1, 1, 128)).with_matte([0.5]),
        "Matte must contain 3 numbers for a 3-component image, got 1",
    );
    assert_image_rejected(
        rgb(1, 1, 0)
            .with_smask(gray(1, 1, 128))
            .with_matte([f64::NAN, 0.0, 0.0]),
        "Matte must contain finite numbers",
    );
    assert_image_rejected(
        rgb(1, 1, 0).with_matte([0.1, 0.2, 0.3]),
        "Matte requires an SMask",
    );
    assert_image_rejected(
        stencil().with_matte([0.5]),
        "image mask must not have a Matte entry",
    );
}

#[test]
fn invalid_alternate_images_fail_the_build() {
    let nested = rgb(1, 1, 0).with_alternates(vec![rgb(1, 1, 0)]);
    assert_image_rejected(
        rgb(2, 2, 0).with_alternates(vec![nested]),
        "alternate image 0 must not contain nested Alternates",
    );

    assert_image_rejected(
        rgb(2, 2, 0).with_alternates(vec![stencil()]),
        "alternate image 0 must not be an image mask",
    );

    assert_image_rejected(
        rgb(1, 1, 0).with_alternates(vec![rgb(2, 2, 0)]),
        "alternate image 0 dimensions 2x2 exceed the base image 1x1",
    );

    let mut truncated = rgb(1, 1, 0);
    truncated.data.clear();
    assert_image_rejected(
        rgb(1, 1, 0).with_alternates(vec![truncated]),
        "alternate image 0: data length 0 does not match expected 3",
    );
}

#[test]
fn image_mask_rejects_intent_and_alternates() {
    assert_image_rejected(
        stencil().with_intent(ImageIntent::Perceptual),
        "image mask must not have an Intent entry",
    );
    assert_image_rejected(
        stencil().with_alternates(vec![rgb(1, 1, 0)]),
        "image mask must not have Alternates",
    );
}
