//! Artifact 属性表（`/Type` `/BBox` `/Attached` `/Subtype`，ISO 32000-1
//! 14.8.2.2.2 与 Table 330）的写端测试。
//!
//! 覆盖类型化构造与 Table 330 条目编码、BDC/EMC 内容流输出、特征画像
//! 登记，以及 Artifact 属性表的 PDF 1.4 门禁与 `/Type /Background`、
//! `/Subtype` 的 PDF 1.7 门禁；所有失败均要求零字节输出。

use super::*;
use crate::core::content::MarkedContentProperties;
use crate::core::types::{Array, Dict, PdfVersion, Value};

/// 在单页上包一段 Artifact 并构建，返回构建结果与输出缓冲。
fn build_artifact(
    version: PdfVersion,
    properties: &ArtifactProperties,
) -> (Result<(), WriteError>, Vec<u8>) {
    let mut builder = PdfBuilder::with_version(version);
    let page = builder.page(PageSize::A4);
    builder.begin_artifact(page, properties);
    builder.end_artifact(page);
    let mut out = Vec::new();
    let result = builder.build(&mut out);
    (result, out)
}

/// 通过通用标记内容 API 注入原始 Artifact 属性字典并构建。
fn build_raw_artifact(dict: Dict) -> (Result<(), WriteError>, Vec<u8>) {
    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    let page = builder.page(PageSize::A4);
    builder.pages[page]
        .content
        .begin_marked_content_properties("Artifact", MarkedContentProperties::Dict(dict));
    builder.pages[page].content.end_marked_content();
    let mut out = Vec::new();
    let result = builder.build(&mut out);
    (result, out)
}

#[test]
fn artifact_properties_to_dict_encodes_table330_entries() {
    // 仅 `/Type`：可选条目全部省略，属性表仍非空。
    let minimal = ArtifactProperties::new(ArtifactType::Layout);
    let minimal_dict = minimal.to_dict();
    assert_eq!(minimal_dict.0.len(), 1, "{minimal_dict:?}");
    assert_eq!(
        minimal_dict.get("Type"),
        Some(&Value::Name("Layout".into()))
    );
    assert!(minimal_dict.get("BBox").is_none());
    assert!(minimal_dict.get("Attached").is_none());
    assert!(minimal_dict.get("Subtype").is_none());

    let properties = ArtifactProperties::new(ArtifactType::Pagination)
        .with_bounding_box([0.0, 0.0, 612.0, 792.0])
        .with_attachment(ArtifactAttachment::Top)
        .with_attachment(ArtifactAttachment::Bottom)
        .with_subtype(ArtifactSubtype::Header);
    let dict = properties.to_dict();
    assert_eq!(dict.0.len(), 4, "{dict:?}");
    assert_eq!(dict.get("Type"), Some(&Value::Name("Pagination".into())));
    assert_eq!(
        dict.get("BBox"),
        Some(&Value::Array(Array(vec![
            Value::Float(0.0),
            Value::Float(0.0),
            Value::Float(612.0),
            Value::Float(792.0),
        ])))
    );
    assert_eq!(
        dict.get("Attached"),
        Some(&Value::Array(Array(vec![
            Value::Name("Top".into()),
            Value::Name("Bottom".into()),
        ]))),
        "Attached 应按调用顺序写成名字数组"
    );
    assert_eq!(dict.get("Subtype"), Some(&Value::Name("Header".into())));

    // 重复的附着边只保留首次出现；自定义子类型按原样写出。
    let deduped = ArtifactProperties::new(ArtifactType::Pagination)
        .with_attachment(ArtifactAttachment::Left)
        .with_attachment(ArtifactAttachment::Left)
        .with_subtype(ArtifactSubtype::Other("BatesNumber".into()));
    let dict = deduped.to_dict();
    assert_eq!(
        dict.get("Attached"),
        Some(&Value::Array(Array(vec![Value::Name("Left".into())])))
    );
    assert_eq!(
        dict.get("Subtype"),
        Some(&Value::Name("BatesNumber".into()))
    );
}

