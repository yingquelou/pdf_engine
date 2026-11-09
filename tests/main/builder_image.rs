use super::*;

/// 写入 → 重新解析：CMYK 图片 XObject 应声明 `/ColorSpace /DeviceCMYK`，
/// 流字节原样保留，页面内容流经 `Do` 引用该图片且文本仍可提取。
#[test]
fn builder_cmyk_image_roundtrips_through_parser() {
    use pdf_engine::core::builder::{ColorSpace, Font, ImageData, PageSize, PdfBuilder};

    let pixels = vec![255, 0, 0, 0, 0, 255, 0, 0];
    let img = ImageData {
        intent: None,
        alternates: Vec::new(),
        oc: None,
        metadata: None,
        matte: None,
        width: 2,
        height: 1,
        color_space: ColorSpace::DeviceCMYK,
        bits: 8,
        data: pixels.clone(),
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "CMYK");
    builder.image(page, 72.0, 600.0, 100.0, 50.0, &img);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let page_object = (3..=64)
        .filter_map(|num| reparsed.get_object(num).ok().flatten())
        .find(|object| {
            object
                .value
                .as_dict()
                .and_then(|dict| dict.get("Type"))
                .and_then(Value::as_name)
                == Some("Page")
        })
        .expect("should find page object");
    let page_dict = page_object.value.as_dict().expect("page is a dict");

    let xobjects = page_dict
        .get("Resources")
        .and_then(Value::as_dict)
        .and_then(|resources| resources.get("XObject"))
        .and_then(Value::as_dict)
        .expect("page should have XObject resources");
    let Value::Ref(image_num, _) = xobjects.get("Im1").expect("Im1 resource") else {
        panic!("Im1 should be an indirect reference");
    };
    let image_object = reparsed
        .get_object(*image_num)
        .expect("get object")
        .expect("image object should exist");
    let Value::Stream(dict, _) = &image_object.value else {
        panic!("image should be a stream");
    };
    assert_eq!(dict.get("Subtype").and_then(Value::as_name), Some("Image"));
    assert_eq!(
        dict.get("ColorSpace").and_then(Value::as_name),
        Some("DeviceCMYK")
    );
    assert_eq!(dict.get("Width").and_then(Value::as_integer), Some(2));
    assert_eq!(dict.get("Height").and_then(Value::as_integer), Some(1));
    assert_eq!(
        dict.get("BitsPerComponent").and_then(Value::as_integer),
        Some(8)
    );
    assert_eq!(
        reparsed
            .decode_stream_bytes(&image_object.value)
            .expect("decode image"),
        pixels
    );

    let Value::Ref(content_num, _) = page_dict.get("Contents").expect("Contents") else {
        panic!("Contents should be an indirect reference");
    };
    let content_object = reparsed
        .get_object(*content_num)
        .expect("get content object")
        .expect("content stream should exist");
    let content = reparsed
        .decode_stream_bytes(&content_object.value)
        .expect("decode content");
    let content_text = String::from_utf8_lossy(&content);
    assert!(content_text.contains("/Im1 Do"), "{content_text}");
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("CMYK"));
}

