use super::*;

/// 写入 → 重新解析：Separation 与 DeviceN 图片的色彩空间数组应带色料名、
/// 备用空间与 tint 变换函数（Type 2 内联、Type 0 独立流）；像素流可解码，
/// 文本仍可提取。
#[test]
fn builder_separation_and_devicen_images_roundtrip_through_parser() {
    use pdf_engine::core::builder::{
        ColorSpace, DeviceN, ExponentialFunction, Font, Function, ImageData, PageSize, PdfBuilder,
        SampledFunction, Separation,
    };

    let separation = ImageData {
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

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "SPOT");
    builder.image(page, 72.0, 600.0, 100.0, 50.0, &separation);
    builder.image(page, 72.0, 500.0, 100.0, 50.0, &devicen);

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

    let separation_object = image_object("Im1");
    let Value::Stream(separation_dict, _) = &separation_object.value else {
        panic!("separation image should be a stream");
    };
    let separation_space = separation_dict
        .get("ColorSpace")
        .and_then(Value::as_array)
        .expect("Separation ColorSpace should be an array");
    assert_eq!(separation_space.0.len(), 4);
    assert_eq!(separation_space.0[0].as_name(), Some("Separation"));
    assert_eq!(separation_space.0[1].as_name(), Some("Orange"));
    assert_eq!(separation_space.0[2].as_name(), Some("DeviceCMYK"));
    let tint = separation_space.0[3]
        .as_dict()
        .expect("Type 2 tint transform should be an inline dictionary");
    assert_eq!(
        tint.get("FunctionType").and_then(Value::as_integer),
        Some(2)
    );
    let c1 = tint.get("C1").and_then(Value::as_array).expect("C1");
    assert_eq!(c1.0[1], Value::Float(0.4));
    assert_eq!(
        reparsed
            .decode_stream_bytes(&separation_object.value)
            .expect("decode separation image"),
        vec![0x00, 0xFF]
    );

    let devicen_object = image_object("Im2");
    let Value::Stream(devicen_dict, _) = &devicen_object.value else {
        panic!("DeviceN image should be a stream");
    };
    let devicen_space = devicen_dict
        .get("ColorSpace")
        .and_then(Value::as_array)
        .expect("DeviceN ColorSpace should be an array");
    assert_eq!(devicen_space.0.len(), 4);
    assert_eq!(devicen_space.0[0].as_name(), Some("DeviceN"));
    let names = devicen_space.0[1]
        .as_array()
        .expect("DeviceN names should be an array");
    let names: Vec<&str> = names.0.iter().filter_map(Value::as_name).collect();
    assert_eq!(names, vec!["Orange", "Green"]);
    assert_eq!(devicen_space.0[2].as_name(), Some("DeviceRGB"));
    let (function_num, _) = devicen_space.0[3]
        .as_ref()
        .expect("Type 0 tint transform should be an indirect stream");
    let function_object = reparsed
        .get_object(function_num)
        .expect("get function object")
        .expect("function stream should exist");
    let Value::Stream(function_dict, _) = &function_object.value else {
        panic!("Type 0 function should be a stream");
    };
    assert_eq!(
        function_dict
            .get("FunctionType")
            .and_then(Value::as_integer),
        Some(0)
    );
    let size = function_dict
        .get("Size")
        .and_then(Value::as_array)
        .expect("Size");
    let size: Vec<i64> = size.0.iter().filter_map(Value::as_integer).collect();
    assert_eq!(size, vec![2, 2]);
    assert_eq!(
        reparsed
            .decode_stream_bytes(&function_object.value)
            .expect("decode sampled function")
            .len(),
        12,
        "2×2 采样点 × 3 个输出"
    );
    assert_eq!(
        reparsed
            .decode_stream_bytes(&devicen_object.value)
            .expect("decode devicen image"),
        vec![0x10, 0x20, 0x30, 0x40]
    );

    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("SPOT"));
}

