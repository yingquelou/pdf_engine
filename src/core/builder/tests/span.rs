//! Span 属性表（`/Alt` `/ActualText` `/E` `/Lang`，ISO 32000-1
//! 14.9.2.1、14.9.3–14.9.5）的写端测试。
//!
//! 覆盖类型化构造与文本串编码、BDC/EMC 内容流输出、特征画像登记，
//! 以及 `/Lang` `/E` 的 PDF 1.4 门禁与 `/Alt` `/ActualText` 的
//! PDF 1.5 门禁；所有失败均要求零字节输出。

use super::*;
use crate::core::types::{Document, PdfFeature, PdfTextString, PdfVersion, Value};

fn language(value: &str) -> LanguageIdentifier {
    LanguageIdentifier::parse(value).unwrap_or_else(|error| panic!("{value:?}: {error}"))
}

/// 按写端相同的 PDF 文本串规则编码成大写十六进制。
fn hex_of(text: &str) -> String {
    let bytes = PdfTextString::from(text).encode();
    let mut hex = String::new();
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02X}");
    }
    hex
}

/// 从组装结果取第 `index` 页的内容流文本（Page 3/Content 4 起成对排列）。
fn content_stream(doc: &Document, index: i64) -> String {
    let content_num = 4 + index * 2;
    let object = doc
        .get_object(content_num)
        .expect("content lookup")
        .expect("content object");
    let bytes = doc.stream_bytes(&object.value).expect("content bytes");
    String::from_utf8_lossy(&bytes).into_owned()
}

/// 在单页上包一段 Span 并构建，返回构建结果与输出缓冲。
fn build_span(
    version: PdfVersion,
    properties: &SpanProperties,
) -> (Result<(), WriteError>, Vec<u8>) {
    let mut b = PdfBuilder::with_version(version);
    let p = b.page(PageSize::A4);
    b.begin_span(p, properties);
    b.end_span(p);
    let mut out = Vec::new();
    let result = b.build(&mut out);
    (result, out)
}

#[test]
fn span_properties_to_dict_encodes_typed_entries() {
    assert!(SpanProperties::new().is_empty());
    assert!(SpanProperties::new().to_dict().0.is_empty());

    let properties = SpanProperties::new()
        .with_alternate_text("图片")
        .with_actual_text("Actual")
        .with_expansion("Fig.")
        .with_language(language("zh-Hans"));
    assert!(!properties.is_empty());

    let dict = properties.to_dict();
    assert_eq!(dict.0.len(), 4, "{dict:?}");
    // 非 ASCII 文本走 UTF-16BE（BOM + 大端码元）的大写十六进制。
    assert_eq!(
        dict.get("Alt"),
        Some(&Value::HexString("FEFF56FE7247".into())),
        "UTF-16BE 十六进制应带 BOM 与大端码元"
    );
    assert_eq!(
        dict.get("ActualText"),
        Some(&Value::HexString(hex_of("Actual")))
    );
    assert_eq!(dict.get("E"), Some(&Value::HexString(hex_of("Fig."))));
    assert_eq!(dict.get("Lang"), Some(&Value::HexString(hex_of("zh-Hans"))));
    for key in ["Alt", "ActualText", "E", "Lang"] {
        assert!(
            matches!(dict.get(key), Some(Value::HexString(_))),
            "{key} 应为文本串（十六进制字符串）"
        );
    }
}

