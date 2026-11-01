use super::*;

/// 写入 → 重新解析：ExtGState `/TR` 应写出 Type 2 直接字典、Type 0 流对象
/// 与 Type 3 拼接函数（内联子函数），读端可再次构造并求值，文本提取不受影响。
#[test]
fn builder_transfer_functions_roundtrip_through_parser() {
    use pdf_engine::core::builder::{
        ExponentialFunction, Font, Function, GraphicsState, PageSize, PdfBuilder, SampledFunction,
        StitchingFunction,
    };
    use pdf_engine::core::functions::Function as ReaderFunction;

    // Type 2：y = x²。
    let exponential = Function::Exponential(
        ExponentialFunction::new(vec![0.0, 1.0], 2.0)
            .with_c0(vec![0.0])
            .with_c1(vec![1.0])
            .with_range(vec![0.0, 1.0]),
    );
    // Type 0：5 个 8 位样本，线性插值。
    let sampled = Function::Sampled(SampledFunction::new(
        vec![0.0, 1.0],
        vec![0.0, 1.0],
        vec![5],
        8,
        vec![0.0, 64.0, 128.0, 191.0, 255.0],
    ));
    // Type 3：前半段恒等，后半段放大 2 倍。
    let stitching = Function::Stitching(
        StitchingFunction::new(
            vec![0.0, 1.0],
            vec![
                Function::Exponential(ExponentialFunction::new(vec![0.0, 1.0], 1.0)),
                Function::Exponential(
                    ExponentialFunction::new(vec![0.0, 1.0], 1.0)
                        .with_c0(vec![0.0])
                        .with_c1(vec![2.0])
                        .with_range(vec![0.0, 2.0]),
                ),
            ],
            vec![0.5],
        )
        .with_encode(vec![0.0, 1.0, 0.0, 1.0]),
    );

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    for function in [&exponential, &sampled, &stitching] {
        let handle = builder.function(function);
        builder.set_graphics_state(page, &GraphicsState::new().with_transfer_function(handle));
    }
    builder.text(page, &Font::helvetica(12.0), 72.0, 700.0, "TR");

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

    let exp = ReaderFunction::from_value(
        states
            .get("GS1")
            .expect("GS1")
            .transfer
            .as_ref()
            .expect("/TR"),
        None,
    )
    .expect("parse type 2 /TR");
    assert_eq!(exp.function_type(), 2);
    assert!((exp.eval(&[0.5]).expect("eval")[0] - 0.25).abs() < 1e-9);

    let transfer = states
        .get("GS2")
        .expect("GS2")
        .transfer
        .as_ref()
        .expect("/TR");
    assert!(
        matches!(transfer, Value::Stream(_, _)),
        "Type 0 /TR should resolve to a stream, got {transfer:?}"
    );
    let dict = transfer.as_dict().expect("function stream dict");
    let samples = reparsed
        .decode_stream_bytes(transfer)
        .expect("decode sample stream");
    let sampled_reader = ReaderFunction::from_dict(dict, Some(&samples)).expect("parse type 0 /TR");
    assert_eq!(sampled_reader.function_type(), 0);
    let interpolated = sampled_reader.eval(&[0.5]).expect("eval")[0];
    assert!(
        (interpolated - 128.0 / 255.0).abs() < 1e-9,
        "unexpected interpolation: {interpolated}"
    );
    assert_eq!(sampled_reader.eval(&[0.0]).expect("eval"), vec![0.0]);
    assert_eq!(sampled_reader.eval(&[1.0]).expect("eval"), vec![1.0]);

    let stitch = ReaderFunction::from_value(
        states
            .get("GS3")
            .expect("GS3")
            .transfer
            .as_ref()
            .expect("/TR"),
        None,
    )
    .expect("parse type 3 /TR");
    assert_eq!(stitch.function_type(), 3);
    // 段内输入先归一到 [0,1]：0.25 → 0.5（恒等），0.75 → 0.5 → 1.0（放大 2 倍）。
    assert!((stitch.eval(&[0.25]).expect("eval")[0] - 0.5).abs() < 1e-9);
    assert!((stitch.eval(&[0.75]).expect("eval")[0] - 1.0).abs() < 1e-9);

    let extracted = reparsed.extract_page_text(0).expect("extract text");
    assert!(extracted.contains("TR"), "{extracted}");
}

