//! 图片参数拒绝契约：非法数据长度、位深、Decode、SMask 与 Mask 参数的
//! `WriteError::InvalidObject` 拒绝与零字节产出。

use super::*;

#[test]
fn cmyk_image_data_length_mismatch_is_invalid_object() {
    let img = ImageData {
        intent: None,
        alternates: Vec::new(),
        oc: None,
        metadata: None,
        matte: None,
        width: 2,
        height: 2,
        color_space: ColorSpace::DeviceCMYK,
        bits: 8,
        // RGB 尺寸的缓冲：2*2*3 = 12，而 CMYK 需要 16。
        data: vec![0; 12],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::Letter);
    b.image(p, 0.0, 0.0, 10.0, 10.0, &img);

    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("CMYK 图片数据长度不匹配应被拒绝");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(
        error.to_string().contains("does not match expected 16"),
        "{error}"
    );
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

#[test]
fn image_with_unsupported_bits_is_invalid_object() {
    let img = ImageData {
        intent: None,
        alternates: Vec::new(),
        oc: None,
        metadata: None,
        matte: None,
        width: 1,
        height: 1,
        color_space: ColorSpace::DeviceGray,
        bits: 3,
        data: vec![0; 1],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::Letter);
    b.image(p, 0.0, 0.0, 10.0, 10.0, &img);

    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("BitsPerComponent 仅允许 1/2/4/8/16");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(
        error.to_string().contains("bits per component 3"),
        "{error}"
    );
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

#[test]
fn invalid_image_decode_fails_the_build() {
    let base = || ImageData {
        intent: None,
        alternates: Vec::new(),
        oc: None,
        metadata: None,
        matte: None,
        width: 1,
        height: 1,
        color_space: ColorSpace::DeviceRGB,
        bits: 8,
        data: vec![0; 3],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };
    assert_image_rejected(
        base().with_decode(vec![0.0, 1.0]),
        "Decode must contain 6 numbers for a 3-component image, got 2",
    );
    assert_image_rejected(
        base().with_decode(vec![0.0, f64::NAN, 0.0, 1.0, 0.0, 1.0]),
        "Decode must contain finite numbers",
    );
}

#[test]
fn invalid_smask_parameters_fail_the_build() {
    fn gray_mask(width: u32, height: u32, data: Vec<u8>) -> ImageData {
        ImageData {
            intent: None,
            alternates: Vec::new(),
            oc: None,
            metadata: None,
            matte: None,
            width,
            height,
            color_space: ColorSpace::DeviceGray,
            bits: 8,
            data,
            decode: None,
            interpolate: false,
            smask: None,
            image_mask: false,
            mask: None,
            opi: None,
        }
    }
    fn base_with(mask: ImageData) -> ImageData {
        ImageData {
            intent: None,
            alternates: Vec::new(),
            oc: None,
            metadata: None,
            matte: None,
            width: 2,
            height: 2,
            color_space: ColorSpace::DeviceRGB,
            bits: 8,
            data: vec![0; 12],
            decode: None,
            interpolate: false,
            smask: Some(Box::new(mask)),
            image_mask: false,
            mask: None,
            opi: None,
        }
    }

    assert_image_rejected(
        base_with(gray_mask(1, 1, vec![0])),
        "SMask dimensions 1x1 do not match image 2x2",
    );
    assert_image_rejected(
        base_with(ImageData {
            intent: None,
            alternates: Vec::new(),
            oc: None,
            metadata: None,
            matte: None,
            width: 2,
            height: 2,
            color_space: ColorSpace::DeviceRGB,
            bits: 8,
            data: vec![0; 12],
            decode: None,
            interpolate: false,
            smask: None,
            image_mask: false,
            mask: None,
            opi: None,
        }),
        "SMask color space must be DeviceGray",
    );
    assert_image_rejected(
        base_with(gray_mask(2, 2, vec![0; 4]).with_smask(gray_mask(2, 2, vec![0; 4]))),
        "SMask must not contain another SMask",
    );
    assert_image_rejected(
        base_with(gray_mask(2, 2, vec![0; 3])),
        "SMask: data length 3 does not match expected 4",
    );
}

#[test]
fn invalid_image_mask_parameters_fail_the_build() {
    let base = || ImageData {
        intent: None,
        alternates: Vec::new(),
        oc: None,
        metadata: None,
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
    assert_image_rejected(
        ImageData {
            intent: None,
            alternates: Vec::new(),
            oc: None,
            metadata: None,
            matte: None,
            bits: 8,
            data: vec![0; 8],
            ..base()
        },
        "image mask requires BitsPerComponent 1",
    );
    assert_image_rejected(
        ImageData {
            intent: None,
            alternates: Vec::new(),
            oc: None,
            metadata: None,
            matte: None,
            color_space: ColorSpace::DeviceRGB,
            ..base()
        },
        "image mask color space must be DeviceGray",
    );
    assert_image_rejected(
        base().with_decode(vec![0.0, 0.5]),
        "image mask Decode must be [0 1] or [1 0]",
    );
    let smask = ImageData {
        intent: None,
        alternates: Vec::new(),
        oc: None,
        metadata: None,
        matte: None,
        width: 8,
        height: 1,
        color_space: ColorSpace::DeviceGray,
        bits: 8,
        data: vec![0; 8],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };
    assert_image_rejected(
        ImageData {
            intent: None,
            alternates: Vec::new(),
            oc: None,
            metadata: None,
            matte: None,
            smask: Some(Box::new(smask)),
            ..base()
        },
        "image mask must not have an SMask",
    );
    assert_image_rejected(
        ImageData {
            intent: None,
            alternates: Vec::new(),
            oc: None,
            metadata: None,
            matte: None,
            mask: Some(ImageMask::color_key(vec![0, 1])),
            ..base()
        },
        "image mask must not have a Mask entry",
    );
}

#[test]
fn invalid_explicit_and_colour_key_masks_fail_the_build() {
    let base = || ImageData {
        intent: None,
        alternates: Vec::new(),
        oc: None,
        metadata: None,
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
        mask: None,
        opi: None,
    };
    let plain_gray = ImageData {
        intent: None,
        alternates: Vec::new(),
        oc: None,
        metadata: None,
        matte: None,
        width: 2,
        height: 1,
        color_space: ColorSpace::DeviceGray,
        bits: 8,
        data: vec![0; 2],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };
    assert_image_rejected(
        base().with_mask(ImageMask::explicit(plain_gray)),
        "Mask image must be an image mask (ImageMask true)",
    );
    let broken_mask = ImageData {
        intent: None,
        alternates: Vec::new(),
        oc: None,
        metadata: None,
        matte: None,
        width: 2,
        height: 1,
        color_space: ColorSpace::DeviceGray,
        bits: 8,
        data: vec![0; 2],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: true,
        mask: None,
        opi: None,
    };
    assert_image_rejected(
        base().with_mask(ImageMask::explicit(broken_mask)),
        "Mask: image mask requires BitsPerComponent 1",
    );
    assert_image_rejected(
        base().with_mask(ImageMask::color_key(vec![0, 1])),
        "color key Mask must contain 6 integers for a 3-component image, got 2",
    );
    assert_image_rejected(
        base().with_mask(ImageMask::color_key(vec![0, 1, 2, 3, 4, 300])),
        "color key Mask ranges must satisfy 0 <= min <= max <= 255",
    );
    assert_image_rejected(
        base().with_mask(ImageMask::color_key(vec![0, 10, 20, 15, 0, 255])),
        "color key Mask ranges must satisfy 0 <= min <= max <= 255",
    );
}
