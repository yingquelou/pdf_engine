use super::*;

/// 写入 → 重新解析：渐变应写入页面 `/Shading` 资源，渐变字典带
/// `/ShadingType`、`/Coords`、`/Domain`、`/Extend` 与内联 Type 2 函数；
/// 着色图案写入 `/Pattern` 资源，`PatternType 2` 字典内联径向渐变；
/// 页面内容含 `sh`、`/Pattern cs` 与 `scn`，文本仍可提取。
#[test]
fn builder_shading_and_pattern_roundtrip_through_parser() {
    use pdf_engine::core::builder::{
        AxialShading, ColorSpace, ExponentialFunction, Font, Function, PageSize, PdfBuilder,
        RadialShading, Shading, ShadingPattern,
    };

    let exponential = |c0: Vec<f64>, c1: Vec<f64>| {
        Function::Exponential(
            ExponentialFunction::new(vec![0.0, 1.0], 1.0)
                .with_c0(c0)
                .with_c1(c1),
        )
    };
    let axial = Shading::Axial(
        AxialShading::new(
            ColorSpace::DeviceGray,
            [0.0, 0.0, 200.0, 0.0],
            exponential(vec![0.0], vec![1.0]),
        )
        .with_domain([0.0, 1.0])
        .with_extend([true, false]),
    );
    let radial = Shading::Radial(
        RadialShading::new(
            ColorSpace::DeviceRGB,
            [50.0, 50.0, 0.0, 50.0, 50.0, 80.0],
            exponential(vec![1.0, 0.0, 0.0], vec![0.0, 0.0, 1.0]),
        )
        .with_extend([true, true]),
    );
    let pattern = ShadingPattern::new(radial).with_matrix([1.0, 0.0, 0.0, 1.0, 10.0, 20.0]);

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "GRAD");
    let axial_handle = builder.shading(&axial);
    builder.paint_shading(page, axial_handle);
    builder.paint_shading(page, axial_handle);
    let pattern_handle = builder.shading_pattern(&pattern);
    builder.fill_rect_with_pattern(page, pattern_handle, [72.0, 500.0, 120.0, 80.0]);

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
    let resources = page_dict
        .get("Resources")
        .and_then(Value::as_dict)
        .expect("page resources");
    let shadings = resources
        .get("Shading")
        .and_then(Value::as_dict)
        .expect("page should have Shading resources");
    let patterns = resources
        .get("Pattern")
        .and_then(Value::as_dict)
        .expect("page should have Pattern resources");

    let shading_num = shadings.get("Sh1").and_then(Value::as_ref).unwrap().0;
    let shading_object = reparsed
        .get_object(shading_num)
        .expect("get shading object")
        .expect("shading object should exist");
    let shading_dict = shading_object.value.as_dict().expect("shading dict");
    assert_eq!(
        shading_dict.get("ShadingType").and_then(Value::as_integer),
        Some(2)
    );
    assert_eq!(
        shading_dict.get("ColorSpace").and_then(Value::as_name),
        Some("DeviceGray")
    );
    let coords: Vec<Value> = shading_dict
        .get("Coords")
        .and_then(Value::as_array)
        .expect("Coords")
        .0
        .clone();
    assert_eq!(coords, [0.0, 0.0, 200.0, 0.0].map(Value::Float).to_vec());
    let domain = shading_dict
        .get("Domain")
        .and_then(Value::as_array)
        .expect("Domain");
    assert_eq!(domain.0, vec![Value::Float(0.0), Value::Float(1.0)]);
    let extend = shading_dict
        .get("Extend")
        .and_then(Value::as_array)
        .expect("Extend");
    assert_eq!(extend.0, vec![Value::Bool(true), Value::Bool(false)]);
    let axial_function = shading_dict
        .get("Function")
        .and_then(Value::as_dict)
        .expect("Type 2 function should be inline");
    assert_eq!(
        axial_function
            .get("FunctionType")
            .and_then(Value::as_integer),
        Some(2)
    );
    assert_eq!(
        axial_function
            .get("C0")
            .and_then(Value::as_array)
            .map(|a| a.0.clone()),
        Some(vec![Value::Float(0.0)])
    );
    assert_eq!(
        axial_function
            .get("C1")
            .and_then(Value::as_array)
            .map(|a| a.0.clone()),
        Some(vec![Value::Float(1.0)])
    );

    let pattern_num = patterns.get("P1").and_then(Value::as_ref).unwrap().0;
    let pattern_object = reparsed
        .get_object(pattern_num)
        .expect("get pattern object")
        .expect("pattern object should exist");
    let pattern_dict = pattern_object.value.as_dict().expect("pattern dict");
    assert_eq!(
        pattern_dict.get("Type").and_then(Value::as_name),
        Some("Pattern")
    );
    assert_eq!(
        pattern_dict.get("PatternType").and_then(Value::as_integer),
        Some(2)
    );
    let matrix = pattern_dict
        .get("Matrix")
        .and_then(Value::as_array)
        .expect("Matrix");
    assert_eq!(
        matrix.0,
        [1.0, 0.0, 0.0, 1.0, 10.0, 20.0].map(Value::Float).to_vec()
    );
    let inline = pattern_dict
        .get("Shading")
        .and_then(Value::as_dict)
        .expect("pattern shading should be inline");
    assert_eq!(
        inline.get("ShadingType").and_then(Value::as_integer),
        Some(3)
    );
    assert_eq!(
        inline.get("ColorSpace").and_then(Value::as_name),
        Some("DeviceRGB")
    );
    let radial_coords: Vec<Value> = inline
        .get("Coords")
        .and_then(Value::as_array)
        .expect("radial Coords")
        .0
        .clone();
    assert_eq!(
        radial_coords,
        [50.0, 50.0, 0.0, 50.0, 50.0, 80.0]
            .map(Value::Float)
            .to_vec()
    );
    let radial_extend = inline
        .get("Extend")
        .and_then(Value::as_array)
        .expect("Extend");
    assert_eq!(radial_extend.0, vec![Value::Bool(true), Value::Bool(true)]);
    let radial_function = inline
        .get("Function")
        .and_then(Value::as_dict)
        .expect("radial function");
    assert_eq!(
        radial_function
            .get("C0")
            .and_then(Value::as_array)
            .map(|a| a.0.clone()),
        Some(vec![
            Value::Float(1.0),
            Value::Float(0.0),
            Value::Float(0.0)
        ])
    );
    assert_eq!(
        radial_function
            .get("C1")
            .and_then(Value::as_array)
            .map(|a| a.0.clone()),
        Some(vec![
            Value::Float(0.0),
            Value::Float(0.0),
            Value::Float(1.0)
        ])
    );

    let Value::Ref(content_num, _) = page_dict.get("Contents").expect("Contents") else {
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
    assert_eq!(content.matches("/Sh1 sh").count(), 2, "{content}");
    assert!(content.contains("/Pattern cs"), "{content}");
    assert!(content.contains("/P1 scn"), "{content}");
    assert!(content.contains("72.0 500.0 120.0 80.0 re"), "{content}");

    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("GRAD"));
}

