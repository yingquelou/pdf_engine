use super::*;

/// 写入 → 重新解析：Type 4/5 网格渐变携带 `/Function` 时，每个顶点只
/// 编码一个参数变量，`/Decode` 只保留一对颜色范围；Type 0 函数写成
/// 独立流对象，Type 2 函数内联；页面仍用 `sh` 绘制且文本可提取。
#[test]
fn builder_mesh_shading_function_roundtrips_through_parser() {
    use pdf_engine::core::builder::{
        ColorSpace, ExponentialFunction, Font, FreeFormMeshShading, Function, LatticeMeshShading,
        MeshVertex, PageSize, PdfBuilder, SampledFunction, Shading,
    };

    let color_function = Function::Exponential(
        ExponentialFunction::new(vec![0.0, 1.0], 1.0)
            .with_c0(vec![1.0, 0.0, 0.0])
            .with_c1(vec![0.0, 0.0, 1.0]),
    );
    let free_form = FreeFormMeshShading::new(
        ColorSpace::DeviceRGB,
        vec![
            MeshVertex::new(10.0, 20.0, vec![0.0]),
            MeshVertex::new(90.0, 20.0, vec![0.5]),
            MeshVertex::new(50.0, 80.0, vec![1.0]),
        ],
    )
    .with_function(color_function)
    .with_decode([0.0, 100.0, 0.0, 100.0])
    .with_bits(8, 8, 8);
    let sampled = Function::Sampled(SampledFunction::new(
        vec![0.0, 1.0],
        vec![0.0, 1.0, 0.0, 1.0, 0.0, 1.0],
        vec![2],
        8,
        vec![0.0, 0.0, 0.0, 255.0, 255.0, 255.0],
    ));
    let lattice = LatticeMeshShading::new(
        ColorSpace::DeviceRGB,
        2,
        vec![
            MeshVertex::new(0.0, 0.0, vec![0.0]),
            MeshVertex::new(10.0, 0.0, vec![0.0]),
            MeshVertex::new(0.0, 10.0, vec![1.0]),
            MeshVertex::new(10.0, 10.0, vec![1.0]),
        ],
    )
    .with_function(sampled)
    .with_decode([0.0, 10.0, 0.0, 10.0])
    .with_bits(8, 8);

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "FUNC");
    let free_form_handle = builder.shading(&Shading::FreeForm(free_form));
    builder.paint_shading(page, free_form_handle);
    let lattice_handle = builder.shading(&Shading::Lattice(lattice));
    builder.paint_shading(page, lattice_handle);

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
    let shading_object = |name: &str| {
        let shading_num = shadings.get(name).and_then(Value::as_ref).unwrap().0;
        reparsed
            .get_object(shading_num)
            .expect("get shading object")
            .expect("shading object should exist")
    };

    let free_form_object = shading_object("Sh1");
    let free_form_dict = free_form_object.value.as_dict().expect("shading dict");
    assert_eq!(
        free_form_dict
            .get("ShadingType")
            .and_then(Value::as_integer),
        Some(4)
    );
    assert_eq!(
        free_form_dict
            .get("Decode")
            .and_then(Value::as_array)
            .map(|array| array.0.clone()),
        Some(
            [0.0, 100.0, 0.0, 100.0, 0.0, 1.0]
                .map(Value::Float)
                .to_vec()
        )
    );
    let inline_function = free_form_dict
        .get("Function")
        .and_then(Value::as_dict)
        .expect("Type 2 function should be inline");
    assert_eq!(
        inline_function
            .get("FunctionType")
            .and_then(Value::as_integer),
        Some(2)
    );
    assert_eq!(
        inline_function
            .get("C0")
            .and_then(Value::as_array)
            .map(|array| array.0.clone()),
        Some(vec![
            Value::Float(1.0),
            Value::Float(0.0),
            Value::Float(0.0)
        ])
    );
    assert_eq!(
        reparsed
            .decode_stream_bytes(&free_form_object.value)
            .expect("decode type 4 payload"),
        vec![0, 26, 51, 0, 0, 230, 51, 128, 0, 128, 204, 255]
    );

    let lattice_object = shading_object("Sh2");
    let lattice_dict = lattice_object.value.as_dict().expect("shading dict");
    assert_eq!(
        lattice_dict.get("ShadingType").and_then(Value::as_integer),
        Some(5)
    );
    assert!(!lattice_dict.0.contains_key("BitsPerFlag"));
    let Value::Ref(function_num, _) = lattice_dict.get("Function").expect("Function") else {
        panic!("Type 0 function must be an indirect stream");
    };
    let function_object = reparsed
        .get_object(*function_num)
        .expect("get function object")
        .expect("function object should exist");
    let function_dict = function_object.value.as_dict().expect("function dict");
    assert_eq!(
        function_dict
            .get("FunctionType")
            .and_then(Value::as_integer),
        Some(0)
    );
    assert_eq!(
        function_dict
            .get("Size")
            .and_then(Value::as_array)
            .map(|array| array.0.clone()),
        Some(vec![Value::Integer(2)])
    );
    assert_eq!(
        function_dict
            .get("BitsPerSample")
            .and_then(Value::as_integer),
        Some(8)
    );
    assert_eq!(
        reparsed
            .decode_stream_bytes(&function_object.value)
            .expect("decode sampled samples"),
        vec![0, 0, 0, 255, 255, 255]
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
    assert!(content.contains("/Sh1 sh"), "{content}");
    assert!(content.contains("/Sh2 sh"), "{content}");

    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("FUNC"));
}

