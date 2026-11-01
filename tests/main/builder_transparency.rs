use super::*;

/// 写入 → 重新解析：Group XObject 的 `/Group` 字典应写出
/// `/Type /Group`、`/S /Transparency`、组颜色空间与 `/I`，表单仍可绘制
/// 并经 `Do` 提取内部文本。
#[test]
fn builder_group_xobject_roundtrips_through_parser() {
    use pdf_engine::core::builder::{
        ColorSpace, Font, FormXObject, GroupXObject, PageSize, PdfBuilder,
    };
    use pdf_engine::core::content::ContentStream;

    let mut inner = ContentStream::new();
    inner
        .begin_text()
        .set_font("F1", 12.0)
        .move_next_line(10.0, 20.0)
        .show_text("Group")
        .end_text();
    let form = FormXObject::new([0.0, 0.0, 100.0, 50.0], inner).with_group(
        GroupXObject::transparency()
            .with_color_space(ColorSpace::DeviceRGB)
            .with_isolated(true),
    );

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "GROUP");
    let handle = builder.form_xobject(&form);
    builder.draw_form(page, handle, [1.0, 0.0, 0.0, 1.0, 72.0, 600.0]);

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
    let form_object = reparsed
        .get_object(xobjects.get("Fm1").and_then(Value::as_ref).unwrap().0)
        .expect("get object")
        .expect("form object should exist");
    let Value::Stream(dict, _) = &form_object.value else {
        panic!("form should be a stream");
    };
    let group = dict
        .get("Group")
        .and_then(Value::as_dict)
        .expect("form should carry a Group dictionary");
    assert_eq!(group.get("Type").and_then(Value::as_name), Some("Group"));
    assert_eq!(
        group.get("S").and_then(Value::as_name),
        Some("Transparency")
    );
    assert_eq!(group.get("CS").and_then(Value::as_name), Some("DeviceRGB"));
    assert!(
        matches!(group.get("I"), Some(Value::Bool(true))),
        "isolated 组应写 /I true"
    );
    assert!(group.get("K").is_none(), "未设置 knockout 时不应写 /K");

    let extracted = reparsed.extract_page_text(0).expect("extract text");
    assert!(extracted.contains("GROUP"), "{extracted}");
    assert!(
        extracted.contains("Group"),
        "组内文本应经 Do 提取: {extracted}"
    );
}

/// 写入 → 重新解析：页字典应携带 `/Group` 透明组字典，页面文本仍可提取。
#[test]
fn builder_page_group_roundtrips_through_parser() {
    use pdf_engine::core::builder::{ColorSpace, Font, GroupXObject, PageSize, PdfBuilder};

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "PAGE");
    builder.set_page_group(
        page,
        &GroupXObject::transparency()
            .with_color_space(ColorSpace::DeviceRGB)
            .with_isolated(true)
            .with_knockout(true),
    );

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
    let group = page_dict
        .get("Group")
        .and_then(Value::as_dict)
        .expect("page should carry a Group dictionary");
    assert_eq!(group.get("Type").and_then(Value::as_name), Some("Group"));
    assert_eq!(
        group.get("S").and_then(Value::as_name),
        Some("Transparency")
    );
    assert_eq!(group.get("CS").and_then(Value::as_name), Some("DeviceRGB"));
    assert!(
        matches!(group.get("I"), Some(Value::Bool(true))),
        "isolated 页组应写 /I true"
    );
    assert!(
        matches!(group.get("K"), Some(Value::Bool(true))),
        "knockout 页组应写 /K true"
    );

    let extracted = reparsed.extract_page_text(0).expect("extract text");
    assert!(extracted.contains("PAGE"), "{extracted}");
}