/// 写入 → 重新解析：所有渐变共有的 `/Background`、`/BBox` 与
/// `/AntiAlias` 应同时出现在直接渐变字典与网格流渐变字典中，页面仍以
/// `sh` 绘制且文本可提取。
#[test]
fn builder_shading_options_roundtrip_through_parser() {
    use pdf_engine::core::builder::{
        AxialShading, ColorSpace, CoonsPatch, CoonsPatchShading, ExponentialFunction, Font,
        Function, PageSize, PdfBuilder, Shading, ShadingOptions,
    };

    let function = Function::Exponential(
        ExponentialFunction::new(vec![0.0, 1.0], 1.0)
            .with_c0(vec![0.0])
            .with_c1(vec![1.0]),
    );
    let axial = Shading::Axial(
        AxialShading::new(ColorSpace::DeviceGray, [0.0, 0.0, 200.0, 0.0], function).with_options(
            ShadingOptions::new()
                .with_background([0.5])
                .with_bbox([10.0, 20.0, 200.0, 100.0])
                .with_anti_alias(true),
        ),
    );
    let coons_points = [
        [0.0, 0.0],
        [10.0, 0.0],
        [20.0, 0.0],
        [30.0, 0.0],
        [30.0, 10.0],
        [30.0, 20.0],
        [30.0, 30.0],
        [20.0, 30.0],
        [10.0, 30.0],
        [0.0, 30.0],
        [0.0, 20.0],
        [0.0, 10.0],
    ];
    let coons = CoonsPatchShading::new(
        ColorSpace::DeviceGray,
        vec![CoonsPatch::new(
            coons_points,
            [vec![0.0], vec![1.0], vec![0.0], vec![1.0]],
        )],
    )
    .with_bits(8, 8, 8)
    .with_options(
        ShadingOptions::new()
            .with_background([1.0])
            .with_bbox([0.0, 0.0, 30.0, 30.0])
            .with_anti_alias(false),
    );

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "OPTS");
    let axial_handle = builder.shading(&axial);
    builder.paint_shading(page, axial_handle);
    let coons_handle = builder.shading(&Shading::Coons(coons));
    builder.paint_shading(page, coons_handle);

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
    let shadings = page_object
        .value
        .as_dict()
        .and_then(|dict| dict.get("Resources"))
        .and_then(Value::as_dict)
        .and_then(|resources| resources.get("Shading"))
        .and_then(Value::as_dict)
        .expect("page should have Shading resources");
    let shading_dict = |name: &str| {
        let shading_num = shadings.get(name).and_then(Value::as_ref).unwrap().0;
        reparsed
            .get_object(shading_num)
            .expect("get shading object")
            .expect("shading object should exist")
            .value
            .as_dict()
            .expect("shading dictionary")
            .clone()
    };

    let direct = shading_dict("Sh1");
    assert_eq!(
        direct.get("ShadingType").and_then(Value::as_integer),
        Some(2)
    );
    assert_eq!(
        direct
            .get("Background")
            .and_then(Value::as_array)
            .map(|array| array.0.clone()),
        Some(vec![Value::Float(0.5)])
    );
    assert_eq!(
        direct
            .get("BBox")
            .and_then(Value::as_array)
            .map(|array| array.0.clone()),
        Some([10.0, 20.0, 200.0, 100.0].map(Value::Float).to_vec())
    );
    assert_eq!(direct.get("AntiAlias"), Some(&Value::Bool(true)));

    let mesh = shading_dict("Sh2");
    assert_eq!(mesh.get("ShadingType").and_then(Value::as_integer), Some(6));
    assert_eq!(
        mesh.get("Background")
            .and_then(Value::as_array)
            .map(|array| array.0.clone()),
        Some(vec![Value::Float(1.0)])
    );
    assert_eq!(
        mesh.get("BBox")
            .and_then(Value::as_array)
            .map(|array| array.0.clone()),
        Some([0.0, 0.0, 30.0, 30.0].map(Value::Float).to_vec())
    );
    assert_eq!(mesh.get("AntiAlias"), Some(&Value::Bool(false)));

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
    assert!(content.contains("/Sh1 sh"), "{content}");
    assert!(content.contains("/Sh2 sh"), "{content}");

    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("OPTS"));
}

