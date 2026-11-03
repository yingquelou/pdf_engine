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
