use super::*;

/// C5 读写模型统一往返：页面内容 → 读端 Op 模型 → `serialize_ops` →
/// 重新解析模型相等；再经 `to_content_op` 重建为写端 `ContentStream`
/// 表单写出，同样的文本仍可提取。
#[test]
fn content_model_roundtrip_preserves_text_extraction() {
    use pdf_engine::core::builder::{Font, FormXObject, PageSize, PdfBuilder};
    use pdf_engine::core::content::ContentStream;
    use pdf_engine::core::reader::content::{parse_content_stream, serialize_ops, to_content_op};

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 700.0, "Roundtrip");
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let doc = parse_buf(&buf);

    let ops = doc.page_content_ops(0).expect("page content ops");
    let rewritten = serialize_ops(&ops);
    let reparsed = parse_content_stream(&rewritten).expect("re-parse serialized content stream");
    assert_eq!(ops, reparsed, "模型往返应无损");
    assert_eq!(doc.extract_page_text(0).as_deref(), Some("Roundtrip"));

    // 模型 → 写端 Op → FormXObject 重建表单（继承页面资源）后文本仍可提取。
    let mut inner = ContentStream::new();
    for op in &ops {
        inner.push(to_content_op(op).expect("builder 生成的操作符应全部可映射"));
    }
    let form = FormXObject::new([0.0, 0.0, 842.0, 595.0], inner);
    let mut builder2 = PdfBuilder::new();
    let page2 = builder2.page(PageSize::A4);
    // 先在 page2 注册 /F1 资源，供模型引用的 Tf 名称解析。
    builder2.text(page2, &Font::helvetica(12.0), 72.0, 50.0, "Base");
    let handle = builder2.form_xobject(&form);
    builder2.draw_form(page2, handle, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    let mut buf2 = Vec::new();
    builder2.build(&mut buf2).expect("模型重建内容应可写出");
    let doc2 = parse_buf(&buf2);
    let text2 = doc2.extract_page_text(0).expect("重建后应有文本");
    assert!(text2.contains("Roundtrip"), "重建后提取文本: {text2:?}");
}

/// B7：位置片段 API 与 builder 写出的文本坐标一致，且平铺文本输出不变。
#[test]
fn builder_text_fragments_report_positions() {
    use pdf_engine::core::builder::{Font, PageSize, PdfBuilder};
    use pdf_engine::core::reader::text::TextFragment;

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let font = Font::helvetica(12.0);
    builder.text(page, &font, 72.0, 700.0, "First");
    builder.text(page, &font, 72.0, 680.0, "Second");
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    assert_eq!(
        reparsed
            .extract_page_text_fragments(0)
            .expect("page fragments"),
        vec![
            TextFragment {
                text: "First".into(),
                x: 72.0,
                y: 700.0,
            },
            TextFragment {
                text: "Second".into(),
                x: 72.0,
                y: 680.0,
            },
        ]
    );
    assert_eq!(
        reparsed.extract_page_text(0).as_deref(),
        Some("First\nSecond")
    );
}