/// 写入 → 重新解析：Type 6（Coons）与 Type 7（张量积）网格渐变应写成
/// 流对象，包含 `/BitsPerFlag`、`/Decode` 与按 MSB-first 打包的网格片
/// 数据；页面两个 `/Shading` 资源分别由 `sh` 绘制，文本仍可提取。
#[test]
fn builder_patch_mesh_shading_roundtrips_through_parser() {
    use pdf_engine::core::builder::{
        ColorSpace, CoonsPatch, CoonsPatchShading, Font, PageSize, PdfBuilder, Shading,
        TensorProductPatch, TensorProductPatchShading,
    };

    let coons_points = [
        [0.0, 0.0],
        [10.0, 0.0],
        [20.0, 0.0],
        [30.0, 0.0],
        [30.0, 10.0],
        [30.0, 20.0],
        [30.0, 30.0],
        [20.0, 30.0],
        [10.0, 30.0],
        [0.0, 30.0],
        [0.0, 20.0],
        [0.0, 10.0],
    ];
    let tensor_points = [
        [0.0, 0.0],
        [10.0, 0.0],
        [20.0, 0.0],
        [30.0, 0.0],
        [0.0, 10.0],
        [10.0, 10.0],
        [20.0, 10.0],
        [30.0, 10.0],
        [0.0, 20.0],
        [10.0, 20.0],
        [20.0, 20.0],
        [30.0, 20.0],
        [0.0, 30.0],
        [10.0, 30.0],
        [20.0, 30.0],
        [30.0, 30.0],
    ];
    let colors = || {
        [
            vec![0.0, 0.0, 0.0],
            vec![1.0, 0.0, 0.0],
            vec![0.0, 1.0, 0.0],
            vec![0.0, 0.0, 1.0],
        ]
    };
    let coons = CoonsPatchShading::new(
        ColorSpace::DeviceRGB,
        vec![CoonsPatch::new(coons_points, colors())],
    )
    .with_bits(8, 8, 8);
    let tensor = TensorProductPatchShading::new(
        ColorSpace::DeviceRGB,
        vec![TensorProductPatch::new(tensor_points, colors())],
    )
    .with_bits(8, 8, 8);

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "PATCH");
    let coons_handle = builder.shading(&Shading::Coons(coons));
    let tensor_handle = builder.shading(&Shading::TensorProduct(tensor));
    builder.paint_shading(page, coons_handle);
    builder.paint_shading(page, tensor_handle);

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
    let shadings = page_dict
        .get("Resources")
        .and_then(Value::as_dict)
        .and_then(|resources| resources.get("Shading"))
        .and_then(Value::as_dict)
        .expect("page should have Shading resources");

    for (name, expected_type, expected_len) in [("Sh1", 6, 37), ("Sh2", 7, 45)] {
        let shading_num = shadings.get(name).and_then(Value::as_ref).unwrap().0;
        let shading_object = reparsed
            .get_object(shading_num)
            .expect("get shading object")
            .expect("shading object should exist");
        let dict = shading_object
            .value
            .as_dict()
            .expect("patch shading should be a stream dictionary");
        assert_eq!(
            dict.get("ShadingType").and_then(Value::as_integer),
            Some(expected_type)
        );
        assert_eq!(dict.get("BitsPerFlag").and_then(Value::as_integer), Some(8));
        assert_eq!(
            dict.get("BitsPerCoordinate").and_then(Value::as_integer),
            Some(8)
        );
        assert_eq!(
            dict.get("BitsPerComponent").and_then(Value::as_integer),
            Some(8)
        );
        assert_eq!(
            dict.get("Length").and_then(Value::as_integer),
            Some(expected_len)
        );
        let decode = dict
            .get("Decode")
            .and_then(Value::as_array)
            .expect("Decode");
        assert_eq!(
            decode.0[..4].to_vec(),
            [0.0, 30.0, 0.0, 30.0].map(Value::Float).to_vec()
        );
        let payload = reparsed
            .decode_stream_bytes(&shading_object.value)
            .expect("decode patch payload");
        assert_eq!(payload.len(), expected_len as usize);
        assert_eq!(payload[0], 0, "每个网格片的标志位应为 0");
        assert!(payload.iter().any(|&byte| byte != 0), "网格数据不应全为零");
    }

    let Value::Ref(content_num, _) = page_dict.get("Contents").expect("Contents") else {
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
    assert!(content.contains("/Sh1 sh"), "{content}");
    assert!(content.contains("/Sh2 sh"), "{content}");

    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("PATCH"));
}