/// 写入 → 重新解析：带 `/Decode`、`/Interpolate` 与 `/SMask` 的图片应
/// 写出软蒙版流对象并在基图中引用；蒙版为同尺寸 DeviceGray 图片，
/// 像素与文本经解析器往返后保持可读。
#[test]
fn builder_soft_mask_image_roundtrips_through_parser() {
    use pdf_engine::core::builder::{ColorSpace, Font, ImageData, PageSize, PdfBuilder};

    let mask_pixels = vec![0, 64, 128, 255];
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
        data: mask_pixels.clone(),
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };
    let pixels = vec![10, 20, 30, 40, 50, 60, 70, 80, 90, 100, 110, 120];
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
        data: pixels.clone(),
        decode: Some(vec![0.0, 1.0, 1.0, 0.0, 0.0, 1.0]),
        interpolate: true,
        smask: Some(Box::new(mask)),
        image_mask: false,
        mask: None,
        opi: None,
    };

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "IMG");
    builder.image(page, 72.0, 600.0, 100.0, 100.0, &img);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let page_object = (3..=64)
        .filter_map(|num| reparsed.get_object(num).ok().flatten())
        .find(|object| {
            object
                .value
                .as_dict()
                .and_then(|dict| dict.get("Type"))
                .and_then(Value::as_name)
                == Some("Page")
        })
        .expect("should find page object");
    let page_dict = page_object.value.as_dict().expect("page is a dict");
    let xobjects = page_dict
        .get("Resources")
        .and_then(Value::as_dict)
        .and_then(|resources| resources.get("XObject"))
        .and_then(Value::as_dict)
        .expect("page should have XObject resources");

    let image_object = reparsed
        .get_object(xobjects.get("Im1").and_then(Value::as_ref).unwrap().0)
        .expect("get object")
        .expect("image object should exist");
    let Value::Stream(dict, _) = &image_object.value else {
        panic!("image should be a stream");
    };
    assert_eq!(dict.get("Subtype").and_then(Value::as_name), Some("Image"));
    assert_eq!(
        dict.get("ColorSpace").and_then(Value::as_name),
        Some("DeviceRGB")
    );
    let decode = dict
        .get("Decode")
        .and_then(Value::as_array)
        .expect("should carry Decode array");
    let decode_values: Vec<f64> = decode
        .0
        .iter()
        .map(|value| match value {
            Value::Float(f) => *f,
            Value::Integer(i) => *i as f64,
            other => panic!("Decode entry should be numeric, got {}", other.type_name()),
        })
        .collect();
    assert_eq!(decode_values, vec![0.0, 1.0, 1.0, 0.0, 0.0, 1.0]);
    assert!(
        matches!(dict.get("Interpolate"), Some(Value::Bool(true))),
        "应写出 /Interpolate true"
    );
    assert_eq!(
        reparsed
            .decode_stream_bytes(&image_object.value)
            .expect("decode image"),
        pixels
    );

    let mask_num = dict
        .get("SMask")
        .and_then(Value::as_ref)
        .expect("image should reference an SMask")
        .0;
    let mask_object = reparsed
        .get_object(mask_num)
        .expect("get mask object")
        .expect("mask object should exist");
    let Value::Stream(mask_dict, _) = &mask_object.value else {
        panic!("mask should be a stream");
    };
    assert_eq!(
        mask_dict.get("Subtype").and_then(Value::as_name),
        Some("Image")
    );
    assert_eq!(
        mask_dict.get("ColorSpace").and_then(Value::as_name),
        Some("DeviceGray")
    );
    assert_eq!(mask_dict.get("Width").and_then(Value::as_integer), Some(2));
    assert_eq!(mask_dict.get("Height").and_then(Value::as_integer), Some(2));
    assert_eq!(
        mask_dict
            .get("BitsPerComponent")
            .and_then(Value::as_integer),
        Some(8)
    );
    assert!(mask_dict.get("SMask").is_none(), "软蒙版不得再嵌套蒙版");
    assert_eq!(
        reparsed
            .decode_stream_bytes(&mask_object.value)
            .expect("decode mask"),
        mask_pixels
    );

    let Value::Ref(content_num, _) = page_dict.get("Contents").expect("Contents") else {
        panic!("Contents should be an indirect reference");
    };
    let content_object = reparsed
        .get_object(*content_num)
        .expect("get content object")
        .expect("content stream should exist");
    let content = reparsed
        .decode_stream_bytes(&content_object.value)
        .expect("decode content");
    let content_text = String::from_utf8_lossy(&content);
    assert!(content_text.contains("/Im1 Do"), "{content_text}");
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("IMG"));
}

