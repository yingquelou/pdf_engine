//! 图像 XObject 写出：字典条目、软蒙版、图像蒙版与显式/色彩键 `/Mask`。

use super::*;

#[test]
fn image_xobject_emitted() {
    // 2x2 RGB 全红。
    let img = ImageData {
        intent: None,
        alternates: Vec::new(),
        oc: None,
        metadata: None,
        matte: None,
        width: 2,
        height: 2,
        color_space: ColorSpace::DeviceRGB,
        bits: 8,
        data: vec![255, 0, 0, 255, 0, 0, 255, 0, 0, 255, 0, 0],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::Letter);
    b.image(p, 100.0, 100.0, 50.0, 50.0, &img);
    let s = render(&mut b);

    assert!(s.contains("/Type /XObject"));
    assert!(s.contains("/Subtype /Image"));
    assert!(s.contains("/Width 2"));
    assert!(s.contains("/Height 2"));
    assert!(s.contains("/ColorSpace /DeviceRGB"));
    assert!(s.contains("/BitsPerComponent 8"));
    // 内容流应含 Do
    assert!(s.contains("/Im1 Do"));
    assert!(s.contains("cm"));
    assert!(!s.contains("/Decode"), "未提供 Decode 时不应写出: {s}");
    assert!(!s.contains("/Interpolate"), "未启用插值时不应写出: {s}");
    assert!(!s.contains("/SMask"), "无软蒙版时不应写出: {s}");
}

#[test]
fn cmyk_image_xobject_emitted() {
    // 1x1 CMYK（C 满、M/Y/K 为零）：每像素 4 字节。
    let img = ImageData {
        intent: None,
        alternates: Vec::new(),
        oc: None,
        metadata: None,
        matte: None,
        width: 1,
        height: 1,
        color_space: ColorSpace::DeviceCMYK,
        bits: 8,
        data: vec![255, 0, 0, 0],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::Letter);
    b.image(p, 10.0, 10.0, 20.0, 20.0, &img);
    let s = render(&mut b);

    assert!(s.contains("/ColorSpace /DeviceCMYK"));
    assert!(s.contains("/BitsPerComponent 8"));
    assert!(s.contains("/Length 4"), "CMYK 像素应原样写出 4 字节: {s}");
}