/// 写入 → 重新解析：ExtGState `/SMask` 应被读端解析为类型化软蒙版，
/// `/G` 指向带 `/Group` 的 Form 流且不登记为页面 XObject 资源；
/// 页面文本仍可提取。
#[test]
fn builder_soft_mask_graphics_state_roundtrips_through_parser() {
    use pdf_engine::core::builder::{
        ColorSpace, Font, FormXObject, GraphicsState, GroupXObject, PageSize, PdfBuilder, SoftMask,
    };
    use pdf_engine::core::content::ContentStream;
    use pdf_engine::core::reader::graphics_state::SoftMaskSubtype;

    let mut mask_content = ContentStream::new();
    mask_content
        .set_fill_gray(0.5)
        .rect(0.0, 0.0, 50.0, 50.0)
        .fill();
    let mask_form = FormXObject::new([0.0, 0.0, 50.0, 50.0], mask_content)
        .with_group(GroupXObject::transparency().with_color_space(ColorSpace::DeviceGray));

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let handle = builder.form_xobject(&mask_form);
    builder.set_graphics_state(
        page,
        &GraphicsState::new()
            .with_fill_alpha(0.5)
            .with_soft_mask(SoftMask::luminosity(handle).with_backdrop([0.0, 0.5, 1.0])),
    );
    builder.text(page, &Font::helvetica(12.0), 72.0, 700.0, "Mask");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let page_dict = first_page_dict(&reparsed);
    let resources = page_dict
        .get("Resources")
        .and_then(Value::as_dict)
        .expect("/Resources");
    let states = reparsed
        .ext_g_states(resources)
        .expect("parse /ExtGState resources");
    let gs = states.get("GS1").expect("GS1 graphics state");
    let raw_mask = gs.soft_mask.as_ref().expect("GS1 should carry /SMask");
    let mask = reparsed.soft_mask(raw_mask).expect("typed soft mask");

    assert_eq!(mask.subtype, SoftMaskSubtype::Luminosity);
    assert_eq!(mask.backdrop, Some(vec![0.0, 0.5, 1.0]));
    let Value::Ref(group_num, _) = &mask.group else {
        panic!("/G should be an indirect reference");
    };
    let group_object = reparsed
        .get_object(*group_num)
        .expect("group lookup")
        .expect("group object");
    let Value::Stream(group_dict, _) = &group_object.value else {
        panic!("/G should reference a stream");
    };
    assert_eq!(
        group_dict.get("Subtype").and_then(Value::as_name),
        Some("Form")
    );
    assert_eq!(
        group_dict
            .get("Group")
            .and_then(Value::as_dict)
            .and_then(|group| group.get("S"))
            .and_then(Value::as_name),
        Some("Transparency")
    );
    assert!(
        !reparsed
            .decode_stream_bytes(&group_object.value)
            .expect("decode group stream")
            .is_empty(),
        "软蒙版表单应有内容流"
    );

    // 软蒙版表单不是页面绘制资源；页面只经 /GS1 gs 应用透明度。
    let page_xobjects = resources
        .get("XObject")
        .and_then(Value::as_dict)
        .map(|xobjects| xobjects.0.len())
        .unwrap_or(0);
    assert_eq!(page_xobjects, 0, "软蒙版表单不应登记为页面 XObject");

    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("Mask"));
}

