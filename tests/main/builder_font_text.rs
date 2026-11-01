use super::*;

/// 写入 → 重新解析 → 提取：builder 写端的自定义 `/Differences`
/// 应经读端编码解析后反映到文本提取。
#[test]
fn builder_custom_encoding_roundtrips_through_parser() {
    use pdf_engine::core::builder::{Font, FontEncoding, PageSize, PdfBuilder};

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    let font = Font::helvetica(12.0).with_encoding(FontEncoding::with_differences(
        Some("WinAnsiEncoding"),
        vec![(65, "eacute".to_string()), (66, "egrave".to_string())],
    ));
    builder.text(page, &font, 72.0, 700.0, "éè");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");

    let reparsed = parse_buf(&buf);
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("éè"));
}

/// 写入 → 重新解析 → 提取：WinAnsi/MacRoman 的非 ASCII 文本应经写端
/// 字符码转换后，由读端按同一编码还原。
#[test]
fn builder_encodes_non_ascii_text_for_roundtrip() {
    use pdf_engine::core::builder::{Font, FontEncoding, PageSize, PdfBuilder};

    let mut builder = PdfBuilder::new();
    let win_page = builder.page(PageSize::A4);
    builder.text(win_page, &Font::helvetica(12.0), 72.0, 700.0, "Café €");
    let mac_page = builder.page(PageSize::A4);
    let mac_font = Font::helvetica(12.0).with_encoding(FontEncoding::named("MacRomanEncoding"));
    builder.text(mac_page, &mac_font, 72.0, 700.0, "Café €");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");

    let reparsed = parse_buf(&buf);
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("Café €"));
    assert_eq!(reparsed.extract_page_text(1).as_deref(), Some("Café €"));
}

/// 写入 → 重新解析：FontDescriptor 应可被读端解析并暴露度量。
#[test]
fn builder_font_descriptor_roundtrips_through_parser() {
    use pdf_engine::core::builder::{Font, FontDescriptor, PageSize, PdfBuilder};
    use pdf_engine::core::types::Value;

    let descriptor = FontDescriptor::new()
        .with_family("Helvetica")
        .with_ascent(718.0)
        .with_descent(-207.0)
        .with_cap_height(718.0)
        .with_stem_v(80.0)
        .with_flags(32);
    let font = Font::helvetica(12.0).with_descriptor(descriptor);

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &font, 72.0, 700.0, "Test");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");

    let reparsed = parse_buf(&buf);
    // 对象 3 是字体字典（Catalog=1, Pages=2, Page=3, ContentStream=4, Font=5, Descriptor=6）
    // 但实际上对象号分配可能不同，我们需要找到字体对象
    // 简单做法：遍历所有对象找到包含 FontDescriptor 引用的字体
    for num in 1..=20 {
        if let Ok(Some(obj)) = reparsed.get_object(num) {
            if let Value::Dict(dict) = &obj.value {
                if dict.get("Type").and_then(Value::as_name) == Some("Font") {
                    if dict.get("FontDescriptor").is_some() {
                        let type1 = reparsed
                            .type1_font(&Value::Dict(dict.clone()))
                            .expect("should parse font")
                            .expect("should be Type1");
                        let desc = type1.descriptor.expect("should have descriptor");
                        assert_eq!(desc.font_family.as_deref(), Some("Helvetica"));
                        assert_eq!(desc.ascent, Some(718.0));
                        assert_eq!(desc.descent, Some(-207.0));
                        assert_eq!(desc.cap_height, Some(718.0));
                        assert_eq!(desc.stem_v, Some(80.0));
                        assert_eq!(desc.flags, Some(32));
                        return;
                    }
                }
            }
        }
    }
    panic!("Font with FontDescriptor not found");
}

/// 写入 → 重新解析： `with_widths` 写出的 `/FirstChar`、`/LastChar` 与
/// `/Widths` 应被读端按同一数值还原，文本提取不受影响。
#[test]
fn builder_widths_roundtrip_through_parser() {
    use pdf_engine::core::builder::{Font, PageSize, PdfBuilder};

    let widths = [500.0, 600.5, 700.0];
    let font = Font::helvetica(12.0).with_widths(65, &widths);

    let mut builder = PdfBuilder::new();
    let page = builder.page(PageSize::A4);
    builder.text(page, &font, 72.0, 700.0, "AB");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let reparsed = parse_buf(&buf);

    let font_dict = find_font_dict(&reparsed, "Type1");
    let dict = font_dict.as_dict().expect("font dict");
    assert_eq!(dict.get("FirstChar").and_then(Value::as_integer), Some(65));
    assert_eq!(dict.get("LastChar").and_then(Value::as_integer), Some(67));
    let parsed = reparsed
        .type1_font(&font_dict)
        .expect("should parse font")
        .expect("should be Type1");
    assert_eq!(parsed.first_char, Some(65));
    assert_eq!(parsed.widths, widths.to_vec());
    assert_eq!(parsed.width_for(65), Some(500.0));
    assert_eq!(parsed.width_for(66), Some(600.5));
    assert_eq!(parsed.width_for(67), Some(700.0));
    assert_eq!(reparsed.extract_page_text(0).as_deref(), Some("AB"));
}
