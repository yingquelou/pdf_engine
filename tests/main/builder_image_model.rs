use super::*;

/// D1 读端颜色空间模型：写端产出的 Indexed / Separation / DeviceN / ICCBased
/// 图片与页级命名颜色空间，经 `Document::color_space` 解析为类型化模型，
/// `decode_samples` 解码样本值正确（调色板展开、tint 函数求值、ICC 配置
/// 字节与 /N），非法空间返回类型化错误。
#[test]
fn reader_color_space_model_decodes_builder_images() {
    use pdf_engine::core::builder::{
        CalRGB, ColorSpace, DeviceN, ExponentialFunction, Font, Function, ICCBased, ImageData,
        Indexed, PageSize, PdfBuilder, SampledFunction, Separation,
    };
    use pdf_engine::core::reader::color_space::{ColorSpaceError, ColorSpaceModel};

    let assert_close = |samples: &[f64], expected: &[f64]| {
        assert_eq!(samples.len(), expected.len(), "样本个数不符: {samples:?}");
        for (actual, want) in samples.iter().zip(expected) {
            assert!((actual - want).abs() < 1e-6, "got {actual}, want {want}");
        }
    };

    let indexed_lookup = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0x11];
    let indexed = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 2,
        height: 1,
        color_space: ColorSpace::Indexed(Indexed::new(
            ColorSpace::DeviceRGB,
            1,
            indexed_lookup.clone(),
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
    let separation = ImageData {
        intent: None,
        alternates: Vec::new(),
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
        data: vec![0x00, 0xFF],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };
    let devicen = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 2,
        height: 1,
        color_space: ColorSpace::DeviceN(DeviceN::new(
            vec!["Orange".to_string(), "Green".to_string()],
            ColorSpace::DeviceRGB,
            Function::Sampled(SampledFunction::new(
                vec![0.0, 1.0, 0.0, 1.0],
                vec![0.0, 1.0, 0.0, 1.0, 0.0, 1.0],
                vec![2, 2],
                8,
                vec![0.0; 12],
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
    let profile: Vec<u8> = (0..64u32).map(|v| v as u8).collect();
    let icc = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 2,
        height: 1,
        color_space: ColorSpace::ICCBased(ICCBased {
            n: 3,
            data: profile.clone(),
            alternate: None,
            range: None,
        }),
        bits: 8,
        data: vec![10, 20, 30, 40, 50, 60],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "CS");
    builder.image(page, 72.0, 600.0, 60.0, 30.0, &indexed);
    builder.image(page, 72.0, 540.0, 60.0, 30.0, &separation);
    builder.image(page, 72.0, 480.0, 60.0, 30.0, &devicen);
    builder.image(page, 72.0, 420.0, 60.0, 30.0, &icc);
    let cal = ColorSpace::CalRGB(CalRGB {
        white_point: [0.9505, 1.0, 1.089],
        black_point: None,
        gamma: Some([2.2, 2.2, 2.2]),
        matrix: None,
    });
    let handle = builder.color_space(&cal);
    builder.set_fill_color_space(page, handle);

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
    let image_object = |name: &str| {
        let number = xobjects.get(name).and_then(Value::as_ref).unwrap().0;
        reparsed
            .get_object(number)
            .expect("get object")
            .expect("image object should exist")
    };

    let decode_image = |name: &str| -> (ColorSpaceModel, Vec<f64>) {
        let object = image_object(name);
        let Value::Stream(dict, _) = &object.value else {
            panic!("{name} should be a stream");
        };
        let model = reparsed
            .color_space(dict.get("ColorSpace").expect("ColorSpace"))
            .expect("color space should parse");
        let bits = dict
            .get("BitsPerComponent")
            .and_then(Value::as_integer)
            .expect("BitsPerComponent") as usize;
        let width = dict
            .get("Width")
            .and_then(Value::as_integer)
            .expect("Width") as usize;
        let raw = reparsed
            .decode_stream_bytes(&object.value)
            .expect("decode image bytes");
        let samples = model
            .decode_samples(bits, width, &raw)
            .expect("decode samples");
        (model, samples)
    };

    let (model, samples) = decode_image("Im1");
    let ColorSpaceModel::Indexed(indexed_space) = &model else {
        panic!("Im1 should be Indexed");
    };
    assert_eq!(indexed_space.hival, 1);
    assert_eq!(indexed_space.lookup, indexed_lookup);
    assert_eq!(
        *indexed_space.base,
        ColorSpaceModel::DeviceRGB,
        "基空间应解析为 DeviceRGB"
    );
    let expected: Vec<f64> = indexed_lookup.iter().map(|v| *v as f64 / 255.0).collect();
    assert_close(&samples, &expected);

    let (model, samples) = decode_image("Im2");
    let ColorSpaceModel::Separation(separation_space) = &model else {
        panic!("Im2 should be Separation");
    };
    assert_eq!(separation_space.colorant, "Orange");
    assert_eq!(*separation_space.alternate, ColorSpaceModel::DeviceCMYK);
    assert!(separation_space.tint.is_some(), "tint 函数应已构造");
    assert_close(&samples, &[0.0, 0.0, 0.0, 0.0, 0.0, 0.4, 0.8, 0.0]);

    let (model, samples) = decode_image("Im3");
    let ColorSpaceModel::DeviceN(device_n) = &model else {
        panic!("Im3 should be DeviceN");
    };
    assert_eq!(device_n.colorants, vec!["Orange", "Green"]);
    assert_eq!(model.component_count(), Some(2));
    assert_close(&samples, &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);

    let (model, samples) = decode_image("Im4");
    let ColorSpaceModel::IccBased(icc_space) = &model else {
        panic!("Im4 should be ICCBased");
    };
    assert_eq!(icc_space.n, 3);
    assert_eq!(icc_space.profile, profile, "应返回 ICC 配置文件字节");
    assert_eq!(
        *icc_space.alternate,
        ColorSpaceModel::DeviceRGB,
        "缺 /Alternate 时按 /N=3 取 DeviceRGB"
    );
    assert_eq!(icc_space.range, vec![(0.0, 1.0); 3]);
    let expected: Vec<f64> = [10, 20, 30, 40, 50, 60]
        .iter()
        .map(|v| *v as f64 / 255.0)
        .collect();
    assert_close(&samples, &expected);

    let named = reparsed
        .page_color_space(0, "CS1")
        .expect("named color space lookup")
        .expect("CS1 should exist");
    let ColorSpaceModel::CalRGB(cal_rgb) = named else {
        panic!("CS1 should be CalRGB");
    };
    assert_eq!(cal_rgb.white_point, vec![0.9505, 1.0, 1.089]);
    assert_eq!(cal_rgb.gamma, vec![2.2, 2.2, 2.2]);
    assert_eq!(
        cal_rgb.matrix,
        vec![1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]
    );
    assert!(
        reparsed
            .page_color_space(0, "Absent")
            .expect("lookup")
            .is_none()
    );
    assert!(matches!(
        reparsed.color_space(&Value::Name("NChannel".into())),
        Err(ColorSpaceError::Unsupported(_))
    ));
}

/// D2 读端图像模型：写端产出的 RGB、软蒙版、显式蒙版与颜色键蒙版图片
/// 经 `Document::page_image` 解析为 `ImageModel`；`samples()` 返回解码后的
/// 样本值，`/Mask` 区分为显式蒙版或颜色键范围；页面文本仍可提取。
#[test]
fn builder_images_roundtrip_through_image_model() {
    use pdf_engine::core::builder::{ColorSpace, Font, ImageData, ImageMask, PageSize, PdfBuilder};
    use pdf_engine::core::reader::color_space::ColorSpaceModel;
    use pdf_engine::core::reader::image::ImageMaskModel;

    let rgb = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 2,
        height: 2,
        color_space: ColorSpace::DeviceRGB,
        bits: 8,
        data: vec![255, 0, 0, 0, 255, 0, 0, 0, 255, 255, 255, 255],
        decode: None,
        interpolate: true,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };
    let alpha = ImageData {
        intent: None,
        alternates: Vec::new(),
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
    let soft = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 2,
        height: 2,
        color_space: ColorSpace::DeviceRGB,
        bits: 8,
        data: vec![9, 9, 9, 8, 8, 8, 7, 7, 7, 6, 6, 6],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    }
    .with_smask(alpha);
    let stencil = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 4,
        height: 1,
        color_space: ColorSpace::DeviceGray,
        bits: 1,
        data: vec![0b1010_0000],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: true,
        mask: None,
        opi: None,
    };
    let explicit = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 2,
        height: 2,
        color_space: ColorSpace::DeviceRGB,
        bits: 8,
        data: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    }
    .with_mask(ImageMask::explicit(stencil));
    let keyed = ImageData {
        intent: None,
        alternates: Vec::new(),
        matte: None,
        width: 2,
        height: 1,
        color_space: ColorSpace::DeviceRGB,
        bits: 8,
        data: vec![0, 10, 20, 30, 40, 50],
        decode: None,
        interpolate: false,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    }
    .with_mask(ImageMask::color_key(vec![0, 10, 20, 30, 40, 50]));

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "IMG");
    builder.image(page, 72.0, 600.0, 100.0, 100.0, &rgb);
    builder.image(page, 72.0, 480.0, 100.0, 100.0, &soft);
    builder.image(page, 72.0, 360.0, 100.0, 100.0, &explicit);
    builder.image(page, 72.0, 240.0, 100.0, 100.0, &keyed);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let rgb_model = reparsed
        .page_image(0, "Im1")
        .expect("page image lookup")
        .expect("Im1 should exist");
    assert_eq!((rgb_model.width, rgb_model.height), (2, 2));
    assert_eq!(rgb_model.color_space, Some(ColorSpaceModel::DeviceRGB));
    assert_eq!(rgb_model.bits_per_component, 8);
    assert!(rgb_model.interpolate);
    assert_eq!(rgb_model.decode, None);
    assert!(rgb_model.smask.is_none() && rgb_model.mask.is_none());
    let samples = rgb_model.samples().expect("rgb samples");
    assert_eq!(samples.pixel(0, 0), Some(&[1.0, 0.0, 0.0][..]));
    assert_eq!(samples.pixel(1, 0), Some(&[0.0, 1.0, 0.0][..]));
    assert_eq!(samples.pixel(0, 1), Some(&[0.0, 0.0, 1.0][..]));
    assert_eq!(samples.pixel(1, 1), Some(&[1.0, 1.0, 1.0][..]));

    let soft_model = reparsed
        .page_image(0, "Im2")
        .expect("page image lookup")
        .expect("Im2 should exist");
    let smask = soft_model.smask.as_ref().expect("Im2 should have /SMask");
    assert_eq!(smask.color_space, Some(ColorSpaceModel::DeviceGray));
    assert_eq!(
        smask.samples().expect("smask samples").values,
        vec![0.0, 64.0 / 255.0, 128.0 / 255.0, 1.0]
    );

    let explicit_model = reparsed
        .page_image(0, "Im3")
        .expect("page image lookup")
        .expect("Im3 should exist");
    let Some(ImageMaskModel::Explicit(mask)) = explicit_model.mask else {
        panic!("expected an explicit mask, got {:?}", explicit_model.mask);
    };
    assert!(mask.image_mask);
    assert_eq!((mask.width, mask.height), (4, 1));
    assert_eq!(
        mask.samples().expect("mask samples").values,
        vec![1.0, 0.0, 1.0, 0.0]
    );

    let keyed_model = reparsed
        .page_image(0, "Im4")
        .expect("page image lookup")
        .expect("Im4 should exist");
    assert_eq!(
        keyed_model.mask,
        Some(ImageMaskModel::ColorKey(vec![0, 10, 20, 30, 40, 50]))
    );

    assert!(
        reparsed
            .page_image(0, "Im9")
            .expect("page image lookup")
            .is_none()
    );
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("IMG"));
}