#[test]
fn span_property_versions_are_gated() {
    // PDF 1.3：四个条目全部拒绝，且失败时零字节输出。
    let cases = [
        (
            SpanProperties::new().with_language(language("en")),
            "span property lists",
            "1.4",
        ),
        (
            SpanProperties::new().with_expansion("Fig."),
            "span property lists",
            "1.4",
        ),
        (
            SpanProperties::new().with_alternate_text("Alt"),
            "span alternate and replacement text",
            "1.5",
        ),
        (
            SpanProperties::new().with_actual_text("Actual"),
            "span alternate and replacement text",
            "1.5",
        ),
    ];
    for (properties, feature, required) in cases {
        let (result, out) = build_span(PdfVersion::V1_3, &properties);
        let error = result.expect_err("PDF 1.3 must reject span properties");
        assert!(
            matches!(&error, WriteError::UnsupportedVersion(_)),
            "错误应为 UnsupportedVersion: {error}"
        );
        assert!(error.to_string().contains(feature), "{error}");
        assert!(error.to_string().contains(required), "{error}");
        assert!(out.is_empty(), "构建失败时不应写出任何字节");
    }

    // PDF 1.4：`/Lang` 与 `/E` 可用；`/Alt` 与 `/ActualText` 仍被拒绝。
    let (result, out) = build_span(
        PdfVersion::V1_4,
        &SpanProperties::new()
            .with_language(language("zh-Hans"))
            .with_expansion("Fig."),
    );
    result.expect("PDF 1.4 supports /Lang and /E");
    assert!(!out.is_empty(), "允许时应写出文档字节");

    for properties in [
        SpanProperties::new().with_alternate_text("Alt"),
        SpanProperties::new().with_actual_text("Actual"),
    ] {
        let (result, out) = build_span(PdfVersion::V1_4, &properties);
        let error = result.expect_err("PDF 1.4 must reject /Alt and /ActualText");
        assert!(
            matches!(&error, WriteError::UnsupportedVersion(_)),
            "错误应为 UnsupportedVersion: {error}"
        );
        assert!(
            error
                .to_string()
                .contains("span alternate and replacement text"),
            "{error}"
        );
        assert!(out.is_empty(), "构建失败时不应写出任何字节");
    }

    // PDF 1.5：四个条目全部允许。
    let (result, out) = build_span(
        PdfVersion::V1_5,
        &SpanProperties::new()
            .with_alternate_text("Alt")
            .with_actual_text("Actual")
            .with_expansion("Fig.")
            .with_language(language("en-US")),
    );
    result.expect("PDF 1.5 supports every span property entry");
    assert!(!out.is_empty(), "允许时应写出文档字节");
}

#[test]
fn begin_span_writes_bdc_and_feature_profile() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_5);
    let p = b.page(PageSize::A4);
    let properties = SpanProperties::new()
        .with_alternate_text("Logo")
        .with_actual_text("Logo")
        .with_expansion("Lg.")
        .with_language(language("en-US"));
    b.begin_span(p, &properties);
    b.text(p, &Font::helvetica(12.0), 72.0, 700.0, "ACME");
    b.end_span(p);
    // 未知页索引被忽略，不产生额外的 BDC/EMC。
    b.begin_span(7, &properties);
    b.end_span(7);

    let doc = b.assemble_document().expect("assemble");
    let content = content_stream(&doc, 0);
    assert_eq!(content.matches("/Span <<").count(), 1, "{content}");
    assert_eq!(content.matches("BDC\n").count(), 1, "{content}");
    assert_eq!(content.matches("EMC\n").count(), 1, "{content}");
    assert!(
        content.contains(&format!("/Alt <{}>", hex_of("Logo"))),
        "{content}"
    );
    assert!(
        content.contains(&format!("/ActualText <{}>", hex_of("Logo"))),
        "{content}"
    );
    assert!(
        content.contains(&format!("/E <{}>", hex_of("Lg."))),
        "{content}"
    );
    assert!(
        content.contains(&format!("/Lang <{}>", hex_of("en-US"))),
        "{content}"
    );

    let features = doc.features.borrow();
    assert!(
        features.contains(PdfFeature::SpanPropertyLists),
        "特征画像应登记 Span 属性表 /Lang /E"
    );
    assert!(
        features.contains(PdfFeature::SpanAlternateText),
        "特征画像应登记 Span /Alt /ActualText"
    );
}