/// 写入 → 重新解析：ExtGState 的 `/BG`/`/BG2`/`/UCR`/`/UCR2` 应引用
/// 同一个四输入单输出采样函数流，读端把四个条目保留为同一原始值。
#[test]
fn builder_black_generation_functions_roundtrip_through_parser() {
    use pdf_engine::core::builder::{
        Font, Function, GraphicsState, PageSize, PdfBuilder, SampledFunction,
    };

    let function = Function::Sampled(SampledFunction::new(
        vec![0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0],
        vec![0.0, 1.0],
        vec![2, 2, 2, 2],
        8,
        (0..16)
            .map(|value| value as f64 * 17.0)
            .collect::<Vec<f64>>(),
    ));

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let handle = builder.function(&function);
    builder.set_graphics_state(
        page,
        &GraphicsState::new()
            .with_black_generation(handle)
            .with_black_generation_2(handle)
            .with_undercolor_removal(handle)
            .with_undercolor_removal_2(handle),
    );
    builder.text(page, &Font::helvetica(12.0), 72.0, 700.0, "BG");

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
    let state = states.get("GS1").expect("GS1");

    let bg = state.black_generation.as_ref().expect("/BG");
    for (name, value) in [
        ("/BG2", state.black_generation2.as_ref()),
        ("/UCR", state.under_color_removal.as_ref()),
        ("/UCR2", state.under_color_removal2.as_ref()),
    ] {
        assert_eq!(
            value.expect(name),
            bg,
            "{name} should reference the same function"
        );
    }
    let dict = bg.as_dict().expect("function stream dictionary");
    assert_eq!(
        dict.get("FunctionType").and_then(Value::as_integer),
        Some(0)
    );
    assert_eq!(
        dict.get("Size")
            .and_then(Value::as_array)
            .map(|array| array.0.len()),
        Some(4)
    );
    assert_eq!(
        reparsed
            .decode_stream_bytes(bg)
            .expect("decode samples")
            .len(),
        16
    );

    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("BG"));
}

/// 写入 → 重新解析：builder 构造的 `/ExtGState` 资源应被读端完整解析，
/// 包括 `/D`、`/BM` 回退数组与 `/Font` 字体字典引用，同时不影响文本提取。
#[test]
fn builder_graphics_state_roundtrips_through_parser() {
    use pdf_engine::core::builder::{DashPattern, Font, GraphicsState, PageSize, PdfBuilder};
    use pdf_engine::core::reader::graphics_state::BlendMode;

    let font = Font::helvetica(12.0);
    let state = GraphicsState::new()
        .with_line_width(2.5)
        .with_line_cap(1)
        .with_line_join(2)
        .with_miter_limit(4.0)
        .with_dash_pattern(DashPattern::new(vec![2.0, 1.0], 0.5))
        .with_rendering_intent("RelativeColorimetric")
        .with_overprint(true)
        .with_nonstroking_overprint(false)
        .with_overprint_mode(1)
        .with_font(font.clone(), 18.0)
        .with_flatness(50.0)
        .with_smoothness(0.25)
        .with_stroke_adjustment(true)
        .with_blend_modes(&["Normal", "Compatible"])
        .with_stroke_alpha(0.8)
        .with_fill_alpha(0.6)
        .with_alpha_is_shape(true)
        .with_text_knockout(false);

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.set_graphics_state(page, &state);
    builder.text(page, &font, 72.0, 700.0, "Hello");

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

    assert_eq!(gs.line_width, Some(2.5));
    assert_eq!(gs.line_cap, Some(1));
    assert_eq!(gs.line_join, Some(2));
    assert_eq!(gs.miter_limit, Some(4.0));
    let dash = gs.dash_pattern.as_ref().expect("/D");
    assert_eq!(dash.array, vec![2.0, 1.0]);
    assert_eq!(dash.phase, 0.5);
    assert_eq!(gs.rendering_intent.as_deref(), Some("RelativeColorimetric"));
    assert_eq!(gs.overprint, Some(true));
    assert_eq!(gs.nonstroking_overprint, Some(false));
    assert_eq!(gs.overprint_mode, Some(1));
    assert_eq!(gs.flatness, Some(50.0));
    assert_eq!(gs.smoothness, Some(0.25));
    assert_eq!(gs.stroke_adjustment, Some(true));
    assert_eq!(
        gs.blend_mode,
        Some(BlendMode::Array(vec![
            "Normal".to_string(),
            "Compatible".to_string(),
        ]))
    );
    assert_eq!(gs.stroke_alpha, Some(0.8));
    assert_eq!(gs.fill_alpha, Some(0.6));
    assert_eq!(gs.alpha_is_shape, Some(true));
    assert_eq!(gs.text_knockout, Some(false));

    let font_entry = gs.font.as_ref().expect("/Font");
    assert_eq!(font_entry.size, 18.0);
    let num = font_entry.font.as_ref().expect("font reference").0;
    let font_object = reparsed
        .get_object(num)
        .expect("font lookup")
        .expect("font object");
    let font_dict = font_object.value.as_dict().expect("font dictionary");
    assert_eq!(font_dict.get("Type").and_then(Value::as_name), Some("Font"));

    // ISO 32000-1 Table A.1 无 ca/CA/BM 操作符：三者只能存在于
    // ExtGState 字典，内容流仅经 `/GS1 gs` 应用。
    let Value::Ref(content_num, _) = page_dict.get("Contents").expect("Contents") else {
        panic!("Contents should be an indirect reference");
    };
    let content_object = reparsed
        .get_object(*content_num)
        .expect("get content object")
        .expect("content stream");
    let content = reparsed
        .decode_stream_bytes(&content_object.value)
        .expect("decode content");
    let content_text = String::from_utf8_lossy(&content);
    assert!(content_text.contains("/GS1 gs\n"), "{content_text}");
    for inline in [" ca", " CA", " BM"] {
        assert!(
            !content_text.contains(inline),
            "内容流不得内联 {inline:?}: {content_text}"
        );
    }

    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("Hello"));
}