#[test]
fn image_writes_decode_and_interpolate_entries() {
    let img = ImageData {
        intent: None,
        alternates: Vec::new(),
        oc: None,
        metadata: None,
        matte: None,
        width: 1,
        height: 1,
        color_space: ColorSpace::DeviceRGB,
        bits: 8,
        data: vec![10, 20, 30],
        decode: Some(vec![0.0, 1.0, 1.0, 0.0, 0.0, 0.5]),
        interpolate: true,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.image(p, 0.0, 0.0, 10.0, 10.0, &img);
    let s = render(&mut b);

    assert!(s.contains("/Decode [0.0 1.0 1.0 0.0 0.0 0.5]"), "{s}");
    assert!(s.contains("/Interpolate true"), "{s}");
}

#[test]
fn image_with_smask_writes_soft_mask_object_and_reference() {
    let mask = ImageData {
        intent: None,
        alternates: Vec::new(),
        oc: None,
        metadata: None,
        matte: None,
        width: 2,
        height: 2,
        color_space: ColorSpace::DeviceGray,
        bits: 8,
        data: vec![0, 64, 128, 255],
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
    };
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.image(p, 0.0, 0.0, 10.0, 10.0, &img);
    b.image(p, 20.0, 0.0, 10.0, 10.0, &img);
    let s = render(&mut b);

    assert_eq!(
        s.matches("/Subtype /Image").count(),
        2,
        "基图与软蒙版各一个流对象: {s}"
    );
    assert_eq!(s.matches("/SMask ").count(), 1, "{s}");
    assert!(s.contains("/ColorSpace /DeviceGray"), "{s}");
    // 每次绘制分配独立资源名，但重复绘制复用同一基图对象号。
    assert_eq!(s.matches("/Im1 Do").count(), 1, "{s}");
    assert_eq!(s.matches("/Im2 Do").count(), 1, "{s}");
    let resources = &s[s.find("/XObject <<").expect("应写出 XObject 资源字典")..];
    let base_ref = resources
        .split("/Im1 ")
        .nth(1)
        .and_then(|rest| rest.split(" /Im2 ").next())
        .expect("应写出 Im1 资源引用");
    assert!(
        resources.contains(&format!("/Im2 {base_ref}")),
        "重复绘制应复用同一基图对象: {s}"
    );
    assert!(!s.contains("/Im3 "), "蒙版不应登记为页面资源: {s}");
    assert_eq!(s.matches("/Length 4").count(), 1, "蒙版 4 字节: {s}");
}

#[test]
fn stencil_image_mask_writes_image_mask_and_omits_color_space() {
    let img = ImageData {
        intent: None,
        alternates: Vec::new(),
        oc: None,
        metadata: None,
        matte: None,
        width: 8,
        height: 1,
        color_space: ColorSpace::DeviceGray,
        bits: 1,
        data: vec![0b1010_1010],
        decode: Some(vec![1.0, 0.0]),
        interpolate: true,
        smask: None,
        image_mask: true,
        mask: None,
        opi: None,
    };
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.image(p, 0.0, 0.0, 10.0, 10.0, &img);
    let s = render(&mut b);

    assert!(s.contains("/ImageMask true"), "{s}");
    assert!(!s.contains("/ColorSpace"), "图像蒙版不得写 ColorSpace: {s}");
    assert!(s.contains("/BitsPerComponent 1"), "{s}");
    assert!(s.contains("/Decode [1.0 0.0]"), "{s}");
    assert!(s.contains("/Interpolate true"), "{s}");
    assert!(s.contains("/Im1 Do"), "{s}");
}

#[test]
fn explicit_mask_writes_mask_reference_and_stencil_object() {
    let mask = ImageData {
        intent: None,
        alternates: Vec::new(),
        oc: None,
        metadata: None,
        matte: None,
        width: 4,
        height: 2,
        color_space: ColorSpace::DeviceGray,
        bits: 1,
        data: vec![0b1111_0000, 0b0000_1111],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: true,
        mask: None,
        opi: None,
    };
    let img = ImageData {
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
        mask: Some(ImageMask::explicit(mask)),
        opi: None,
    };
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.image(p, 0.0, 0.0, 10.0, 10.0, &img);
    let s = render(&mut b);

    assert!(s.contains("/ColorSpace /DeviceRGB"), "{s}");
    let mask_num = s
        .split("/Mask ")
        .nth(1)
        .and_then(|rest| rest.split(" 0 R").next())
        .expect("基图应写出 /Mask 引用");
    let start = s
        .find(&format!("\n{mask_num} 0 obj"))
        .unwrap_or_else(|| panic!("蒙版对象 {mask_num} 应存在: {s}"));
    let mask_object = &s[start..start + s[start..].find("endobj").expect("endobj")];
    assert!(mask_object.contains("/ImageMask true"), "{mask_object}");
    assert!(!mask_object.contains("/ColorSpace"), "{mask_object}");
    assert!(mask_object.contains("/Width 4"), "{mask_object}");
    assert!(mask_object.contains("/Height 2"), "{mask_object}");
    assert!(mask_object.contains("/BitsPerComponent 1"), "{mask_object}");
}

#[test]
fn colour_key_mask_writes_integer_ranges() {
    let img = ImageData {
        intent: None,
        alternates: Vec::new(),
        oc: None,
        metadata: None,
        matte: None,
        width: 2,
        height: 1,
        color_space: ColorSpace::DeviceRGB,
        bits: 8,
        data: vec![10, 20, 30, 40, 50, 60],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: Some(ImageMask::color_key(vec![0, 10, 20, 30, 40, 50])),
        opi: None,
    };
    let mut b = PdfBuilder::new();
    let p = b.page(PageSize::A4);
    b.image(p, 0.0, 0.0, 10.0, 10.0, &img);
    let s = render(&mut b);

    assert!(s.contains("/Mask [0 10 20 30 40 50]"), "{s}");
}