/// 写入 → 重新解析：网格 `/Function` 可以是按颜色分量排列的函数数组；
/// 每个顶点仍只编码一个参数变量，数组内联三个 Type 2 函数。
#[test]
fn builder_mesh_function_array_roundtrips_through_parser() {
    use pdf_engine::core::builder::{
        ColorSpace, ExponentialFunction, Font, FreeFormMeshShading, Function, MeshVertex, PageSize,
        PdfBuilder, Shading,
    };

    let component = |c0: f64, c1: f64| {
        Function::Exponential(
            ExponentialFunction::new(vec![0.0, 1.0], 1.0)
                .with_c0(vec![c0])
                .with_c1(vec![c1]),
        )
    };
    let mesh = FreeFormMeshShading::new(
        ColorSpace::DeviceRGB,
        vec![
            MeshVertex::new(10.0, 20.0, vec![0.0]),
            MeshVertex::new(90.0, 20.0, vec![0.5]),
            MeshVertex::new(50.0, 80.0, vec![1.0]),
        ],
    )
    .with_component_functions(vec![
        component(0.0, 1.0),
        component(1.0, 0.0),
        component(0.0, 0.0),
    ])
    .with_decode([0.0, 100.0, 0.0, 100.0])
    .with_bits(8, 8, 8);

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "AFUN");
    let handle = builder.shading(&Shading::FreeForm(mesh));
    builder.paint_shading(page, handle);

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
    let shading_num = shadings.get("Sh1").and_then(Value::as_ref).unwrap().0;
    let shading_object = reparsed
        .get_object(shading_num)
        .expect("get shading object")
        .expect("shading object should exist");
    let shading_dict = shading_object.value.as_dict().expect("shading dict");
    assert_eq!(
        shading_dict.get("ShadingType").and_then(Value::as_integer),
        Some(4)
    );
    assert_eq!(
        shading_dict
            .get("Decode")
            .and_then(Value::as_array)
            .map(|array| array.0.clone()),
        Some(
            [0.0, 100.0, 0.0, 100.0, 0.0, 1.0]
                .map(Value::Float)
                .to_vec()
        )
    );
    let functions = shading_dict
        .get("Function")
        .and_then(Value::as_array)
        .expect("Function should be an array");
    assert_eq!(functions.0.len(), 3);
    for (value, (c0, c1)) in functions.0.iter().zip([(0.0, 1.0), (1.0, 0.0), (0.0, 0.0)]) {
        let function = value.as_dict().expect("component function dictionary");
        assert_eq!(
            function.get("FunctionType").and_then(Value::as_integer),
            Some(2)
        );
        assert_eq!(
            function
                .get("C0")
                .and_then(Value::as_array)
                .map(|array| array.0.clone()),
            Some(vec![Value::Float(c0)])
        );
        assert_eq!(
            function
                .get("C1")
                .and_then(Value::as_array)
                .map(|array| array.0.clone()),
            Some(vec![Value::Float(c1)])
        );
    }
    assert_eq!(
        reparsed
            .decode_stream_bytes(&shading_object.value)
            .expect("decode type 4 payload"),
        vec![0, 26, 51, 0, 0, 230, 51, 128, 0, 128, 204, 255],
        "参数变量编码应与单函数形式一致"
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
    assert!(content.contains("/Sh1 sh"), "{content}");

    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("AFUN"));
}

