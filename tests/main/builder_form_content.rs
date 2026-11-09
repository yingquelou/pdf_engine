use super::*;

/// 写入 → 重新解析：Form XObject 应写出 `/Subtype /Form` 字典与内容流，
/// 页面资源用 `/Fm1` 引用并由 `Do` 绘制；表单未写 `/Resources` 时继承
/// 页面资源，内部文本仍可提取。
#[test]
fn builder_form_xobject_roundtrips_through_parser() {
    use pdf_engine::core::builder::{Font, FormXObject, PageSize, PdfBuilder};
    use pdf_engine::core::content::ContentStream;

    let mut inner = ContentStream::new();
    inner
        .save_state()
        .set_fill_rgb(0.2, 0.4, 0.6)
        .rect(0.0, 0.0, 100.0, 50.0)
        .fill()
        .restore_state()
        .begin_text()
        .set_font("F1", 12.0)
        .move_next_line(10.0, 20.0)
        .show_text("Inner")
        .end_text();
    let form = FormXObject::new([0.0, 0.0, 100.0, 50.0], inner)
        .with_matrix([1.0, 0.0, 0.0, 1.0, 5.0, 5.0]);

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "FORM");
    let handle = builder.form_xobject(&form);
    builder.draw_form(page, handle, [2.0, 0.0, 0.0, 2.0, 72.0, 600.0]);

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
    assert_eq!(dict.get("Subtype").and_then(Value::as_name), Some("Form"));
    assert_eq!(dict.get("FormType").and_then(Value::as_integer), Some(1));
    let bbox = dict
        .get("BBox")
        .and_then(Value::as_array)
        .expect("BBox should be an array");
    let bbox_values: Vec<i64> = bbox.0.iter().filter_map(Value::as_integer).collect();
    assert_eq!(bbox_values, vec![0, 0, 100, 50]);
    let matrix = dict
        .get("Matrix")
        .and_then(Value::as_array)
        .expect("Matrix should be an array");
    let matrix_values: Vec<i64> = matrix.0.iter().filter_map(Value::as_integer).collect();
    assert_eq!(matrix_values, vec![1, 0, 0, 1, 5, 5]);
    assert!(
        dict.get("Resources").is_none(),
        "表单未写 /Resources，应继承页面资源"
    );

    let stream_bytes = reparsed
        .decode_stream_bytes(&form_object.value)
        .expect("decode form stream");
    let stream_text = String::from_utf8_lossy(&stream_bytes);
    assert!(
        stream_text.contains("0.0 0.0 100.0 50.0 re"),
        "{stream_text}"
    );
    assert!(stream_text.contains("/F1 12.0 Tf"), "{stream_text}");
    assert!(stream_text.contains("(Inner) Tj"), "{stream_text}");

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
    assert!(content_text.contains("/Fm1 Do"), "{content_text}");
    assert!(
        content_text.contains("2.0 0.0 0.0 2.0 72.0 600.0 cm"),
        "{content_text}"
    );

    let extracted = reparsed.extract_page_text(0).expect("extract text");
    assert!(extracted.contains("FORM"), "{extracted}");
    assert!(
        extracted.contains("Inner"),
        "表单内文本应经 Do 提取: {extracted}"
    );
}

/// 写入 → 重新解析：标记内容操作符（`BMC`/`BDC`/`EMC`/`MP`/`DP`）应写入
/// 表单内容流，属性名与内联字典原样保留，标记范围内的文本仍可提取。
#[test]
fn builder_marked_content_operators_roundtrip_through_parser() {
    use pdf_engine::core::builder::{Font, FormXObject, PageSize, PdfBuilder};
    use pdf_engine::core::content::{ContentStream, MarkedContentProperties};
    use pdf_engine::core::types::{Dict, Value};

    let properties = Dict::new()
        .insert("Lang", Value::String("en".into()))
        .insert("MCID", Value::Integer(0));
    let mut inner = ContentStream::new();
    inner
        .begin_marked_content("P")
        .begin_marked_content_properties("Span", MarkedContentProperties::Dict(properties))
        .begin_text()
        .set_font("F1", 12.0)
        .move_next_line(10.0, 20.0)
        .show_text("Marked")
        .end_text()
        .end_marked_content()
        .end_marked_content()
        .begin_marked_content_properties("OC", MarkedContentProperties::Name("Layer1".into()))
        .marked_content_point_properties("Artifact", MarkedContentProperties::Name("Props1".into()))
        .end_marked_content();
    let form = FormXObject::new([0.0, 0.0, 100.0, 50.0], inner);

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "OUTER");
    let handle = builder.form_xobject(&form);
    builder.draw_form(page, handle, [1.0, 0.0, 0.0, 1.0, 72.0, 600.0]);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let form_object = (3..=64)
        .filter_map(|num| reparsed.get_object(num).ok().flatten())
        .find(|object| {
            object
                .value
                .as_dict()
                .and_then(|dict| dict.get("Subtype"))
                .and_then(Value::as_name)
                == Some("Form")
        })
        .expect("should find form object");
    let stream_text = String::from_utf8_lossy(
        &reparsed
            .decode_stream_bytes(&form_object.value)
            .expect("decode form stream"),
    )
    .into_owned();
    for expected in [
        "/P BMC\n",
        "/Span << /Lang (en) /MCID 0 >> BDC\n",
        "EMC\n",
        "/OC /Layer1 BDC\n",
        "/Artifact /Props1 DP\n",
        "(Marked) Tj",
    ] {
        assert!(
            stream_text.contains(expected),
            "missing {expected:?} in form stream: {stream_text}"
        );
    }

    let extracted = reparsed.extract_page_text(0).expect("extract text");
    assert!(extracted.contains("OUTER"), "{extracted}");
    assert!(
        extracted.contains("Marked"),
        "标记内容范围内的文本应可提取: {extracted}"
    );
}

