//! E7 增量更新：`/Prev` 链、对象覆盖与删除条目的追加写出→解析往返。

use super::*;

/// 增量更新追加页面（E7 / 7.5.6）：builder 写单页文档 → 解析 →
/// 手工追加第二页并改写 Pages 节点 → `build_incremental` → 回读，
/// 两页均可解析且新页文本可提取；原文件字节为输出前缀。
#[test]
fn builder_incremental_update_appends_second_page() {
    use pdf_engine::core::builder::{Font, PageSize, PdfBuilder};
    use pdf_engine::core::writer::DocumentWriter;

    let mut builder = PdfBuilder::new();
    let p0 = builder.page(PageSize::A4);
    builder.text(p0, &Font::helvetica(24.0), 72.0, 700.0, "First");
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let doc = parse_buf(&buf);
    assert_eq!(doc.page_count().expect("page count"), 1);

    // 在已解析文档上做对象手术：复制第 1 页字典、新建内容流，
    // 追加进 Pages /Kids 并递增 /Count。
    let next_num = doc.xref.max_size();
    let page_num = next_num;
    let content_num = next_num + 1;
    let pages = doc.get_object(2).expect("pages").expect("pages node").value;
    let pages_dict = pages.as_dict().expect("pages dict");
    let kids = pages_dict
        .get("Kids")
        .and_then(|v| v.as_array())
        .expect("/Kids array");
    let first_page = doc
        .get_object(3)
        .expect("first page")
        .expect("page object")
        .value;
    let first_page_dict = first_page.as_dict().expect("page dict");

    let content_bytes = b"BT /F1 24 Tf 72 650 Td (Second) Tj ET\n".to_vec();
    let content_dict = Dict::new().insert("Length", Value::Integer(content_bytes.len() as i64));
    doc.set_object(Object::new(
        content_num,
        0,
        Value::Stream(content_dict, StreamData::Owned(content_bytes)),
    ));
    let mut new_page = Dict::new()
        .insert("Type", Value::Name("Page".into()))
        .insert("Parent", Value::Ref(2, 0))
        .insert("Contents", Value::Ref(content_num, 0));
    if let Some(v) = first_page_dict.get("MediaBox") {
        new_page = new_page.insert("MediaBox", v.clone());
    }
    if let Some(v) = first_page_dict.get("Resources") {
        new_page = new_page.insert("Resources", v.clone());
    }
    doc.set_object(Object::new(page_num, 0, Value::Dict(new_page)));
    let updated_pages = Value::Dict(
        pages_dict
            .clone()
            .insert(
                "Kids",
                Value::Array(kids.clone().push(Value::Ref(page_num, 0))),
            )
            .insert("Count", Value::Integer(kids.len() as i64 + 1)),
    );
    doc.set_object(Object::new(2, 0, updated_pages));

    let original_start = doc.startxref.0;
    let mut buf2 = Vec::new();
    DocumentWriter::new()
        .build_incremental(&doc, &mut buf2)
        .expect("incremental build");
    assert_eq!(&buf2[..buf.len()], &buf[..], "增量输出应以原文件为前缀");

    let revised = parse_buf(&buf2);
    assert_eq!(revised.page_count().expect("page count after update"), 2);
    assert_eq!(
        revised.trailer().dict.get("Prev"),
        Some(&Value::Integer(original_start)),
        "trailer /Prev 应指向原 startxref"
    );
    assert_eq!(revised.extract_page_text(0).as_deref(), Some("First"));
    assert_eq!(revised.extract_page_text(1).as_deref(), Some("Second"));
}

/// 增量更新删除与遮蔽（E7）：注解对象被删除后回读应消失，
/// 其余注解保留；输出可再次被完整重写（全量 rewrite 兼容增量文档）。
#[test]
fn builder_incremental_update_deletes_annotation_and_supports_full_rewrite() {
    use pdf_engine::core::builder::{AnnotationKind, Font, PageSize, PdfAnnotation, PdfBuilder};
    use pdf_engine::core::writer::DocumentWriter;

    let mut builder = PdfBuilder::new();
    let p0 = builder.page(PageSize::A4);
    builder.text(p0, &Font::helvetica(18.0), 72.0, 700.0, "Notes");
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Text, [72.0, 600.0, 92.0, 620.0])
            .with_contents("keep me"),
    );
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Text, [72.0, 500.0, 92.0, 520.0])
            .with_contents("drop me"),
    );
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let doc = parse_buf(&buf);
    let before = doc.page_annotations(0).expect("annotations before");
    assert_eq!(before.len(), 2);

    // 删除第二条注解：更新页面 /Annots 并把注解对象标记为删除。
    let dropped = match before[1].reference {
        Some((num, _)) => num,
        None => panic!("second annotation should be an indirect object"),
    };
    let kept_ref = match before[0].reference {
        Some((num, _)) => Value::Ref(num, 0),
        None => panic!("first annotation should be an indirect object"),
    };
    doc.delete_object(dropped);
    let page = doc.get_object(3).expect("page").expect("page object").value;
    let page_dict = page.as_dict().expect("page dict");
    let updated_page = Value::Dict(
        page_dict
            .clone()
            .insert("Annots", Value::Array(Array(vec![kept_ref]))),
    );
    doc.set_object(Object::new(3, 0, updated_page));

    let mut buf2 = Vec::new();
    DocumentWriter::new()
        .build_incremental(&doc, &mut buf2)
        .expect("incremental delete");
    let revised = parse_buf(&buf2);
    let after = revised.page_annotations(0).expect("annotations after");
    assert_eq!(after.len(), 1, "删除后仅剩一条注解");
    fn decode_text(value: &Value) -> String {
        use pdf_engine::core::types::PdfTextString;
        match value {
            Value::String(text) => text.clone(),
            Value::HexString(hex) => {
                let bytes: Vec<u8> = (0..hex.len() / 2)
                    .map(|i| u8::from_str_radix(&hex[i * 2..i * 2 + 2], 16).unwrap())
                    .collect();
                PdfTextString::decode(&bytes).unwrap().into_string()
            }
            other => panic!("expected text string, got {other:?}"),
        }
    }

    assert_eq!(
        after[0].contents.as_ref().map(decode_text),
        Some("keep me".to_string()),
        "保留的是第一条注解"
    );
    assert!(
        revised.get_object(dropped).expect("lookup").is_none(),
        "被删对象应被自由条目遮蔽"
    );

    // 全量重写增量文档：读端沿 /Prev 链加载全部对象后重新序列化。
    let mut flat = Vec::new();
    DocumentWriter::new()
        .build(&revised, &mut flat)
        .expect("full rewrite of incremental document");
    let rewritten = parse_buf(&flat);
    assert!(
        rewritten.trailer().dict.get("Prev").is_none(),
        "全量重写输出自包含，不应保留 /Prev 链"
    );
    assert!(
        rewritten.warnings().is_empty(),
        "重写文档回读不应产生告警: {:?}",
        rewritten.warnings()
    );
    let final_annots = rewritten.page_annotations(0).expect("annotations final");
    assert_eq!(final_annots.len(), 1);
    assert_eq!(rewritten.extract_page_text(0).as_deref(), Some("Notes"));
}
