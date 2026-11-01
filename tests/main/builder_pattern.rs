use super::*;

/// 写入 → 重新解析：平铺图案应作为流对象写出 `/Type /Pattern`、
/// `/PatternType 1`、`/PaintType 1`、`/TilingType`、`/BBox`、`/XStep`、
/// `/YStep`、`/Resources` 与图块内容流；页面 `/Pattern` 资源以 `/P1`
/// 引用且重复填充复用同一资源，文本仍可提取。
#[test]
fn builder_tiling_pattern_roundtrips_through_parser() {
    use pdf_engine::core::builder::{Font, PageSize, PdfBuilder, TilingPattern, TilingType};
    use pdf_engine::core::content::ContentStream;

    let mut tile = ContentStream::new();
    tile.set_fill_rgb(0.2, 0.4, 0.6)
        .rect(0.0, 0.0, 6.0, 6.0)
        .fill();
    let pattern = TilingPattern::new([0.0, 0.0, 12.0, 12.0], 12.0, 12.0, tile)
        .with_tiling_type(TilingType::ConstantSpacingAndSpeed)
        .with_matrix([1.0, 0.0, 0.0, 1.0, 3.0, 4.0]);

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "TILE");
    let handle = builder.tiling_pattern(&pattern);
    builder.fill_rect_with_pattern(page, handle, [72.0, 500.0, 120.0, 80.0]);
    builder.fill_rect_with_pattern(page, handle, [72.0, 300.0, 120.0, 80.0]);

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
    let patterns = resources
        .get("Pattern")
        .and_then(Value::as_dict)
        .expect("page should have Pattern resources");
    let pattern_num = patterns.get("P1").and_then(Value::as_ref).unwrap().0;
    assert!(
        patterns.get("P2").is_none(),
        "重复填充应复用 /P1: {patterns:?}"
    );

    let pattern_object = reparsed
        .get_object(pattern_num)
        .expect("get pattern object")
        .expect("pattern object should exist");
    let Value::Stream(pattern_dict, _) = &pattern_object.value else {
        panic!("tiling pattern should be a stream");
    };
    assert_eq!(
        pattern_dict.get("Type").and_then(Value::as_name),
        Some("Pattern")
    );
    assert_eq!(
        pattern_dict.get("PatternType").and_then(Value::as_integer),
        Some(1)
    );
    assert_eq!(
        pattern_dict.get("PaintType").and_then(Value::as_integer),
        Some(1)
    );
    assert_eq!(
        pattern_dict.get("TilingType").and_then(Value::as_integer),
        Some(3)
    );
    let bbox = pattern_dict
        .get("BBox")
        .and_then(Value::as_array)
        .expect("BBox");
    assert_eq!(bbox.0, [0.0, 0.0, 12.0, 12.0].map(Value::Float).to_vec());
    assert_eq!(pattern_dict.get("XStep"), Some(&Value::Float(12.0)));
    assert_eq!(pattern_dict.get("YStep"), Some(&Value::Float(12.0)));
    let matrix = pattern_dict
        .get("Matrix")
        .and_then(Value::as_array)
        .expect("Matrix");
    assert_eq!(
        matrix.0,
        [1.0, 0.0, 0.0, 1.0, 3.0, 4.0].map(Value::Float).to_vec()
    );
    let pattern_resources = pattern_dict
        .get("Resources")
        .and_then(Value::as_dict)
        .expect("pattern Resources");
    assert!(pattern_resources.0.is_empty(), "{pattern_resources:?}");

    let tile_content = String::from_utf8_lossy(
        &reparsed
            .decode_stream_bytes(&pattern_object.value)
            .expect("decode pattern stream"),
    )
    .into_owned();
    assert!(tile_content.contains("0.2 0.4 0.6 rg"), "{tile_content}");
    assert!(
        tile_content.contains("0.0 0.0 6.0 6.0 re"),
        "{tile_content}"
    );
    assert!(tile_content.contains("re\nf\n"), "{tile_content}");

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
    assert!(content.contains("/Pattern cs"), "{content}");
    assert_eq!(content.matches("/P1 scn").count(), 2, "{content}");
    assert!(content.contains("72.0 500.0 120.0 80.0 re"), "{content}");
    assert!(content.contains("72.0 300.0 120.0 80.0 re"), "{content}");

    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("TILE"));
}

