//! 图像内联颜色空间：CIE 基（CalGray/CalRGB/Lab）、ICCBased、Indexed、
//! Separation/DeviceN 的写出与非法参数零字节拒绝。

use super::*;

#[test]
fn cal_gray_image_writes_white_black_and_gamma() {
    let img = ImageData {
        intent: None,
        alternates: Vec::new(),
        oc: None,
        metadata: None,
        matte: None,
        width: 2,
        height: 1,
        color_space: ColorSpace::CalGray(CalGray {
            white_point: [0.9505, 1.0, 1.089],
            black_point: Some([0.0, 0.0, 0.0]),
            gamma: Some(2.2),
        }),
        bits: 8,
        data: vec![0, 255],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.image(p, 10.0, 10.0, 20.0, 20.0, &img);
    let s = render(&mut b);

    assert!(s.contains("/ColorSpace [/CalGray <<"), "{s}");
    assert!(s.contains("/WhitePoint [0.9505 1.0 1.089]"), "{s}");
    assert!(s.contains("/BlackPoint [0.0 0.0 0.0]"), "{s}");
    assert!(s.contains("/Gamma 2.2"), "{s}");
}

#[test]
fn cal_rgb_image_writes_gamma_and_matrix() {
    let img = ImageData {
        intent: None,
        alternates: Vec::new(),
        oc: None,
        metadata: None,
        matte: None,
        width: 1,
        height: 1,
        color_space: ColorSpace::CalRGB(CalRGB {
            white_point: [0.9505, 1.0, 1.089],
            black_point: None,
            gamma: Some([2.2, 2.2, 2.2]),
            matrix: Some([0.5, 0.0, 0.0, 0.0, 0.5, 0.0, 0.0, 0.0, 0.5]),
        }),
        bits: 8,
        data: vec![1, 2, 3],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.image(p, 0.0, 0.0, 10.0, 10.0, &img);
    let s = render(&mut b);

    assert!(s.contains("/ColorSpace [/CalRGB <<"), "{s}");
    assert!(s.contains("/Gamma [2.2 2.2 2.2]"), "{s}");
    assert!(
        s.contains("/Matrix [0.5 0.0 0.0 0.0 0.5 0.0 0.0 0.0 0.5]"),
        "{s}"
    );
    assert!(!s.contains("/BlackPoint"), "未提供的黑点应省略: {s}");
}

#[test]
fn lab_image_writes_range() {
    let img = ImageData {
        intent: None,
        alternates: Vec::new(),
        oc: None,
        metadata: None,
        matte: None,
        width: 1,
        height: 1,
        color_space: ColorSpace::Lab(Lab {
            white_point: [0.9505, 1.0, 1.089],
            black_point: None,
            range: Some([-128.0, 127.0, -128.0, 127.0]),
        }),
        bits: 8,
        data: vec![128, 0, 0],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.image(p, 0.0, 0.0, 10.0, 10.0, &img);
    let s = render(&mut b);

    assert!(s.contains("/ColorSpace [/Lab <<"), "{s}");
    assert!(s.contains("/Range [-128.0 127.0 -128.0 127.0]"), "{s}");
    assert!(s.contains("/WhitePoint [0.9505 1.0 1.089]"), "{s}");
}

#[test]
fn icc_based_image_writes_profile_stream_and_alternative() {
    let profile = vec![0, 1, 2, 3];
    let img = ImageData {
        intent: None,
        alternates: Vec::new(),
        oc: None,
        metadata: None,
        matte: None,
        width: 2,
        height: 1,
        color_space: ColorSpace::ICCBased(ICCBased {
            n: 3,
            data: profile,
            alternate: Some(Box::new(ColorSpace::DeviceRGB)),
            range: Some(vec![0.0, 1.0, 0.0, 1.0, 0.0, 1.0]),
        }),
        bits: 8,
        data: vec![1, 2, 3, 4, 5, 6],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.image(p, 0.0, 0.0, 10.0, 10.0, &img);
    let s = render(&mut b);

    assert!(s.contains("/ColorSpace [/ICCBased "), "{s}");
    assert!(s.contains(" 0 R]"), "ICCBased 应引用配置文件流: {s}");
    assert!(s.contains("/N 3"), "{s}");
    assert!(s.contains("/Alternate /DeviceRGB"), "{s}");
    assert!(s.contains("/Range [0.0 1.0 0.0 1.0 0.0 1.0]"), "{s}");
    assert!(s.contains("/Length 4"), "配置文件 4 字节原样写入: {s}");
}

#[test]
fn indexed_image_writes_base_hival_and_lookup_hex() {
    let img = ImageData {
        intent: None,
        alternates: Vec::new(),
        oc: None,
        metadata: None,
        matte: None,
        width: 2,
        height: 1,
        color_space: ColorSpace::Indexed(Indexed::new(
            ColorSpace::DeviceRGB,
            1,
            vec![0xFF, 0x00, 0x00, 0x00, 0xFF, 0x00],
        )),
        bits: 8,
        data: vec![0, 1],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.image(p, 0.0, 0.0, 10.0, 10.0, &img);
    let s = render(&mut b);

    assert!(
        s.contains("/ColorSpace [/Indexed /DeviceRGB 1 <FF000000FF00>]"),
        "{s}"
    );
    assert!(s.contains("/BitsPerComponent 8"), "{s}");
}

#[test]
fn indexed_image_embeds_cie_base_definition() {
    let img = ImageData {
        intent: None,
        alternates: Vec::new(),
        oc: None,
        metadata: None,
        matte: None,
        width: 3,
        height: 1,
        color_space: ColorSpace::Indexed(Indexed::new(
            ColorSpace::CalGray(CalGray {
                white_point: [0.9505, 1.0, 1.089],
                black_point: None,
                gamma: Some(2.2),
            }),
            2,
            vec![0x00, 0x80, 0xFF],
        )),
        bits: 8,
        data: vec![0, 1, 2],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.image(p, 0.0, 0.0, 10.0, 10.0, &img);
    let s = render(&mut b);

    assert!(s.contains("/ColorSpace [/Indexed [/CalGray <<"), "{s}");
    assert!(s.contains("/Gamma 2.2"), "{s}");
    assert!(s.contains("2 <0080FF>]"), "{s}");
}

#[test]
fn separation_image_writes_colorant_alternate_and_tint_transform() {
    let img = ImageData {
        intent: None,
        alternates: Vec::new(),
        oc: None,
        metadata: None,
        matte: None,
        width: 2,
        height: 1,
        color_space: ColorSpace::Separation(Separation::new(
            "Orange",
            ColorSpace::DeviceCMYK,
            Function::Exponential(
                ExponentialFunction::new(vec![0.0, 1.0], 1.0)
                    .with_c0(vec![0.0, 0.0, 0.0, 0.0])
                    .with_c1(vec![0.0, 0.4, 0.8, 0.0]),
            ),
        )),
        bits: 8,
        data: vec![0x20, 0xE0],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.image(p, 0.0, 0.0, 10.0, 10.0, &img);
    let s = render(&mut b);

    assert!(
        s.contains("/ColorSpace [/Separation /Orange /DeviceCMYK <<"),
        "{s}"
    );
    assert!(s.contains("/FunctionType 2"), "{s}");
    assert!(s.contains("/Domain [0.0 1.0]"), "{s}");
    assert!(s.contains("/C0 [0.0 0.0 0.0 0.0]"), "{s}");
    assert!(s.contains("/C1 [0.0 0.4 0.8 0.0]"), "{s}");
}

#[test]
fn device_n_image_writes_names_and_sampled_tint_transform() {
    let img = ImageData {
        intent: None,
        alternates: Vec::new(),
        oc: None,
        metadata: None,
        matte: None,
        width: 2,
        height: 1,
        color_space: ColorSpace::DeviceN(DeviceN::new(
            vec!["Orange".to_string(), "Green".to_string()],
            ColorSpace::DeviceRGB,
            Function::Sampled(SampledFunction::new(
                vec![0.0, 1.0, 0.0, 1.0],
                vec![0.0, 1.0, 0.0, 1.0, 0.0, 1.0],
                vec![2, 1],
                8,
                vec![0.0; 6],
            )),
        )),
        bits: 8,
        data: vec![0x10, 0x20, 0x30, 0x40],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.image(p, 0.0, 0.0, 10.0, 10.0, &img);
    let s = render(&mut b);

    assert!(
        s.contains("/ColorSpace [/DeviceN [/Orange /Green] /DeviceRGB "),
        "{s}"
    );
    assert!(s.contains("/FunctionType 0"), "{s}");
    assert!(s.contains("/Domain [0.0 1.0 0.0 1.0]"), "{s}");
    assert!(s.contains("/Range [0.0 1.0 0.0 1.0 0.0 1.0]"), "{s}");
    assert!(s.contains("/Size [2 1]"), "{s}");
    assert!(s.contains("/BitsPerSample 8"), "{s}");
    assert!(s.contains("/Length 6"), "6 个采样点各 1 字节: {s}");
}

#[test]
fn device_n_allows_repeated_none_colorant() {
    let img = ImageData {
        intent: None,
        alternates: Vec::new(),
        oc: None,
        metadata: None,
        matte: None,
        width: 1,
        height: 1,
        color_space: ColorSpace::DeviceN(DeviceN::new(
            vec!["None".to_string(), "None".to_string()],
            ColorSpace::DeviceGray,
            Function::Calculator(CalculatorFunction::new(
                vec![0.0, 1.0, 0.0, 1.0],
                vec![0.0, 1.0],
                b"pop pop 0",
            )),
        )),
        bits: 8,
        data: vec![0x00, 0xFF],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.image(p, 0.0, 0.0, 10.0, 10.0, &img);
    let s = render(&mut b);

    assert!(
        s.contains("/ColorSpace [/DeviceN [/None /None] /DeviceGray "),
        "{s}"
    );
    assert!(s.contains("/FunctionType 4"), "{s}");
}

#[test]
fn invalid_indexed_parameters_fail_the_build() {
    // 调色板长度必须为 (hival + 1) × 基础空间通道数。
    assert_color_space_rejected(
        ColorSpace::Indexed(Indexed::new(ColorSpace::DeviceRGB, 1, vec![0; 5])),
        "Indexed lookup must contain 6 bytes",
    );
    // 嵌套 Indexed 不是合法的 base。
    assert_color_space_rejected(
        ColorSpace::Indexed(Indexed::new(
            ColorSpace::Indexed(Indexed::new(ColorSpace::DeviceGray, 0, vec![0])),
            0,
            vec![0],
        )),
        "Indexed base must not be another Indexed color space",
    );
    // base 参数错误应带上下文向上传播。
    assert_color_space_rejected(
        ColorSpace::Indexed(Indexed::new(
            ColorSpace::Lab(Lab {
                white_point: [f64::NAN, 1.0, 1.0],
                black_point: None,
                range: None,
            }),
            0,
            vec![0, 0, 0],
        )),
        "Indexed base: Lab WhitePoint must contain 3 finite numbers",
    );
}

fn assert_color_space_rejected(color_space: ColorSpace, expected: &str) {
    let img = ImageData {
        intent: None,
        alternates: Vec::new(),
        oc: None,
        metadata: None,
        matte: None,
        width: 1,
        height: 1,
        color_space,
        bits: 8,
        data: vec![0; 4],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.image(p, 0.0, 0.0, 10.0, 10.0, &img);

    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("非法颜色空间应被拒绝");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(error.to_string().contains(expected), "{error}");
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

#[test]
fn invalid_cie_color_space_parameters_fail_the_build() {
    assert_color_space_rejected(
        ColorSpace::CalGray(CalGray {
            white_point: [f64::NAN, 1.0, 1.0],
            black_point: None,
            gamma: None,
        }),
        "CalGray WhitePoint must contain 3 finite numbers",
    );
    assert_color_space_rejected(
        ColorSpace::CalGray(CalGray {
            white_point: [0.9505, 1.0, 1.089],
            black_point: None,
            gamma: Some(0.0),
        }),
        "CalGray Gamma must be a finite positive number",
    );
    assert_color_space_rejected(
        ColorSpace::CalRGB(CalRGB {
            white_point: [0.9505, 1.0, 1.089],
            black_point: None,
            gamma: Some([1.0, -1.0, 1.0]),
            matrix: None,
        }),
        "CalRGB Gamma must contain 3 finite positive numbers",
    );
    assert_color_space_rejected(
        ColorSpace::CalRGB(CalRGB {
            white_point: [0.9505, 1.0, 1.089],
            black_point: None,
            gamma: None,
            matrix: Some([f64::INFINITY, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]),
        }),
        "CalRGB Matrix must contain 9 finite numbers",
    );
    assert_color_space_rejected(
        ColorSpace::Lab(Lab {
            white_point: [0.9505, 1.0, 1.089],
            black_point: None,
            range: Some([0.0, -1.0, -100.0, 100.0]),
        }),
        "Lab Range minimum must not exceed maximum",
    );
}

#[test]
fn invalid_icc_based_parameters_fail_the_build() {
    assert_color_space_rejected(
        ColorSpace::ICCBased(ICCBased::new(2, vec![0])),
        "ICCBased N must be 1, 3 or 4",
    );
    assert_color_space_rejected(
        ColorSpace::ICCBased(ICCBased::new(3, Vec::new())),
        "ICCBased profile data must not be empty",
    );
    assert_color_space_rejected(
        ColorSpace::ICCBased(ICCBased {
            n: 3,
            data: vec![0],
            alternate: None,
            range: Some(vec![0.0, 1.0, 0.0, 1.0]),
        }),
        "ICCBased Range must contain 6 numbers",
    );
    assert_color_space_rejected(
        ColorSpace::ICCBased(ICCBased {
            n: 3,
            data: vec![0],
            alternate: Some(Box::new(ColorSpace::DeviceCMYK)),
            range: None,
        }),
        "ICCBased Alternate has 4 components, expected 3",
    );
    assert_color_space_rejected(
        ColorSpace::ICCBased(ICCBased {
            n: 3,
            data: vec![0],
            alternate: Some(Box::new(ColorSpace::ICCBased(ICCBased::new(3, vec![1])))),
            range: None,
        }),
        "ICCBased Alternate must not be another ICCBased color space",
    );
}

#[test]
fn invalid_separation_and_device_n_color_spaces_fail_the_build() {
    assert_color_space_rejected(
        ColorSpace::Separation(Separation::new(
            "",
            ColorSpace::DeviceGray,
            Function::Exponential(ExponentialFunction::new(vec![0.0, 1.0], 1.0)),
        )),
        "Separation colorant must not be empty",
    );
    assert_color_space_rejected(
        ColorSpace::Separation(Separation::new(
            "Orange",
            ColorSpace::Indexed(Indexed::new(ColorSpace::DeviceRGB, 0, vec![0, 0, 0])),
            Function::Exponential(ExponentialFunction::new(vec![0.0, 1.0], 1.0)),
        )),
        "Separation alternate color space must be a device or CIE-based color space",
    );
    assert_color_space_rejected(
        ColorSpace::Separation(Separation::new(
            "Orange",
            ColorSpace::DeviceCMYK,
            Function::Exponential(ExponentialFunction::new(vec![0.0, 1.0], 1.0)),
        )),
        "Separation tint transform must produce 4 output(s)",
    );
    assert_color_space_rejected(
        ColorSpace::DeviceN(DeviceN::new(
            Vec::new(),
            ColorSpace::DeviceGray,
            Function::Exponential(ExponentialFunction::new(vec![0.0, 1.0], 1.0)),
        )),
        "DeviceN must have at least one colorant",
    );
    assert_color_space_rejected(
        ColorSpace::DeviceN(DeviceN::new(
            vec!["All".to_string()],
            ColorSpace::DeviceGray,
            Function::Exponential(ExponentialFunction::new(vec![0.0, 1.0], 1.0)),
        )),
        "DeviceN must not use the reserved colorant name All",
    );
    assert_color_space_rejected(
        ColorSpace::DeviceN(DeviceN::new(
            vec!["Orange".to_string(), "Orange".to_string()],
            ColorSpace::DeviceGray,
            Function::Exponential(ExponentialFunction::new(vec![0.0, 1.0], 1.0)),
        )),
        "DeviceN colorant names must be distinct, got duplicate Orange",
    );
    assert_color_space_rejected(
        ColorSpace::DeviceN(DeviceN::new(
            vec!["Orange".to_string(), "Green".to_string()],
            ColorSpace::DeviceGray,
            Function::Exponential(ExponentialFunction::new(vec![0.0, 1.0], 1.0)),
        )),
        "DeviceN tint transform must take 2 input(s), got 1",
    );
    assert_color_space_rejected(
        ColorSpace::DeviceN(DeviceN::new(
            vec!["Orange".to_string()],
            ColorSpace::Separation(Separation::new(
                "Orange",
                ColorSpace::DeviceGray,
                Function::Exponential(ExponentialFunction::new(vec![0.0, 1.0], 1.0)),
            )),
            Function::Exponential(ExponentialFunction::new(vec![0.0, 1.0], 1.0)),
        )),
        "DeviceN alternate color space must be a device or CIE-based color space",
    );
}