/// 写入 → 重新解析：可选内容组应写出 `/OCG`、页资源 `/Properties` 与
/// 目录 `/OCProperties`；`/OC name BDC ... EMC` 标记范围内，默认可见组
/// 的文本仍可提取、默认隐藏组的文本被过滤，组名称、`/Intent` 与
/// 默认可见性可经读端类型化还原。
#[test]
fn builder_optional_content_roundtrips_through_parser() {
    use pdf_engine::core::builder::{Font, OptionalContentGroup, PageSize, PdfBuilder};
    use pdf_engine::core::reader::optional_content::BaseState;
    use pdf_engine::core::types::Value;

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let visible = builder.optional_content_group(&OptionalContentGroup::new("Visible Layer"));
    let hidden = builder.optional_content_group(
        &OptionalContentGroup::new("Hidden Layer")
            .with_visibility(false)
            .with_intent("View"),
    );
    builder.begin_optional_content(page, visible);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "VISIBLE");
    builder.end_optional_content(page);
    builder.begin_optional_content(page, hidden);
    builder.text(page, &Font::helvetica(12.0), 72.0, 700.0, "HIDDEN");
    builder.end_optional_content(page);
    builder.text(page, &Font::helvetica(12.0), 72.0, 680.0, "PLAIN");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let page_dict = first_page_dict(&reparsed);
    let resources = page_dict
        .get("Resources")
        .and_then(Value::as_dict)
        .expect("/Resources");
    let properties = resources
        .get("Properties")
        .and_then(Value::as_dict)
        .expect("page /Properties");
    let visible_object = properties
        .get("OC1")
        .and_then(Value::as_ref)
        .expect("OC1 reference")
        .0;
    let hidden_object = properties
        .get("OC2")
        .and_then(Value::as_ref)
        .expect("OC2 reference")
        .0;
    assert_ne!(visible_object, hidden_object);

    let parsed = reparsed
        .optional_content_properties()
        .expect("parse /OCProperties")
        .expect("expected /OCProperties");
    assert_eq!(parsed.groups.len(), 2);
    assert_eq!(parsed.groups[0].object, visible_object);
    assert_eq!(parsed.groups[0].name, "Visible Layer");
    assert!(parsed.groups[0].intent.is_empty());
    assert_eq!(parsed.groups[1].object, hidden_object);
    assert_eq!(parsed.groups[1].name, "Hidden Layer");
    assert_eq!(parsed.groups[1].intent, ["View"]);
    assert_eq!(parsed.is_visible(visible_object), Some(true));
    assert_eq!(parsed.is_visible(hidden_object), Some(false));
    let configuration = parsed.default.as_ref().expect("default configuration");
    assert_eq!(configuration.base_state, BaseState::On);
    assert!(configuration.on.contains(&visible_object));
    assert!(configuration.off.contains(&hidden_object));
    assert_eq!(configuration.order, [visible_object, hidden_object]);

    let Value::Ref(content_num, _) = page_dict.get("Contents").expect("/Contents") else {
        panic!("Contents should be an indirect reference");
    };
    let content_object = reparsed
        .get_object(*content_num)
        .expect("get content object")
        .expect("content stream");
    let content = String::from_utf8_lossy(
        &reparsed
            .decode_stream_bytes(&content_object.value)
            .expect("decode content"),
    )
    .into_owned();
    assert!(content.contains("/OC /OC1 BDC\n"), "{content}");
    assert!(content.contains("/OC /OC2 BDC\n"), "{content}");
    assert_eq!(content.matches("EMC\n").count(), 2, "{content}");

    let extracted = reparsed.extract_page_text(0).expect("extract text");
    for text in ["VISIBLE", "PLAIN"] {
        assert!(extracted.contains(text), "{text} missing: {extracted}");
    }
    assert!(
        !extracted.contains("HIDDEN"),
        "hidden layer should be filtered: {extracted}"
    );

    // C4：页标记内容还原出两个 /OC 区段，可见性分别跟随默认配置。
    let sections = reparsed.page_marked_content(0).expect("marked sections");
    assert_eq!(sections.len(), 2);
    assert_eq!(sections[0].tag, "OC");
    assert_eq!(sections[0].visibility, Some(true));
    assert_eq!(sections[1].tag, "OC");
    assert_eq!(sections[1].visibility, Some(false));
}

