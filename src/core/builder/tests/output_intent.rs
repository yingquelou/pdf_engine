//! 输出意图写端测试（14.11.5 Table 365）：目录 `/OutputIntents` 数组、
//! 文本串条目、ICC 配置文件流与 PDF 1.4 门禁。

use super::*;
use crate::core::builder::{OutputIntent, OutputIntentProfile};
use crate::core::reader::output_intent::OutputIntentSubtype;
use crate::core::types::{PdfFeature, PdfVersion};
use crate::core::writer::WriteError;

fn text_string(text: &str) -> Value {
    crate::core::builder::types::text_string_value(text)
}

fn catalog(doc: &Document) -> Dict {
    doc.get_object(1)
        .expect("catalog lookup")
        .expect("catalog exists")
        .value
        .as_dict()
        .expect("catalog dict")
        .clone()
}

fn build_error(intent: OutputIntent) -> WriteError {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_4);
    b.page(PageSize::A4);
    b.add_output_intent(intent);
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("invalid output intent must be rejected");
    assert!(out.is_empty(), "校验失败必须零字节输出");
    error
}

#[test]
fn output_intents_write_table365_entries() {
    let bytes = vec![0x00, 0x11, 0x22, 0x33];
    let mut b = PdfBuilder::with_version(PdfVersion::V1_4);
    b.page(PageSize::A4);
    b.add_output_intent(
        OutputIntent::new(OutputIntentSubtype::GtsPdfX, "FOGRA39")
            .with_output_condition("Coated FOGRA39")
            .with_registry_name("http://www.color.org")
            .with_info("ICC profile")
            .with_destination_profile(OutputIntentProfile::new(4, bytes.clone())),
    );
    let doc = b.assemble_document().expect("assemble");

    let catalog = catalog(&doc);
    let Value::Array(intents) = catalog.get("OutputIntents").expect("/OutputIntents") else {
        panic!("/OutputIntents must be an array")
    };
    assert_eq!(intents.len(), 1);
    let intent = intents.0[0].as_dict().expect("intent dict");
    assert_eq!(
        intent.get("Type"),
        Some(&Value::Name("OutputIntent".into()))
    );
    assert_eq!(intent.get("S"), Some(&Value::Name("GTS_PDFX".into())));
    assert_eq!(
        intent.get("OutputCondition"),
        Some(&text_string("Coated FOGRA39"))
    );
    assert_eq!(
        intent.get("OutputConditionIdentifier"),
        Some(&text_string("FOGRA39"))
    );
    assert_eq!(
        intent.get("RegistryName"),
        Some(&text_string("http://www.color.org"))
    );
    assert_eq!(intent.get("Info"), Some(&text_string("ICC profile")));

    let Some(Value::Ref(num, generation)) = intent.get("DestOutputProfile") else {
        panic!("profile must be an indirect stream reference")
    };
    assert_eq!(*generation, 0);
    let object = doc
        .get_object(*num)
        .expect("profile lookup")
        .expect("exists");
    let Value::Stream(dict, StreamData::Owned(data)) = &object.value else {
        panic!("profile must be an owned stream")
    };
    assert_eq!(dict.get("N"), Some(&Value::Integer(4)));
    assert_eq!(
        dict.get("Length"),
        Some(&Value::Integer(bytes.len() as i64))
    );
    assert_eq!(data, &bytes);
    assert!(doc.features.borrow().contains(PdfFeature::OutputIntents));
}

#[test]
fn output_intents_require_pdf_1_4() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_3);
    b.page(PageSize::A4);
    b.add_output_intent(OutputIntent::new(OutputIntentSubtype::GtsPdfA1, "PDF/A-1B"));
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("PDF 1.3 must reject output intents");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(_)),
        "unexpected error {error:?}"
    );
    assert!(error.to_string().contains("output intents"), "{error}");
    assert!(error.to_string().contains("1.4"), "{error}");
    assert!(out.is_empty(), "版本门禁失败必须零字节");
}

#[test]
fn invalid_output_intents_are_rejected_without_output() {
    // Table 365：Custom 标识缺 /Info。
    let error = build_error(
        OutputIntent::new(OutputIntentSubtype::GtsPdfX, "Custom")
            .with_destination_profile(OutputIntentProfile::new(3, vec![0; 3])),
    );
    assert!(
        matches!(&error, WriteError::InvalidObject(_)),
        "unexpected error {error:?}"
    );
    assert!(error.to_string().contains("/Info"), "{error}");
    assert!(error.to_string().contains("Table 365"), "{error}");

    // Table 365：Custom 标识缺 /DestOutputProfile。
    let error = build_error(
        OutputIntent::new(OutputIntentSubtype::GtsPdfX, "Custom").with_info("custom condition"),
    );
    assert!(
        matches!(&error, WriteError::InvalidObject(_)),
        "unexpected error {error:?}"
    );
    assert!(error.to_string().contains("/DestOutputProfile"), "{error}");
    assert!(error.to_string().contains("Table 365"), "{error}");

    // 空标识。
    let error = build_error(OutputIntent::new(OutputIntentSubtype::IsoPdfE1, ""));
    assert!(error.to_string().contains("must not be empty"), "{error}");

    // /N 分量数越界。
    for components in [0_u8, 5] {
        let error = build_error(
            OutputIntent::new(OutputIntentSubtype::GtsPdfA1, "PDF/A-1B")
                .with_destination_profile(OutputIntentProfile::new(components, vec![0; 4])),
        );
        assert!(
            error.to_string().contains("1 to 4 components"),
            "components {components}: {error}"
        );
    }
}