/// 缺陷回归：畸形内容流（`Tf` 只给 1 个操作数）经 `Document::page_content_ops`
/// 入口不得 panic，应返回类型化解析错误。
#[test]
fn page_content_ops_returns_typed_error_for_truncated_operands() {
    fn doc_with_content(content: &'static [u8]) -> Document {
        let mut doc = Document::new();
        doc.set_object(Object::new(
            1,
            0,
            Value::Dict(
                Dict::new()
                    .insert("Type", Value::Name("Catalog".into()))
                    .insert("Pages", Value::Ref(2, 0)),
            ),
        ));
        doc.set_object(Object::new(
            2,
            0,
            Value::Dict(
                Dict::new()
                    .insert("Type", Value::Name("Pages".into()))
                    .insert("Kids", Value::Array(Array::new().push(Value::Ref(3, 0))))
                    .insert("Count", Value::Integer(1)),
            ),
        ));
        doc.set_object(Object::new(
            3,
            0,
            Value::Dict(
                Dict::new()
                    .insert("Type", Value::Name("Page".into()))
                    .insert("Parent", Value::Ref(2, 0))
                    .insert("Resources", Value::Dict(Dict::new()))
                    .insert("Contents", Value::Ref(4, 0)),
            ),
        ));
        doc.set_object(Object::new(
            4,
            0,
            Value::Stream(Dict::new(), StreamData::Owned(content.to_vec())),
        ));
        doc.trailer = Trailer {
            dict: Dict::new()
                .insert("Root", Value::Ref(1, 0))
                .insert("Size", Value::Integer(5)),
        };
        doc
    }

    // 原始缺陷形态：`Tf` 缺少字号操作数。
    let doc = doc_with_content(b"BT /F1 Tf (x) Tj ET");
    let error = doc
        .page_content_ops(0)
        .expect_err("truncated Tf must not parse");
    assert!(
        error.to_string().contains("Tf"),
        "错误应指明算子 Tf: {error}"
    );

    // 单操作数算子整族缺操作数同样只返回错误。
    for content in [
        &b"BT w ET"[..],
        &b"BT gs ET"[..],
        &b"BT Tj ET"[..],
        &b"BT ' ET"[..],
        &b"BT 1 2 \" ET"[..],
        &b"BT /F1 12 Tf 1 0 0 re ET"[..],
        &b"0 0 0 1 2 3 4 sh Do"[..],
    ] {
        let doc = doc_with_content(content);
        assert!(
            doc.page_content_ops(0).is_err(),
            "畸形内容流应返回错误: {}",
            String::from_utf8_lossy(content)
        );
    }
}

/// 单页文档：`/Resources /Font << /F1 Helvetica /F2 Times-Roman >>`
/// （标准 14 字体、无描述符），内容流为给定字节。
fn page_with_two_fonts(content: &[u8]) -> Document {
    fn standard_font(base: &str) -> Value {
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Font".into()))
                .insert("Subtype", Value::Name("Type1".into()))
                .insert("BaseFont", Value::Name(base.into())),
        )
    }
    let resources = Dict::new().insert(
        "Font",
        Value::Dict(
            Dict::new()
                .insert("F1", standard_font("Helvetica"))
                .insert("F2", standard_font("Times-Roman")),
        ),
    );
    let mut doc = Document::new();
    doc.set_object(Object::new(
        1,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Catalog".into()))
                .insert("Pages", Value::Ref(2, 0)),
        ),
    ));
    doc.set_object(Object::new(
        2,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Pages".into()))
                .insert("Kids", Value::Array(Array::new().push(Value::Ref(3, 0))))
                .insert("Count", Value::Integer(1)),
        ),
    ));
    doc.set_object(Object::new(
        3,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Page".into()))
                .insert("Parent", Value::Ref(2, 0))
                .insert("Resources", Value::Dict(resources))
                .insert(
                    "Contents",
                    Value::Stream(Dict::new(), StreamData::Owned(content.to_vec())),
                ),
        ),
    ));
    doc.trailer = Trailer {
        dict: Dict::new()
            .insert("Root", Value::Ref(1, 0))
            .insert("Size", Value::Integer(7)),
    };
    doc
}

/// Table 332 `FontSize`（14.8.2.4.3）：由显示时刻文本矩阵推导。
fn derived_font_size(doc: &Document, name: &str) -> Option<f64> {
    doc.page_font_character_property(0, name)
        .unwrap_or_else(|error| panic!("{name} 的属性推导应成功: {error}"))
        .font_size
}

/// 片段的 `(text, x, y)` 三元组，便于逐条断言。
fn positioned(doc: &Document) -> Vec<(String, f64, f64)> {
    doc.extract_page_text_fragments(0)
        .expect("页面片段")
        .into_iter()
        .map(|fragment| (fragment.text, fragment.x, fragment.y))
        .collect()
}