#[test]
fn builder_form_own_resources_roundtrip_through_parser() {
    use pdf_engine::core::builder::{
        ColorSpace, Font, FormResources, FormXObject, ImageData, OptionalContentGroup, PageSize,
        PdfBuilder,
    };
    use pdf_engine::core::content::ContentStream;

    let image = ImageData {
        width: 1,
        height: 1,
        color_space: ColorSpace::DeviceRGB,
        bits: 8,
        data: vec![40, 80, 120],
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
    let mut inner = ContentStream::new();
    inner
        .begin_text()
        .set_font("F1", 12.0)
        .move_next_line(10.0, 40.0)
        .show_text("OwnRes")
        .end_text();
    inner
        .save_state()
        .concat_matrix([20.0, 0.0, 0.0, 20.0, 10.0, 10.0])
        .draw_xobject("Im1")
        .restore_state();

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "PAGE");
    let group = builder.optional_content_group(&OptionalContentGroup::new("Form Layer"));
    let form = FormXObject::new([0.0, 0.0, 100.0, 50.0], inner)
        .with_resources(
            FormResources::new()
                .with_font(Font::helvetica(12.0))
                .with_image(image)
                .with_optional_content(group),
        )
        .with_optional_content(group);
    let handle = builder.form_xobject(&form);
    builder.draw_form(page, handle, [1.0, 0.0, 0.0, 1.0, 72.0, 600.0]);

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let page_dict = first_page_dict(&reparsed);
    let form_ref = page_dict
        .get("Resources")
        .and_then(Value::as_dict)
        .and_then(|resources| resources.get("XObject"))
        .and_then(Value::as_dict)
        .and_then(|xobjects| xobjects.get("Fm1"))
        .and_then(Value::as_ref)
        .expect("page should reference /Fm1");
    let form_object = reparsed
        .get_object(form_ref.0)
        .expect("form lookup")
        .expect("form object");
    let Value::Stream(form_dict, _) = &form_object.value else {
        panic!("form should be a stream");
    };
    assert_eq!(
        form_dict.get("Subtype").and_then(Value::as_name),
        Some("Form")
    );

    // 表单自有资源：字体与图片子字典。
    let resources = form_dict
        .get("Resources")
        .and_then(Value::as_dict)
        .expect("form should write /Resources");
    assert!(
        resources
            .get("Font")
            .and_then(Value::as_dict)
            .and_then(|fonts| fonts.get("F1"))
            .and_then(Value::as_ref)
            .is_some(),
        "form /Font/F1: {resources:?}"
    );
    assert!(
        resources
            .get("Properties")
            .and_then(Value::as_dict)
            .and_then(|properties| properties.get("OC1"))
            .and_then(Value::as_ref)
            .is_some(),
        "form /Properties/OC1: {resources:?}"
    );
    let image_ref = resources
        .get("XObject")
        .and_then(Value::as_dict)
        .and_then(|xobjects| xobjects.get("Im1"))
        .cloned()
        .expect("form image /Im1");
    let image_model = reparsed.image(&image_ref).expect("parse form image");
    assert_eq!((image_model.width, image_model.height), (1, 1));
    assert_eq!(
        image_model.samples().expect("image samples").pixel(0, 0),
        Some(&[40.0 / 255.0, 80.0 / 255.0, 120.0 / 255.0][..])
    );

    // 表单 /OC 引用同一 OCG，且目录 /OCProperties 包含该组。
    let Value::Ref(form_group, _) = form_dict.get("OC").expect("form /OC") else {
        panic!("form /OC should be an indirect reference");
    };
    assert_eq!(
        resources
            .get("Properties")
            .and_then(Value::as_dict)
            .and_then(|properties| properties.get("OC1"))
            .and_then(Value::as_ref)
            .map(|(num, _)| num),
        Some(*form_group),
        "form /OC and /Properties/OC1 should reference the same OCG"
    );
    let oc_properties = reparsed
        .optional_content_properties()
        .expect("parse /OCProperties")
        .expect("expected /OCProperties");
    assert_eq!(
        oc_properties
            .group(*form_group)
            .map(|group| group.name.as_str()),
        Some("Form Layer")
    );

    // 文本提取经表单自有资源解析字体（页面与表单文字都可提取）。
    let extracted = reparsed.extract_page_text(0).expect("extract text");
    assert!(extracted.contains("PAGE"), "{extracted}");
    assert!(extracted.contains("OwnRes"), "{extracted}");
}
