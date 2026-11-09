use super::*;

/// 写入 → 重新解析：Lab 与 ICCBased（CalRGB 备用空间）图片应写出
/// CIE-Based 颜色空间数组、ICC 配置文件流，并保持像素与文本可提取。
#[test]
fn builder_cie_color_spaces_roundtrip_through_parser() {
    use pdf_engine::core::builder::{
        CalRGB, ColorSpace, Font, ICCBased, ImageData, Lab, PageSize, PdfBuilder,
    };

    let lab_pixels = vec![50, 128, 128];
    let lab_img = ImageData {
        intent: None,
        alternates: Vec::new(),
        oc: None,
        metadata: None,
        matte: None,
        width: 1,
        height: 1,
        color_space: ColorSpace::Lab(Lab {
            white_point: [0.9505, 1.0, 1.089],
            black_point: Some([0.0, 0.0, 0.0]),
            range: Some([-128.0, 127.0, -128.0, 127.0]),
        }),
        bits: 8,
        data: lab_pixels.clone(),
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };

    let profile: Vec<u8> = (0..128).map(|v| v as u8).collect();
    let icc_pixels = vec![10, 20, 30, 40, 50, 60];
    let icc_img = ImageData {
        intent: None,
        alternates: Vec::new(),
        oc: None,
        metadata: None,
        matte: None,
        width: 2,
        height: 1,
        color_space: ColorSpace::ICCBased(ICCBased {
            n: 3,
            data: profile.clone(),
            alternate: Some(Box::new(ColorSpace::CalRGB(CalRGB {
                white_point: [0.9505, 1.0, 1.089],
                black_point: None,
                gamma: Some([2.2, 2.2, 2.2]),
                matrix: None,
            }))),
            range: Some(vec![0.0, 1.0, 0.0, 1.0, 0.0, 1.0]),
        }),
        bits: 8,
        data: icc_pixels.clone(),
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "CIE");
    builder.image(page, 72.0, 600.0, 50.0, 50.0, &lab_img);
    builder.image(page, 150.0, 600.0, 50.0, 50.0, &icc_img);

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

    let lab_object = reparsed
        .get_object(xobjects.get("Im1").and_then(Value::as_ref).unwrap().0)
        .expect("get object")
        .expect("Lab image object should exist");
    let Value::Stream(lab_dict, _) = &lab_object.value else {
        panic!("Lab image should be a stream");
    };
    let lab_space = lab_dict
        .get("ColorSpace")
        .and_then(Value::as_array)
        .expect("Lab ColorSpace should be an array");
    assert_eq!(lab_space.0[0].as_name(), Some("Lab"));
    let lab_cs = lab_space.0[1].as_dict().expect("Lab parameters dict");
    let white_point = lab_cs
        .get("WhitePoint")
        .and_then(Value::as_array)
        .expect("Lab WhitePoint");
    assert_eq!(white_point.0.len(), 3);
    assert!(matches!(white_point.0[1], Value::Float(v) if v == 1.0));
    let range = lab_cs
        .get("Range")
        .and_then(Value::as_array)
        .expect("Lab Range");
    let range_values: Vec<i64> = range.0.iter().filter_map(Value::as_integer).collect();
    assert_eq!(range_values, vec![-128, 127, -128, 127]);
    assert_eq!(
        reparsed
            .decode_stream_bytes(&lab_object.value)
            .expect("decode Lab image"),
        lab_pixels
    );

    let icc_image_object = reparsed
        .get_object(xobjects.get("Im2").and_then(Value::as_ref).unwrap().0)
        .expect("get object")
        .expect("ICCBased image object should exist");
    let Value::Stream(icc_image_dict, _) = &icc_image_object.value else {
        panic!("ICCBased image should be a stream");
    };
    let icc_space = icc_image_dict
        .get("ColorSpace")
        .and_then(Value::as_array)
        .expect("ICCBased ColorSpace should be an array");
    assert_eq!(icc_space.0[0].as_name(), Some("ICCBased"));
    assert_eq!(
        reparsed
            .decode_stream_bytes(&icc_image_object.value)
            .expect("decode ICC image"),
        icc_pixels
    );

    let icc_stream_num = icc_space.0[1].as_ref().expect("ICC profile reference").0;
    let icc_profile_object = reparsed
        .get_object(icc_stream_num)
        .expect("get ICC object")
        .expect("ICC profile stream should exist");
    let Value::Stream(icc_dict, _) = &icc_profile_object.value else {
        panic!("ICC profile should be a stream");
    };
    assert_eq!(icc_dict.get("N").and_then(Value::as_integer), Some(3));
    assert_eq!(
        reparsed
            .decode_stream_bytes(&icc_profile_object.value)
            .expect("decode ICC profile"),
        profile
    );
    let alternate = icc_dict
        .get("Alternate")
        .and_then(Value::as_array)
        .expect("ICCBased alternate should be an array");
    assert_eq!(alternate.0[0].as_name(), Some("CalRGB"));
    assert!(
        alternate.0[1]
            .as_dict()
            .and_then(|dict| dict.get("Gamma"))
            .and_then(Value::as_array)
            .is_some()
    );

    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("CIE"));
}