/// 写入 → 重新解析：无色平铺图案应写出 `/PaintType 2` 与 `/ColorSpace`，
/// 图块内容不含颜色操作符；页面内容以 `c1 … cn /P1 scn` 提供填充色，
/// 文本仍可提取。
#[test]
fn builder_uncolored_tiling_pattern_roundtrips_through_parser() {
    use pdf_engine::core::builder::{ColorSpace, Font, PageSize, PdfBuilder, TilingPattern};
    use pdf_engine::core::content::ContentStream;

    let mut tile = ContentStream::new();
    tile.rect(0.0, 0.0, 6.0, 6.0).fill();
    let pattern = TilingPattern::uncolored(
        [0.0, 0.0, 12.0, 12.0],
        12.0,
        12.0,
        ColorSpace::DeviceRGB,
        tile,
    );

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "SPOT");
    let handle = builder.tiling_pattern(&pattern);
    builder.fill_rect_with_uncolored_pattern(
        page,
        handle,
        &[0.2, 0.4, 0.6],
        [72.0, 500.0, 120.0, 80.0],
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
    let resources = page_dict
        .get("Resources")
        .and_then(Value::as_dict)
        .expect("page resources");
    let patterns = resources
        .get("Pattern")
        .and_then(Value::as_dict)
        .expect("page should have Pattern resources");
    let pattern_num = patterns.get("P1").and_then(Value::as_ref).unwrap().0;

    let pattern_object = reparsed
        .get_object(pattern_num)
        .expect("get pattern object")
        .expect("pattern object should exist");
    let Value::Stream(pattern_dict, _) = &pattern_object.value else {
        panic!("tiling pattern should be a stream");
    };
    assert_eq!(
        pattern_dict.get("PaintType").and_then(Value::as_integer),
        Some(2)
    );
    assert_eq!(
        pattern_dict.get("PatternType").and_then(Value::as_integer),
        Some(1)
    );
    assert_eq!(
        pattern_dict.get("ColorSpace").and_then(Value::as_name),
        Some("DeviceRGB")
    );

    let tile_content = String::from_utf8_lossy(
        &reparsed
            .decode_stream_bytes(&pattern_object.value)
            .expect("decode pattern stream"),
    )
    .into_owned();
    assert_eq!(
        tile_content, "0.0 0.0 6.0 6.0 re\nf\n",
        "无色图案的图块不应包含颜色操作符"
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
    assert!(content.contains("/Pattern cs"), "{content}");
    assert!(content.contains("0.2 0.4 0.6 /P1 scn"), "{content}");
    assert!(content.contains("72.0 500.0 120.0 80.0 re"), "{content}");
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("SPOT"));
}