#[test]
fn artifact_property_lists_require_pdf_1_4() {
    let properties = ArtifactProperties::new(ArtifactType::Page);
    let (result, out) = build_artifact(PdfVersion::V1_3, &properties);
    let error = result.expect_err("PDF 1.3 must reject Artifact property lists");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(_)),
        "错误应为 UnsupportedVersion: {error}"
    );
    assert!(
        error.to_string().contains("artifact property lists"),
        "{error}"
    );
    assert!(error.to_string().contains("1.4"), "{error}");
    assert!(out.is_empty(), "构建失败时不应写出任何字节");

    let (result, out) = build_artifact(PdfVersion::V1_4, &properties);
    result.expect("PDF 1.4 supports Artifact property lists");
    assert!(!out.is_empty(), "允许时应写出文档字节");
}

#[test]
fn artifact_background_and_subtype_require_pdf_1_7() {
    let background = ArtifactProperties::new(ArtifactType::Background)
        .with_bounding_box([0.0, 0.0, 612.0, 792.0]);
    let with_subtype =
        ArtifactProperties::new(ArtifactType::Pagination).with_subtype(ArtifactSubtype::Header);

    for version in [PdfVersion::V1_5, PdfVersion::V1_6] {
        for (properties, feature, required) in [
            (&background, "background artifacts", "1.7"),
            (&with_subtype, "artifact subtype", "1.7"),
        ] {
            let (result, out) = build_artifact(version, properties);
            let error = result.expect_err("PDF 1.5/1.6 must reject PDF 1.7 artifact entries");
            assert!(
                matches!(&error, WriteError::UnsupportedVersion(_)),
                "错误应为 UnsupportedVersion: {error}"
            );
            assert!(error.to_string().contains(feature), "{error}");
            assert!(error.to_string().contains(required), "{error}");
            assert!(out.is_empty(), "构建失败时不应写出任何字节");
        }
    }

    for properties in [&background, &with_subtype] {
        let (result, out) = build_artifact(PdfVersion::V1_7, properties);
        result.expect("PDF 1.7 supports background artifacts and /Subtype");
        assert!(!out.is_empty(), "允许时应写出文档字节");
    }
}

#[test]
fn invalid_artifact_property_lists_are_rejected() {
    let pagination = || Value::Name("Pagination".into());
    let cases = [
        (
            Dict::new().insert("Type", Value::Name("Background".into())),
            "requires /BBox",
        ),
        (
            Dict::new().insert("Type", pagination()).insert(
                "Attached",
                Value::Array(Array(vec![
                    Value::Name("Top".into()),
                    Value::Name("Top".into()),
                ])),
            ),
            "must not repeat",
        ),
        (
            Dict::new().insert("Type", pagination()).insert(
                "Attached",
                Value::Array(Array(vec![Value::Name("Middle".into())])),
            ),
            "Top, Bottom, Left or Right",
        ),
        (
            Dict::new()
                .insert("Type", Value::Name("Layout".into()))
                .insert("Subtype", Value::Name("Header".into())),
            "only allowed when /Type is Pagination",
        ),
        (
            Dict::new().insert("Type", Value::Name("Decoration".into())),
            "must be one of",
        ),
        (
            Dict::new().insert("Type", pagination()).insert(
                "BBox",
                Value::Array(Array(vec![
                    Value::Float(0.0),
                    Value::Float(0.0),
                    Value::Float(612.0),
                ])),
            ),
            "exactly 4 numbers",
        ),
    ];

    for (dict, expected) in cases {
        let (result, out) = build_raw_artifact(dict);
        let error = result.expect_err("invalid Artifact property list must be rejected");
        assert!(
            matches!(&error, WriteError::InvalidObject(_)),
            "错误应为 InvalidObject: {error}"
        );
        assert!(error.to_string().contains(expected), "{error}");
        assert!(out.is_empty(), "构建失败时不应写出任何字节");
    }
}
