//! 反序显示串（ISO 32000-1 14.8.2.3.3）的写 → 解析 → 文本提取往返：
//! `PdfBuilder::{begin_reversed_chars, end_reversed_chars}` 在 PDF 1.4 上
//! 产出 `/ReversedChars << /ReversedChars true >> BDC` + 反序 `Tj`，读端
//! `page_marked_content` 按标签识别区段、`page_reversed_chars` 还原逐串
//! 自然顺序、`extract_page_text` 输出自然顺序文本，特征画像登记
//! `PdfFeature::ReversedCharacters`。

use super::*;
use pdf_engine::core::builder::{Font, PageSize, PdfBuilder};
use pdf_engine::core::reader::accessibility::REVERSED_CHARS_TAG;
use pdf_engine::core::types::{PdfFeature, PdfVersion};

fn s(value: &str) -> String {
    value.to_owned()
}

#[test]
fn reversed_chars_roundtrip_restores_natural_order_through_parser() {
    let mut builder = PdfBuilder::with_version(PdfVersion::V1_4);
    let page = builder.page(PageSize::A4);
    builder.text(page, &Font::helvetica(12.0), 72.0, 720.0, "Lead ");
    builder.begin_reversed_chars(
        page,
        &Font::helvetica(12.0),
        72.0,
        700.0,
        &[s("Hello "), s("world")],
    );
    builder.end_reversed_chars(page);
    builder.text(page, &Font::helvetica(12.0), 72.0, 680.0, " later");

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let raw = String::from_utf8_lossy(&buf).into_owned();
    assert!(
        raw.contains("/ReversedChars << /ReversedChars true >> BDC"),
        "内容流应含 BDC 属性表形态"
    );
    assert!(
        raw.contains("( olleH) Tj") && raw.contains("(dlrow) Tj"),
        "{raw}"
    );

    let doc = parse_buf(&buf);

    let sections = doc.page_marked_content(0).expect("marked content");
    assert_eq!(sections.len(), 1, "{sections:?}");
    assert_eq!(sections[0].tag, REVERSED_CHARS_TAG);
    assert!(
        sections[0].is_reversed_chars(),
        "BDC 区段应按标签识别为反序显示串序列"
    );

    let sequences = doc.page_reversed_chars(0).expect("reversed chars");
    assert_eq!(sequences.len(), 1, "{sequences:?}");
    assert_eq!(
        sequences[0].strings,
        vec![s("Hello "), s("world")],
        "逐串反转、串间保持自然顺序"
    );
    assert!(
        sequences[0].diagnostics.is_empty(),
        "写端产出应全程合规: {:?}",
        sequences[0].diagnostics
    );
    assert_eq!(sequences[0].text(), s("Hello world"));

    assert_eq!(
        doc.extract_page_text(0).as_deref(),
        Some("Lead \nHello world\n later"),
        "提取器应把 ReversedChars 序列还原成自然顺序；不同 BT 块间由提取器插入换行"
    );

    assert!(
        doc.features
            .borrow()
            .contains(PdfFeature::ReversedCharacters),
        "读取标记内容时应登记 14.8.2.3.3 特性"
    );
}