/// 严出校验：`/Matte` 需要 PDF 1.6，降级到 1.5 目标应报错且零字节。
#[test]
fn builder_soft_mask_matte_requires_pdf_1_6_target() {
    use pdf_engine::core::builder::{
        ColorSpace, FormXObject, GraphicsState, GroupXObject, PageSize, PdfBuilder, SoftMask,
    };
    use pdf_engine::core::content::ContentStream;

    let mut mask_content = ContentStream::new();
    mask_content
        .set_fill_gray(0.5)
        .rect(0.0, 0.0, 50.0, 50.0)
        .fill();
    let mask_form = FormXObject::new([0.0, 0.0, 50.0, 50.0], mask_content)
        .with_group(GroupXObject::transparency().with_color_space(ColorSpace::DeviceGray));

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_5);
    let page = builder.page(PageSize::A4);
    let mask_handle = builder.form_xobject(&mask_form);
    builder.set_graphics_state(
        page,
        &GraphicsState::new().with_soft_mask(SoftMask::alpha(mask_handle).with_matte([0.5])),
    );

    let mut buf = Vec::new();
    let error = builder
        .build(&mut buf)
        .expect_err("/Matte must not build at PDF 1.5");
    let message = error.to_string();
    assert!(
        message.contains("soft mask matte color") && message.contains("1.6"),
        "{message}"
    );
    assert!(buf.is_empty(), "构建失败时不应写出任何字节");
}

/// D7 写端图像补全：`/Intent`、`/Alternates`、`/Matte` 经写端组装后由
/// `page_image` 读回；替代图像解析为独立 `ImageModel` 且不占页面资源，
/// `/Matte` 落在 SMask 图像上并按基图分量数校验。
#[test]
fn builder_image_intent_alternates_and_matte_roundtrip() {
    use pdf_engine::core::builder::{
        ColorSpace, Font, ImageData, ImageIntent, PageSize, PdfBuilder,
    };

    let alternate = ImageData {
        width: 1,
        height: 1,
        color_space: ColorSpace::DeviceRGB,
        bits: 8,
        data: vec![10, 20, 30],
        decode: None,
        interpolate: false,
        intent: None,
        alternates: Vec::new(),
        oc: None,
        metadata: None,
        matte: None,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };
    let alpha = ImageData {
        width: 2,
        height: 2,
        color_space: ColorSpace::DeviceGray,
        bits: 8,
        data: vec![64, 128, 192, 255],
        decode: None,
        interpolate: false,
        intent: None,
        alternates: Vec::new(),
        oc: None,
        metadata: None,
        matte: None,
        smask: None,
        image_mask: false,
        mask: None,
        opi: None,
    };
    let img = ImageData {
        width: 2,
        height: 2,
        color_space: ColorSpace::DeviceRGB,
        bits: 8,
        data: vec![128; 12],
        decode: None,
        interpolate: false,
        intent: Some(ImageIntent::Perceptual),
        alternates: vec![alternate.into()],
        oc: None,
        metadata: None,
        matte: Some(vec![0.25, 0.5, 0.75]),
        smask: Some(Box::new(alpha)),
        image_mask: false,
        mask: None,
        opi: None,
    };

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "EXTRA");
    builder.image(page, 72.0, 600.0, 100.0, 100.0, &img);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let model = reparsed
        .page_image(0, "Im1")
        .expect("page image lookup")
        .expect("Im1 should exist");
    assert_eq!(model.intent.as_deref(), Some("Perceptual"));
    assert_eq!(model.alternates.len(), 1, "one alternate image");
    assert_eq!(
        (
            model.alternates[0].image.width,
            model.alternates[0].image.height
        ),
        (1, 1)
    );
    let alternate_samples = model.alternates[0]
        .image
        .samples()
        .expect("alternate samples");
    assert_eq!(
        alternate_samples.pixel(0, 0),
        Some(&[10.0 / 255.0, 20.0 / 255.0, 30.0 / 255.0][..])
    );

    let smask = model.smask.as_ref().expect("Im1 should have an SMask");
    assert_eq!(smask.matte, Some(vec![0.25, 0.5, 0.75]));
    assert_eq!(
        smask.samples().expect("smask samples").values,
        vec![64.0 / 255.0, 128.0 / 255.0, 192.0 / 255.0, 1.0]
    );

    assert!(
        reparsed
            .page_image(0, "Im2")
            .expect("page image lookup")
            .is_none(),
        "alternate and SMask must not be page resources"
    );
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("EXTRA"));
}