/// 写入 → 重新解析：PDF 1.6 透明度扩展（/TR2 字典、/HT 半色调包装字典、
/// `/SMask` 的 /TR、/SMaskInData、/Matte、页/表单透明组）应写出并被读端
/// 解析为类型化模型；页面文本仍可提取。
#[test]
fn builder_transparency_extensions_roundtrip_through_parser() {
    use pdf_engine::core::builder::{
        ColorSpace, ExponentialFunction, Font, FormXObject, Function, GraphicsState, GroupXObject,
        Halftone, HalftoneDictionary, PageSize, PdfBuilder, SoftMask, Type1Halftone, Type2Halftone,
    };
    use pdf_engine::core::content::ContentStream;
    use pdf_engine::core::reader::graphics_state::{
        HalftoneModel, SoftMaskSubtype, Type1Halftone as RType1, Type2Halftone as RType2,
    };
    use pdf_engine::core::reader::group::GroupSubtype;

    let mut mask_content = ContentStream::new();
    mask_content
        .set_fill_gray(0.5)
        .rect(0.0, 0.0, 50.0, 50.0)
        .fill();
    let mask_form = FormXObject::new([0.0, 0.0, 50.0, 50.0], mask_content)
        .with_group(GroupXObject::transparency().with_color_space(ColorSpace::DeviceGray));

    let tr2 = Value::Dict(
        Dict::new()
            .insert("TransferType", Value::Name("HalftoneType".into()))
            .insert("TransferFunction0", Value::Name("identity".into())),
    );

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_6);
    let page = builder.page(PageSize::A4);
    let mask_handle = builder.form_xobject(&mask_form);
    let spot = builder.function(&Function::Exponential(ExponentialFunction::new(
        vec![0.0, 1.0],
        2.2,
    )));
    let halftone = HalftoneDictionary::new(1)
        .with_colorant(
            "C",
            Halftone::Type1(Type1Halftone {
                x_frequency: 65.0,
                angle: 15.0,
                spot_function: spot,
                transfer_function: None,
            }),
        )
        .with_colorant(
            "M",
            Halftone::Type2(Type2Halftone {
                frequency: 133.3,
                angle: 75.0,
                width: 0.5,
                uncompensated: true,
                spot_function: spot,
                transfer_function: None,
            }),
        );
    builder.set_graphics_state(
        page,
        &GraphicsState::new()
            .with_fill_alpha(0.5)
            .with_transfer_function_2(tr2.clone())
            .with_halftone(halftone)
            .with_soft_mask(
                SoftMask::luminosity(mask_handle)
                    .with_transfer_function(spot)
                    .with_smask_in_data(1)
                    .with_matte([0.2, 0.4, 0.6]),
            ),
    );
    builder.set_page_group(
        page,
        &GroupXObject::transparency()
            .with_color_space(ColorSpace::DeviceRGB)
            .with_isolated(true)
            .with_knockout(true),
    );
    builder.text(page, &Font::helvetica(12.0), 72.0, 700.0, "XTND");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("PDF 1.6 build");
    let reparsed = parse_buf(&buf);

    let page_dict = first_page_dict(&reparsed);
    let resources = page_dict
        .get("Resources")
        .and_then(Value::as_dict)
        .expect("/Resources");
    let states = reparsed
        .ext_g_states(resources)
        .expect("parse /ExtGState resources");
    let gs = states.get("GS1").expect("GS1 graphics state");

    assert_eq!(gs.transfer2, Some(tr2));

    let raw_halftone = gs.halftone.as_ref().expect("GS1 should carry /HT");
    let model = reparsed.halftone(raw_halftone).expect("typed halftone");
    match model {
        HalftoneModel::Composite(composite) => {
            assert_eq!(composite.default_type, Some(1));
            match composite.colorants.get("C") {
                Some(HalftoneModel::Type1(RType1 {
                    x_frequency,
                    angle,
                    transfer_function,
                    ..
                })) => {
                    assert_eq!(*x_frequency, 65.0);
                    assert_eq!(*angle, 15.0);
                    assert!(transfer_function.is_none());
                }
                other => panic!("colorant C should be typed, got {other:?}"),
            }
            match composite.colorants.get("M") {
                Some(HalftoneModel::Type2(RType2 {
                    frequency,
                    width,
                    uncompensated,
                    ..
                })) => {
                    assert!((frequency - 133.3).abs() < 1e-9);
                    assert_eq!(*width, 0.5);
                    assert!(*uncompensated);
                }
                other => panic!("colorant M should be typed, got {other:?}"),
            }
        }
        other => panic!("expected composite halftone, got {other:?}"),
    }

    let raw_mask = gs.soft_mask.as_ref().expect("GS1 should carry /SMask");
    let mask = reparsed.soft_mask(raw_mask).expect("typed soft mask");
    assert_eq!(mask.subtype, SoftMaskSubtype::Luminosity);
    assert_eq!(mask.smask_in_data, Some(1));
    assert_eq!(mask.matte, Some(vec![0.2, 0.4, 0.6]));
    let transfer = mask.transfer.as_ref().expect("/SMask /TR function");
    assert_eq!(
        transfer
            .as_dict()
            .and_then(|dict| dict.get("FunctionType"))
            .and_then(Value::as_integer),
        Some(2)
    );

    let group = reparsed
        .page_group(0)
        .expect("page group")
        .expect("page has /Group");
    assert_eq!(group.subtype, GroupSubtype::Transparency);
    assert!(group.isolated);
    assert_eq!(group.knockout, Some(true));
    assert!(matches!(
        group.color_space,
        Some(pdf_engine::core::reader::color_space::ColorSpaceModel::DeviceRGB)
    ));

    let Value::Stream(mask_dict, _) = reparsed
        .get_object(match &mask.group {
            Value::Ref(num, _) => *num,
            other => panic!("soft mask /G should stay a reference, got {other:?}"),
        })
        .expect("lookup")
        .expect("mask form object")
        .value
    else {
        panic!("/G references a stream");
    };
    let form_group = reparsed
        .group(mask_dict.get("Group").expect("form /Group"))
        .expect("typed form group");
    assert_eq!(form_group.subtype, GroupSubtype::Transparency);
    assert_eq!(
        form_group.color_space_raw,
        Some(Value::Name("DeviceGray".into()))
    );

    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("XTND"));
}
