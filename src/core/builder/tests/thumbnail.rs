//! 页面缩略图写端测试（12.3.4、页面字典 Table 30 `/Thumb`）：图像引用
//! 写出、与绘制图像共用去重池、可选性与 12.3.4 颜色空间/图像蒙版契约。

use super::*;

fn page_dict(doc: &Document, num: i64) -> Dict {
    doc.get_object(num)
        .expect("page object")
        .expect("page exists")
        .value
        .as_dict()
        .expect("page dict")
        .clone()
}

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
    }
}

fn thumbnail_ref(doc: &Document, page_num: i64) -> (i64, i64) {
    match page_dict(doc, page_num).get("Thumb") {
        Some(Value::Ref(num, generation)) => (*num, *generation),
        other => panic!("expected /Thumb reference, got {other:?}"),
    }
}

#[test]
fn page_thumbnail_writes_image_reference() {
    let mut b = PdfBuilder::new();
    let p0 = b.page(PageSize::A4);
    b.set_page_thumbnail(p0, &rgb(2, 2, 0x40));
    let doc = b.assemble_document().expect("assemble");

    let page = page_dict(&doc, 3);
    let (num, generation) = thumbnail_ref(&doc, 3);
    assert_eq!(generation, 0);
    let image = doc.get_object(num).expect("thumbnail").expect("exists");
    let Value::Stream(dict, _) = &image.value else {
        panic!("thumbnail must be an image stream");
    };
    assert_eq!(dict.get("Type"), Some(&Value::Name("XObject".into())));
    assert_eq!(dict.get("Subtype"), Some(&Value::Name("Image".into())));
    assert_eq!(dict.get("Width"), Some(&Value::Integer(2)));
    assert_eq!(dict.get("Height"), Some(&Value::Integer(2)));
    assert_eq!(
        dict.get("ColorSpace"),
        Some(&Value::Name("DeviceRGB".into()))
    );
    assert_eq!(dict.get("BitsPerComponent"), Some(&Value::Integer(8)));

    let resources = page
        .get("Resources")
        .and_then(Value::as_dict)
        .expect("page resources");
    assert!(
        resources.get("XObject").is_none(),
        "缩略图不得进入页面内容资源"
    );
}

#[test]
fn page_thumbnail_shares_the_image_pool_with_drawn_images() {
    let image = rgb(2, 2, 1);
    let mut b = PdfBuilder::new();
    let p0 = b.page(PageSize::A4);
    b.image(p0, 0.0, 0.0, 10.0, 10.0, &image);
    b.set_page_thumbnail(p0, &image);
    let doc = b.assemble_document().expect("assemble");

    let page = page_dict(&doc, 3);
    let thumbnail = page.get("Thumb").expect("/Thumb");
    let xobjects = page
        .get("Resources")
        .and_then(Value::as_dict)
        .and_then(|resources| resources.get("XObject"))
        .and_then(Value::as_dict)
        .expect("XObject resources");
    assert_eq!(
        xobjects.get("Im1"),
        Some(thumbnail),
        "同像素图像应复用对象号"
    );
}

#[test]
fn page_thumbnail_is_optional_and_last_wins() {
    let mut b = PdfBuilder::new();
    let p0 = b.page(PageSize::A4);
    b.page(PageSize::A4);
    b.set_page_thumbnail(p0, &rgb(2, 2, 1));
    b.set_page_thumbnail(p0, &rgb(3, 3, 2));
    b.set_page_thumbnail(9, &rgb(4, 4, 3));
    let doc = b.assemble_document().expect("assemble");

    let (num, _) = thumbnail_ref(&doc, 3);
    let image = doc.get_object(num).expect("thumbnail").expect("exists");
    let Value::Stream(dict, _) = &image.value else {
        panic!("thumbnail must be an image stream");
    };
    assert_eq!(
        dict.get("Width"),
        Some(&Value::Integer(3)),
        "重复设置以后一次为准"
    );

    let second = page_dict(&doc, 5);
    assert!(second.get("Thumb").is_none(), "未设置缩略图时不写 /Thumb");
}

#[test]
fn invalid_page_thumbnails_are_rejected() {
    let mut cmyk = rgb(2, 2, 0);
    cmyk.color_space = ColorSpace::DeviceCMYK;
    cmyk.data = vec![0; 16];
    let mut b = PdfBuilder::new();
    let p0 = b.page(PageSize::A4);
    b.set_page_thumbnail(p0, &cmyk);
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("CMYK thumbnails must be rejected");
    assert!(
        matches!(error, WriteError::InvalidObject(_)),
        "unexpected error {error:?}"
    );
    assert!(out.is_empty(), "非法颜色空间必须零字节");

    let mut mask = rgb(8, 1, 0);
    mask.image_mask = true;
    mask.bits = 1;
    mask.data = vec![0; 1];
    let mut b = PdfBuilder::new();
    let p0 = b.page(PageSize::A4);
    b.set_page_thumbnail(p0, &mask);
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("image mask thumbnails must be rejected");
    assert!(
        matches!(error, WriteError::InvalidObject(_)),
        "unexpected error {error:?}"
    );
    assert!(out.is_empty(), "图像蒙版必须零字节");
}
