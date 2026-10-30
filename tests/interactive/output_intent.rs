//! 输出意图（ISO 32000-1 14.11.5、Table 365）写出 → 解析 → 回读往返。

use super::*;

/// 构建器写出两个输出意图：一个 `Custom` 带 ICC 配置文件，一个标准
/// PDF/A-1 标识；解析后子类型、文本条目、配置文件字节与间接引用回读一致。
#[test]
fn builder_output_intents_roundtrip_through_parser() {
    use pdf_engine::core::builder::{OutputIntent, OutputIntentProfile, PageSize, PdfBuilder};
    use pdf_engine::core::reader::output_intent::OutputIntentSubtype;
    use pdf_engine::core::types::{PdfFeature, PdfVersion};

    let profile_data = vec![0x00, 0x01, 0x02, 0x03, 0x04, 0x05];
    let mut builder = PdfBuilder::with_version(PdfVersion::V1_4);
    builder.page(PageSize::A4);
    builder.add_output_intent(
        OutputIntent::new(OutputIntentSubtype::GtsPdfX, "Custom")
            .with_output_condition("Vendor press condition")
            .with_registry_name("http://example.com/registry")
            .with_info("Custom ICC profile")
            .with_destination_profile(OutputIntentProfile::new(4, profile_data.clone())),
    );
    builder.add_output_intent(
        OutputIntent::new(OutputIntentSubtype::GtsPdfA1, "PDF/A-1B")
            .with_output_condition("sRGB")
            .with_registry_name("http://www.color.org"),
    );
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build output intents");
    let doc = parse_buf(&buf);

    let intents = doc.output_intents().expect("output intents");
    assert_eq!(intents.len(), 2);

    let custom = &intents[0];
    assert_eq!(custom.subtype, OutputIntentSubtype::GtsPdfX);
    assert_eq!(custom.output_condition_identifier, "Custom");
    assert_eq!(
        custom.output_condition.as_deref(),
        Some("Vendor press condition")
    );
    assert_eq!(
        custom.registry_name.as_deref(),
        Some("http://example.com/registry")
    );
    assert_eq!(custom.info.as_deref(), Some("Custom ICC profile"));
    assert_eq!(
        custom.dict.get("Type"),
        Some(&Value::Name("OutputIntent".into()))
    );
    let profile = custom
        .destination_profile
        .as_ref()
        .expect("custom intent profile");
    assert_eq!(profile.components, 4);
    assert_eq!(profile.data, profile_data);
    assert!(profile.reference.is_some(), "配置文件流应为间接引用");

    let standard = &intents[1];
    assert_eq!(standard.subtype, OutputIntentSubtype::GtsPdfA1);
    assert_eq!(standard.output_condition_identifier, "PDF/A-1B");
    assert_eq!(standard.output_condition.as_deref(), Some("sRGB"));
    assert_eq!(standard.info, None);
    assert_eq!(standard.destination_profile, None);

    assert!(
        doc.features.borrow().contains(PdfFeature::OutputIntents),
        "解析后特性画像应含 output intents"
    );
}