/// 写入 → 重新解析：Type 4 自由三角网格渐变应写成流对象，字典给出
/// 位深、`/BitsPerFlag` 与 `/Decode`；顶点按 MSB-first 位流编码，
/// 解码后的字节与量化结果一致，页面用 `sh` 绘制且文本仍可提取。
#[test]
fn builder_free_form_mesh_shading_roundtrips_through_parser() {
    use pdf_engine::core::builder::{
        ColorSpace, Font, FreeFormMeshShading, MeshVertex, PageSize, PdfBuilder, Shading,
    };

    let mesh = FreeFormMeshShading::new(
        ColorSpace::DeviceRGB,
        vec![
            MeshVertex::new(10.0, 20.0, vec![0.0, 0.0, 0.0]),
            MeshVertex::new(90.0, 20.0, vec![1.0, 0.0, 0.0]),
            MeshVertex::new(50.0, 80.0, vec![0.0, 0.0, 1.0]),
        ],
    )
    .with_decode([0.0, 100.0, 0.0, 100.0])
    .with_bits(8, 8, 8);

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "MESH");
    let handle = builder.shading(&Shading::FreeForm(mesh));
    builder.paint_shading(page, handle);

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
    let shading_num = shadings.get("Sh1").and_then(Value::as_ref).unwrap().0;
    let shading_object = reparsed
        .get_object(shading_num)
        .expect("get shading object")
        .expect("shading object should exist");
    let shading_dict = shading_object
        .value
        .as_dict()
        .expect("mesh shading should be a stream dictionary");
    assert_eq!(
        shading_dict.get("ShadingType").and_then(Value::as_integer),
        Some(4)
    );
    assert_eq!(
        shading_dict.get("ColorSpace").and_then(Value::as_name),
        Some("DeviceRGB")
    );
    assert_eq!(
        shading_dict
            .get("BitsPerCoordinate")
            .and_then(Value::as_integer),
        Some(8)
    );
    assert_eq!(
        shading_dict
            .get("BitsPerComponent")
            .and_then(Value::as_integer),
        Some(8)
    );
    assert_eq!(
        shading_dict.get("BitsPerFlag").and_then(Value::as_integer),
        Some(8)
    );
    let decode: Vec<Value> = shading_dict
        .get("Decode")
        .and_then(Value::as_array)
        .expect("Decode")
        .0
        .clone();
    assert_eq!(
        decode,
        [0.0, 100.0, 0.0, 100.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0]
            .map(Value::Float)
            .to_vec()
    );

    let payload = reparsed
        .decode_stream_bytes(&shading_object.value)
        .expect("decode mesh payload");
    assert_eq!(
        payload,
        vec![
            0, 26, 51, 0, 0, 0, // flag 0、x=10→26、y=20→51、黑色
            0, 230, 51, 255, 0, 0, // x=90→230、红色
            0, 128, 204, 0, 0, 255, // x=50→128、y=80→204、蓝色
        ]
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
    assert!(content.contains("/Sh1 sh"), "{content}");

    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("MESH"));
}