/// 写入 → 重新解析：Indexed 图片应写出 `[/Indexed base hival 调色板]`，
/// 调色板以十六进制串保留，像素与文本仍可读取。
#[test]
fn builder_indexed_image_roundtrips_through_parser() {
    use pdf_engine::core::builder::{ColorSpace, Font, ImageData, Indexed, PageSize, PdfBuilder};

    let lookup = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x11];
    let pixels = vec![0, 1];
    let img = ImageData {
        intent: None,
        alternates: Vec::new(),
        oc: None,
        metadata: None,
        matte: None,
        width: 2,
        height: 1,
        color_space: ColorSpace::Indexed(Indexed::new(ColorSpace::DeviceRGB, 1, lookup.clone())),
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
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "IDX");
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
    let image_object = reparsed
        .get_object(xobjects.get("Im1").and_then(Value::as_ref).unwrap().0)
        .expect("get object")
        .expect("indexed image object should exist");
    let Value::Stream(dict, _) = &image_object.value else {
        panic!("indexed image should be a stream");
    };
    let space = dict
        .get("ColorSpace")
        .and_then(Value::as_array)
        .expect("Indexed ColorSpace should be an array");
    assert_eq!(space.0.len(), 4);
    assert_eq!(space.0[0].as_name(), Some("Indexed"));
    assert_eq!(space.0[1].as_name(), Some("DeviceRGB"));
    assert_eq!(space.0[2].as_integer(), Some(1));
    assert_eq!(
        space.0[3],
        Value::HexString("DEADBEEF0011".into()),
        "调色板应按字节顺序写成大写十六进制"
    );
    assert_eq!(
        reparsed
            .decode_stream_bytes(&image_object.value)
            .expect("decode indexed image"),
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
    assert!(
        String::from_utf8_lossy(&content).contains("/Im1 Do"),
        "content should paint the indexed image"
    );
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("IDX"));
}

/// 写入 → 重新解析：页级 `/ColorSpace` 命名资源应写出 CalRGB 与
/// Separation 数组，内容流用 `cs`/`CS` 切换空间、用分量操作符设色，
/// 文本仍可提取。
#[test]
fn builder_named_color_spaces_roundtrip_through_parser() {
    use pdf_engine::core::builder::{
        CalRGB, ColorSpace, ExponentialFunction, Font, Function, PageSize, PdfBuilder, Separation,
    };

    let mut cal = CalRGB::new([0.9505, 1.0, 1.089]);
    cal.gamma = Some([2.2, 2.2, 2.2]);

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let fill = builder.color_space(&ColorSpace::CalRGB(cal));
    let stroke = builder.color_space(&ColorSpace::Separation(Separation::new(
        "Spot",
        ColorSpace::DeviceGray,
        Function::Exponential(ExponentialFunction::new(vec![0.0, 1.0], 1.0)),
    )));
    builder.set_fill_color_space(page, fill);
    builder.set_fill_color_components(page, &[0.2, 0.4, 0.6]);
    builder.set_stroke_color_space(page, stroke);
    builder.set_stroke_color_components(page, &[0.5]);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "NAMED");

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
    let color_spaces = page_object
        .value
        .as_dict()
        .and_then(|dict| dict.get("Resources"))
        .and_then(Value::as_dict)
        .and_then(|resources| resources.get("ColorSpace"))
        .and_then(Value::as_dict)
        .expect("page should have ColorSpace resources");

    let cal_array = color_spaces
        .get("CS1")
        .and_then(Value::as_array)
        .expect("CS1 should be an array");
    assert_eq!(cal_array.0[0].as_name(), Some("CalRGB"));
    let cal_dict = cal_array.0[1].as_dict().expect("CalRGB dictionary");
    assert_eq!(
        cal_dict
            .get("WhitePoint")
            .and_then(Value::as_array)
            .map(|array| array.0.clone()),
        Some(vec![
            Value::Float(0.9505),
            Value::Float(1.0),
            Value::Float(1.089)
        ])
    );
    assert_eq!(
        cal_dict
            .get("Gamma")
            .and_then(Value::as_array)
            .map(|array| array.0.clone()),
        Some(vec![
            Value::Float(2.2),
            Value::Float(2.2),
            Value::Float(2.2)
        ])
    );

    let separation_array = color_spaces
        .get("CS2")
        .and_then(Value::as_array)
        .expect("CS2 should be an array");
    assert_eq!(separation_array.0[0].as_name(), Some("Separation"));
    assert_eq!(separation_array.0[1].as_name(), Some("Spot"));
    assert_eq!(separation_array.0[2].as_name(), Some("DeviceGray"));
    assert!(
        separation_array.0[3].as_dict().is_some(),
        "tint transform should be a function dictionary"
    );

    let Value::Ref(content_num, _) = page_object
        .value
        .as_dict()
        .unwrap()
        .get("Contents")
        .unwrap()
    else {
        panic!("Contents should be an indirect reference");
    };
    let content_object = reparsed
        .get_object(*content_num)
        .expect("get content object")
        .expect("content stream should exist");
    let content = String::from_utf8_lossy(
        &reparsed
            .decode_stream_bytes(&content_object.value)
            .expect("decode content"),
    )
    .into_owned();
    assert!(content.contains("/CS1 cs"), "{content}");
    assert!(content.contains("0.2 0.4 0.6 sc"), "{content}");
    assert!(content.contains("/CS2 CS"), "{content}");
    assert!(content.contains("0.5 SC"), "{content}");

    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("NAMED"));
}