/// 缺陷回归：`q`/`Q` 保存并恢复文本矩阵（8.4.2「push a copy of the entire
/// graphics state」/「restore the entire graphics state」，配合 9.3.1 与
/// 9.4.1 把 `Tm`/`Tlm` 归入文本状态）。未改动 `Tm` 的 `q … Q` 对不得影响
/// 派生字号，也不得影响片段位置。
#[test]
fn neutral_save_restore_pair_leaves_font_size_and_positions_untouched() {
    let plain =
        page_with_two_fonts(b"BT /F1 12 Tf 1 0 0 1 72 700 Tm (Hi) Tj 0 -20 Td (There) Tj ET");
    let wrapped =
        page_with_two_fonts(b"BT /F1 12 Tf 1 0 0 1 72 700 Tm q (Hi) Tj Q 0 -20 Td (There) Tj ET");
    assert_eq!(derived_font_size(&plain, "F1"), Some(12.0));
    assert_eq!(
        derived_font_size(&wrapped, "F1"),
        derived_font_size(&plain, "F1"),
        "未动 Tm 的 q/Q 不应改变 Table 332 派生字号"
    );
    assert_eq!(
        positioned(&wrapped),
        positioned(&plain),
        "未动 Tm 的 q/Q 不应改变片段位置"
    );
}

/// 同一回归的另一半：`q` 区域改写了 `Tm`（3× 纵向缩放 + 平移）时，`Q`
/// 之后显示的时刻必须回到 `q` 之前的文本矩阵，故派生字号不再被区域内的
/// 缩放污染。
#[test]
fn font_size_after_restore_uses_the_restored_text_matrix() {
    let doc = page_with_two_fonts(
        b"BT /F1 12 Tf 1 0 0 1 72 700 Tm (a) Tj \
           q /F2 12 Tf 1 0 0 3 40 50 Tm (b) Tj Q (c) Tj ET",
    );
    assert_eq!(
        positioned(&doc),
        vec![
            ("a".to_owned(), 72.0, 700.0),
            ("b".to_owned(), 40.0, 50.0),
            ("c".to_owned(), 72.0, 700.0),
        ],
        "Q 后 Tm 回到 q 之前"
    );
    assert_eq!(
        derived_font_size(&doc, "F2"),
        Some(36.0),
        "区域内 12 × hypot(0, 3) = 36"
    );
    assert_eq!(
        derived_font_size(&doc, "F1"),
        Some(12.0),
        "Q 恢复 Tm 后 F1 的观测高度只能来自单位纵向缩放"
    );
}

/// 写端 → 解析 → 读端往返：表单内容流里 `q … Tm … Q` 写出的 PDF 重新解析
/// 后，`Q` 之后的显示仍落在 `q` 之前的文本矩阵位置。
#[test]
fn builder_form_roundtrip_restores_text_matrix_across_save_restore() {
    use pdf_engine::core::builder::{Font, FormXObject, PageSize, PdfBuilder};
    use pdf_engine::core::content::{ContentStream, Op};

    let mut inner = ContentStream::new();
    inner.push(Op::BeginText);
    inner.push(Op::SetFont {
        name: "F1".into(),
        size: 12.0,
    });
    inner.push(Op::SetTextMatrix([1.0, 0.0, 0.0, 1.0, 100.0, 200.0]));
    inner.push(Op::SaveState);
    inner.push(Op::SetTextMatrix([1.0, 0.0, 0.0, 1.0, 300.0, 400.0]));
    inner.push(Op::ShowText("Inside".into()));
    inner.push(Op::RestoreState);
    inner.push(Op::ShowText("After".into()));
    inner.push(Op::EndText);

    let form = FormXObject::new([0.0, 0.0, 595.0, 842.0], inner);
    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    // 注册页面的 /F1，供表单继承页面资源时的 Tf 名称解析。
    builder.text(page, &Font::helvetica(12.0), 72.0, 50.0, "Base");
    let handle = builder.form_xobject(&form);
    builder.draw_form(page, handle, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("写出内容流表单");
    let doc = parse_buf(&buf);

    let fragments = positioned(&doc);
    assert!(
        fragments.contains(&("Inside".to_owned(), 300.0, 400.0)),
        "q 区域内的显示: {fragments:?}"
    );
    assert!(
        fragments.contains(&("After".to_owned(), 100.0, 200.0)),
        "Q 之后的显示应回到 q 之前的 Tm: {fragments:?}"
    );
    assert!(
        fragments.contains(&("Base".to_owned(), 72.0, 50.0)),
        "页面自身文本不受表单影响: {fragments:?}"
    );
    let text = doc.extract_page_text(0).expect("页面文本");
    assert!(
        text.contains("Inside") && text.contains("After"),
        "往返后文本仍可提取: {text:?}"
    );
}