/// 写入 → 重新解析：Stencil 蒙版、显式蒙版引用与 Colour Key 蒙版
/// 应分别写出 `/ImageMask true`（无 ColorSpace）、`/Mask` 间接引用
/// （指向独立的 1 位蒙版流）与 `/Mask` 整数区间数组。
#[test]
fn builder_image_masks_roundtrip_through_parser() {
    use pdf_engine::core::builder::{ColorSpace, Font, ImageData, ImageMask, PageSize, PdfBuilder};

    let stencil_pixels = vec![0b1010_1010];
    let stencil = ImageData {
        intent: None,
        alternates: Vec::new(),
        oc: None,
        metadata: None,
        matte: None,
        width: 8,
        height: 1,
        color_space: ColorSpace::DeviceGray,
        bits: 1,
        data: stencil_pixels.clone(),
        decode: Some(vec![1.0, 0.0]),
        interpolate: false,
        smask: None,
        image_mask: true,
        mask: None,
        opi: None,
    };
    let explicit_mask_pixels = vec![0b1010_1010, 0b0101_0101];
    let explicit_mask = ImageData {
        intent: None,
        alternates: Vec::new(),
        oc: None,
        metadata: None,
        matte: None,
        width: 4,
        height: 2,
        color_space: ColorSpace::DeviceGray,
        bits: 1,
        data: explicit_mask_pixels.clone(),
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: true,
        mask: None,
        opi: None,
    };
    let base_pixels = vec![1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
    let explicit = ImageData {
        intent: None,
        alternates: Vec::new(),
        oc: None,
        metadata: None,
        matte: None,
        width: 2,
        height: 2,
        color_space: ColorSpace::DeviceRGB,
        bits: 8,
        data: base_pixels.clone(),
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: Some(ImageMask::explicit(explicit_mask)),
        opi: None,
    };
    let keyed = ImageData {
        intent: None,
        alternates: Vec::new(),
        oc: None,
        metadata: None,
        matte: None,
        width: 2,
        height: 1,
        color_space: ColorSpace::DeviceRGB,
        bits: 8,
        data: vec![0u8, 10, 20, 30, 40, 50],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: Some(ImageMask::color_key(vec![0, 10, 20, 30, 40, 50])),
        opi: None,
    };

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "MASK");
    builder.image(page, 72.0, 600.0, 100.0, 100.0, &stencil);
    builder.image(page, 72.0, 480.0, 100.0, 100.0, &explicit);
    builder.image(page, 72.0, 360.0, 100.0, 100.0, &keyed);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let page_object = (3..=64)
        .filter_map(|num| reparsed.get_object(num).ok().flatten())
        .find(|object| {
            object
                .value
                .as_dict()
                .and_then(|dict| dict.get("Type"))
                .and_then(Value::as_name)
                == Some("Page")
        })
        .expect("should find page object");
    let page_dict = page_object.value.as_dict().expect("page is a dict");
    let xobjects = page_dict
        .get("Resources")
        .and_then(Value::as_dict)
        .and_then(|resources| resources.get("XObject"))
        .and_then(Value::as_dict)
        .expect("page should have XObject resources");
    let image_object = |name: &str| -> Object {
        let num = xobjects
            .get(name)
            .and_then(Value::as_ref)
            .unwrap_or_else(|| panic!("{name} should be a reference"))
            .0;
        reparsed
            .get_object(num)
            .expect("get object")
            .unwrap_or_else(|| panic!("{name} object should exist"))
    };
    let stream_dict = |name: &str, object: &Object| -> Dict {
        match &object.value {
            Value::Stream(dict, _) => dict.clone(),
            other => panic!("{name} should be a stream, got {}", other.type_name()),
        }
    };

    let stencil_object = image_object("Im1");
    let stencil_dict = stream_dict("Im1", &stencil_object);
    assert!(
        matches!(stencil_dict.get("ImageMask"), Some(Value::Bool(true))),
        "Stencil 蒙版应写出 /ImageMask true"
    );
    assert!(
        stencil_dict.get("ColorSpace").is_none(),
        "Stencil 蒙版不得带 /ColorSpace"
    );
    assert_eq!(
        stencil_dict
            .get("BitsPerComponent")
            .and_then(Value::as_integer),
        Some(1)
    );
    let stencil_decode: Vec<f64> = stencil_dict
        .get("Decode")
        .and_then(Value::as_array)
        .expect("stencil decode")
        .0
        .iter()
        .map(|value| match value {
            Value::Float(f) => *f,
            Value::Integer(i) => *i as f64,
            other => panic!("Decode entry should be numeric, got {}", other.type_name()),
        })
        .collect();
    assert_eq!(stencil_decode, vec![1.0, 0.0]);
    assert_eq!(
        reparsed
            .decode_stream_bytes(&stencil_object.value)
            .expect("decode stencil"),
        stencil_pixels
    );

    let explicit_object = image_object("Im2");
    let explicit_dict = stream_dict("Im2", &explicit_object);
    assert_eq!(
        explicit_dict.get("ColorSpace").and_then(Value::as_name),
        Some("DeviceRGB")
    );
    let mask_num = explicit_dict
        .get("Mask")
        .and_then(Value::as_ref)
        .expect("显式蒙版应写出 /Mask 间接引用")
        .0;
    let mask_object = reparsed
        .get_object(mask_num)
        .expect("get mask object")
        .expect("mask object should exist");
    let Value::Stream(mask_dict, _) = &mask_object.value else {
        panic!("mask should be a stream");
    };
    assert!(matches!(
        mask_dict.get("ImageMask"),
        Some(Value::Bool(true))
    ));
    assert!(
        mask_dict.get("ColorSpace").is_none(),
        "显式蒙版不得带 /ColorSpace"
    );
    assert_eq!(mask_dict.get("Width").and_then(Value::as_integer), Some(4));
    assert_eq!(mask_dict.get("Height").and_then(Value::as_integer), Some(2));
    assert_eq!(
        mask_dict
            .get("BitsPerComponent")
            .and_then(Value::as_integer),
        Some(1)
    );
    assert_eq!(
        reparsed
            .decode_stream_bytes(&mask_object.value)
            .expect("decode mask"),
        explicit_mask_pixels
    );

    let keyed_object = image_object("Im3");
    let keyed_dict = stream_dict("Im3", &keyed_object);
    assert_eq!(
        keyed_dict.get("ColorSpace").and_then(Value::as_name),
        Some("DeviceRGB")
    );
    let ranges: Vec<i64> = keyed_dict
        .get("Mask")
        .and_then(Value::as_array)
        .expect("Colour Key 蒙版应写出 /Mask 数组")
        .0
        .iter()
        .map(|value| value.as_integer().expect("range entry is an integer"))
        .collect();
    assert_eq!(ranges, vec![0, 10, 20, 30, 40, 50]);

    let Value::Ref(content_num, _) = page_dict.get("Contents").expect("Contents") else {
        panic!("Contents should be an indirect reference");
    };
    let content_object = reparsed
        .get_object(*content_num)
        .expect("get content object")
        .expect("content stream should exist");
    let content = reparsed
        .decode_stream_bytes(&content_object.value)
        .expect("decode content");
    let content_text = String::from_utf8_lossy(&content);
    for name in ["/Im1 Do", "/Im2 Do", "/Im3 Do"] {
        assert!(
            content_text.contains(name),
            "missing {name}: {content_text}"
        );
    }
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("MASK"));
}