/// 写入 → 类型化读端模型：构建器写出的轴向渐变、自由形态网格、
/// 着色/无色平铺与着色图案应能经 `page_shading`/`page_pattern` 还原为
/// 坐标、域、Decode、位深、顶点、图案类型与图块内容字节。
#[test]
fn reader_shading_pattern_model_roundtrips_builder_output() {
    use pdf_engine::core::builder::{
        AxialShading, ColorSpace, ExponentialFunction, Font, FreeFormMeshShading, Function,
        MeshVertex, PageSize, PdfBuilder, RadialShading, Shading, ShadingPattern, TilingPattern,
    };
    use pdf_engine::core::content::ContentStream;
    use pdf_engine::core::reader::color_space::ColorSpaceModel;
    use pdf_engine::core::reader::shading::{PaintType, PatternModel, ShadingModel, TilingType};

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
        .with_domain([0.2, 0.8])
        .with_extend([true, false]),
    );
    let mesh = Shading::FreeForm(
        FreeFormMeshShading::new(
            ColorSpace::DeviceRGB,
            vec![
                MeshVertex::new(0.0, 0.0, vec![0.0, 0.0, 0.0]),
                MeshVertex::new(100.0, 0.0, vec![1.0, 0.0, 0.0]),
                MeshVertex::new(0.0, 100.0, vec![0.0, 1.0, 0.0]),
            ],
        )
        .with_decode([0.0, 100.0, 0.0, 100.0]),
    );
    let pattern = ShadingPattern::new(Shading::Radial(
        RadialShading::new(
            ColorSpace::DeviceRGB,
            [50.0, 50.0, 0.0, 50.0, 50.0, 80.0],
            exponential(vec![1.0, 0.0, 0.0], vec![0.0, 0.0, 1.0]),
        )
        .with_extend([true, true]),
    ))
    .with_matrix([1.0, 0.0, 0.0, 1.0, 10.0, 20.0]);

    let mut colored_tile = ContentStream::new();
    colored_tile
        .set_fill_rgb(0.2, 0.4, 0.6)
        .rect(0.0, 0.0, 6.0, 6.0)
        .fill();
    let colored = TilingPattern::new([0.0, 0.0, 12.0, 12.0], 12.0, 12.0, colored_tile);
    let mut spot_tile = ContentStream::new();
    spot_tile.rect(0.0, 0.0, 6.0, 6.0).fill();
    let uncolored = TilingPattern::uncolored(
        [0.0, 0.0, 12.0, 12.0],
        12.0,
        12.0,
        ColorSpace::DeviceRGB,
        spot_tile,
    );

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "D5");
    let axial_handle = builder.shading(&axial);
    builder.paint_shading(page, axial_handle);
    let mesh_handle = builder.shading(&mesh);
    builder.paint_shading(page, mesh_handle);
    let shading_pattern_handle = builder.shading_pattern(&pattern);
    builder.fill_rect_with_pattern(page, shading_pattern_handle, [72.0, 500.0, 120.0, 80.0]);
    let colored_handle = builder.tiling_pattern(&colored);
    builder.fill_rect_with_pattern(page, colored_handle, [72.0, 400.0, 120.0, 80.0]);
    let uncolored_handle = builder.tiling_pattern(&uncolored);
    builder.fill_rect_with_uncolored_pattern(
        page,
        uncolored_handle,
        &[0.2, 0.4, 0.6],
        [72.0, 300.0, 120.0, 80.0],
    );

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let axial_model = reparsed
        .page_shading(0, "Sh1")
        .expect("page_shading Sh1")
        .expect("Sh1 exists");
    let ShadingModel::Axial(a) = axial_model else {
        panic!("expected axial shading, got {axial_model:?}");
    };
    assert_eq!(a.coords, [0.0, 0.0, 200.0, 0.0]);
    assert_eq!(a.domain, vec![(0.2, 0.8)]);
    assert_eq!(a.extend, [true, false]);
    assert_eq!(a.color_space, ColorSpaceModel::DeviceGray);

    let mesh_model = reparsed
        .page_shading(0, "Sh2")
        .expect("page_shading Sh2")
        .expect("Sh2 exists");
    let ShadingModel::FreeFormMesh(m) = mesh_model else {
        panic!("expected free form mesh, got {mesh_model:?}");
    };
    assert_eq!(m.bits_per_coordinate, 16);
    assert_eq!(m.bits_per_components, 8);
    assert_eq!(m.bits_per_flag, 8);
    assert_eq!(
        m.decode,
        vec![
            (0.0, 100.0),
            (0.0, 100.0),
            (0.0, 1.0),
            (0.0, 1.0),
            (0.0, 1.0)
        ]
    );
    assert_eq!(m.vertices.len(), 3);
    assert_eq!(m.vertices[0].flag, 0);
    assert_eq!(m.vertices[1].x, 100.0);
    assert_eq!(m.vertices[1].components, vec![1.0, 0.0, 0.0]);
    assert_eq!(m.vertices[2].y, 100.0);
    assert_eq!(m.vertices[2].components, vec![0.0, 1.0, 0.0]);

    let shading_pattern = reparsed
        .page_pattern(0, "P1")
        .expect("page_pattern P1")
        .expect("P1 exists");
    let PatternModel::Shading(sp) = shading_pattern else {
        panic!("expected shading pattern, got {shading_pattern:?}");
    };
    assert_eq!(sp.matrix, Some([1.0, 0.0, 0.0, 1.0, 10.0, 20.0]));
    let ShadingModel::Radial(r) = sp.shading else {
        panic!("expected radial shading inside pattern");
    };
    assert_eq!(r.coords, [50.0, 50.0, 0.0, 50.0, 50.0, 80.0]);
    assert_eq!(r.domain, vec![(0.0, 1.0)]);
    assert_eq!(r.extend, [true, true]);
    assert_eq!(r.color_space, ColorSpaceModel::DeviceRGB);

    let colored_pattern = reparsed
        .page_pattern(0, "P2")
        .expect("page_pattern P2")
        .expect("P2 exists");
    let PatternModel::Tiling(t) = colored_pattern else {
        panic!("expected tiling pattern, got {colored_pattern:?}");
    };
    assert_eq!(t.paint_type, PaintType::Colored);
    assert_eq!(t.tiling_type, TilingType::ConstantSpacing);
    assert_eq!(t.bbox, [0.0, 0.0, 12.0, 12.0]);
    assert_eq!((t.x_step, t.y_step), (12.0, 12.0));
    assert!(t.matrix.is_none());
    assert!(t.color_space.is_none());
    let tile_text = String::from_utf8(t.content).expect("tile content is utf-8");
    assert!(
        tile_text.contains("0.2 0.4 0.6"),
        "着色图块应保留颜色操作符: {tile_text}"
    );

    let uncolored_pattern = reparsed
        .page_pattern(0, "P3")
        .expect("page_pattern P3")
        .expect("P3 exists");
    let PatternModel::Tiling(t) = uncolored_pattern else {
        panic!("expected tiling pattern, got {uncolored_pattern:?}");
    };
    assert_eq!(t.paint_type, PaintType::Uncolored);
    assert_eq!(
        t.color_space,
        Some(ColorSpaceModel::DeviceRGB),
        "无色平铺应带回基色空间"
    );
    let spot_text = String::from_utf8(t.content).expect("spot tile is utf-8");
    assert!(
        !spot_text.contains("0.2 0.4 0.6"),
        "无色图块不应含颜色操作符: {spot_text}"
    );

    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("D5"));
}
