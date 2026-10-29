//! FreeText 注解专有条目写端测试（ISO 32000-1 12.5.6.6，Table 174）：
//! `/DA` `/Q` `/RC` `/DS` `/CL` `/IT` `/BE` `/RD` `/LE` 的写出、归属与
//! 取值校验、逐组版本门禁。

use super::support::{annot_object, assert_text_entry};
use super::*;
use crate::core::builder::types::text_string_value;
use crate::core::types::{PdfFeature, Value};

/// 带全部 FreeText 专有条目的注解，供写出与门禁测试复用。
fn full_free_text_annotation() -> PdfAnnotation {
    PdfAnnotation::new(AnnotationKind::FreeText, [72.0, 700.0, 300.0, 640.0])
        .with_default_appearance("/F1 12 Tf 0 g")
        .with_rich_content("<body>hi</body>")
        .with_quadding(1)
        .with_default_style("font: ArialMT 12pt;")
        .with_callout_line(CalloutLine::Knee([20.0, 100.0, 60.5, 140.0, 120.0, 180.0]))
        .with_free_text_intent(FreeTextIntent::FreeTextCallout)
        .with_border_effect(BorderEffect::cloudy(1.5))
        .with_text_rectangle([0.0, 2.5, 0.0, 2.5])
        .with_callout_ending(LineEndStyle::ClosedArrow)
}

/// 把注解挂到单页文档并写出。
fn build_one(annot: PdfAnnotation, version: PdfVersion) -> Result<Vec<u8>, WriteError> {
    let mut b = PdfBuilder::with_version(version);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(p0, annot);
    let mut out = Vec::new();
    b.build(&mut out)?;
    Ok(out)
}

#[test]
fn free_text_annotation_writes_table_174_entries() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_6);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(p0, full_free_text_annotation());
    let doc = b.assemble_document().expect("assemble");
    let dict = annot_object(&doc, 3, 0);

    assert_eq!(dict.get("Subtype"), Some(&Value::Name("FreeText".into())));
    assert_eq!(
        dict.get("DA"),
        Some(&text_string_value("/F1 12 Tf 0 g")),
        "/DA 按文本串写出"
    );
    assert_eq!(dict.get("Q"), Some(&Value::Integer(1)));
    assert_eq!(dict.get("RC"), Some(&text_string_value("<body>hi</body>")));
    assert_eq!(
        dict.get("DS"),
        Some(&text_string_value("font: ArialMT 12pt;"))
    );
    let callout = dict.get("CL").and_then(|v| v.as_array()).expect("/CL");
    assert_eq!(
        callout.0,
        vec![
            Value::Float(20.0),
            Value::Float(100.0),
            Value::Float(60.5),
            Value::Float(140.0),
            Value::Float(120.0),
            Value::Float(180.0)
        ]
    );
    assert_eq!(dict.get("IT"), Some(&Value::Name("FreeTextCallout".into())));
    let effect = dict.get("BE").and_then(|v| v.as_dict()).expect("/BE");
    assert_eq!(effect.get("S"), Some(&Value::Name("C".into())));
    assert_eq!(effect.get("I"), Some(&Value::Float(1.5)));
    let diff = dict.get("RD").and_then(|v| v.as_array()).expect("/RD");
    assert_eq!(
        diff.0,
        vec![
            Value::Float(0.0),
            Value::Float(2.5),
            Value::Float(0.0),
            Value::Float(2.5)
        ]
    );
    assert_eq!(
        dict.get("LE"),
        Some(&Value::Name("ClosedArrow".into())),
        "FreeText /LE 是单个名字，不是数组"
    );
}

#[test]
fn free_text_minimal_annotation_writes_only_default_appearance() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_3);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::FreeText, [0.0, 0.0, 100.0, 50.0])
            .with_default_appearance("/Helv 10 Tf"),
    );
    let doc = b.assemble_document().expect("assemble at 1.3");
    let dict = annot_object(&doc, 3, 0);
    assert_text_entry(&dict, "DA");
    for key in ["Q", "RC", "DS", "CL", "IT", "BE", "RD", "LE"] {
        assert!(dict.get(key).is_none(), "/{key} 未声明时不得写出");
    }
    let features = doc.features.borrow();
    for feature in [
        PdfFeature::FreeTextQuadding,
        PdfFeature::FreeTextRichContent,
        PdfFeature::FreeTextCallout,
    ] {
        assert!(!features.contains(feature), "{feature} 不应被记录");
    }
}

#[test]
fn free_text_entries_are_rejected_on_other_subtypes() {
    let cases = vec![
        PdfAnnotation::new(AnnotationKind::Square, [0.0, 0.0, 10.0, 10.0])
            .with_default_appearance("/F1 12 Tf"),
        PdfAnnotation::new(AnnotationKind::Text, [0.0, 0.0, 10.0, 10.0]).with_quadding(1),
        PdfAnnotation::new(AnnotationKind::Line, [0.0, 0.0, 10.0, 10.0])
            .with_line_endpoints([0.0, 0.0, 10.0, 10.0])
            .with_callout_line(CalloutLine::Straight([0.0, 0.0, 5.0, 5.0])),
        PdfAnnotation::new(AnnotationKind::Ink, [0.0, 0.0, 10.0, 10.0])
            .with_border_effect(BorderEffect::new(BorderEffectStyle::Cloudy)),
    ];
    for annot in cases {
        let error =
            build_one(annot, PdfVersion::V1_7).expect_err("FreeText entries on a foreign subtype");
        let message = error.to_string();
        assert!(matches!(error, WriteError::InvalidObject(_)), "{message}");
        assert!(message.contains("specific to FreeText"), "{message}");
    }
}

#[test]
fn free_text_annotation_requires_default_appearance() {
    // `/DA` 是 Table 174 的必填条目。
    let error = build_one(
        PdfAnnotation::new(AnnotationKind::FreeText, [0.0, 0.0, 10.0, 10.0]),
        PdfVersion::V1_7,
    )
    .expect_err("FreeText without /DA");
    let message = error.to_string();
    assert!(matches!(error, WriteError::InvalidObject(_)), "{message}");
    assert!(message.contains("requires /DA"), "{message}");

    // 空串同样不算提供 `/DA`。
    let error = build_one(
        PdfAnnotation::new(AnnotationKind::FreeText, [0.0, 0.0, 10.0, 10.0])
            .with_default_appearance("   "),
        PdfVersion::V1_7,
    )
    .expect_err("blank /DA");
    assert!(
        matches!(&error, WriteError::InvalidObject(message) if message.contains("/DA")),
        "{error}"
    );
}

#[test]
fn invalid_free_text_entry_values_are_rejected() {
    let base = || {
        PdfAnnotation::new(AnnotationKind::FreeText, [0.0, 0.0, 10.0, 10.0])
            .with_default_appearance("/F1 12 Tf")
    };
    let cases = vec![
        // /Q 只能是 0、1、2
        base().with_quadding(3),
        // /RC /DS 不得为空串
        base().with_rich_content(""),
        base().with_default_style(""),
        // /CL 须为有限数（意图与引线配对由专用测试覆盖）
        base()
            .with_callout_line(CalloutLine::Straight([0.0, f64::NAN, 1.0, 2.0]))
            .with_free_text_intent(FreeTextIntent::FreeTextCallout),
        // /RD 均 ≥0 且有限
        base().with_text_rectangle([0.0, -1.0, 0.0, 0.0]),
        base().with_text_rectangle([0.0, f64::INFINITY, 0.0, 0.0]),
        // /BE /I 落在 0–2，且只在云状边框可带
        base().with_border_effect(BorderEffect::cloudy(3.0)),
        base().with_border_effect(BorderEffect {
            style: BorderEffectStyle::NoEffect,
            intensity: Some(1.0),
        }),
        // /LE 需要 /CL
        base().with_callout_ending(LineEndStyle::OpenArrow),
    ];
    for annot in cases {
        let error = build_one(annot, PdfVersion::V1_7).expect_err("invalid FreeText entry value");
        assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    }
}

#[test]
fn free_text_callout_line_and_intent_are_coupled() {
    let base = || {
        PdfAnnotation::new(AnnotationKind::FreeText, [0.0, 0.0, 10.0, 10.0])
            .with_default_appearance("/F1 12 Tf")
    };
    let line = CalloutLine::Straight([0.0, 0.0, 5.0, 5.0]);
    // `/CL` 仅在 `/IT /FreeTextCallout` 下有意义（Table 174）。
    for annot in [
        base().with_callout_line(line),
        base()
            .with_callout_line(line)
            .with_free_text_intent(FreeTextIntent::FreeText),
        base()
            .with_callout_line(line)
            .with_free_text_intent(FreeTextIntent::FreeTextTypeWriter),
    ] {
        let error = build_one(annot, PdfVersion::V1_7).expect_err("/CL without callout intent");
        let message = error.to_string();
        assert!(matches!(error, WriteError::InvalidObject(_)), "{message}");
        assert!(message.contains("/IT /FreeTextCallout"), "{message}");
    }
    // callout 意图必须通过 `/CL` 关联到页面区域。
    let error = build_one(
        base().with_free_text_intent(FreeTextIntent::FreeTextCallout),
        PdfVersion::V1_7,
    )
    .expect_err("callout intent without /CL");
    let message = error.to_string();
    assert!(matches!(error, WriteError::InvalidObject(_)), "{message}");
    assert!(message.contains("requires /CL"), "{message}");
    build_one(
        base()
            .with_callout_line(line)
            .with_free_text_intent(FreeTextIntent::FreeTextCallout),
        PdfVersion::V1_6,
    )
    .expect("/CL with callout intent at 1.6");
}

#[test]
fn free_text_quadding_requires_pdf_1_4() {
    let annot = || {
        PdfAnnotation::new(AnnotationKind::FreeText, [0.0, 0.0, 10.0, 10.0])
            .with_default_appearance("/F1 12 Tf")
    };
    let error = build_one(annot().with_quadding(2), PdfVersion::V1_3).expect_err("1.3 rejects /Q");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(message)
            if message.contains("quadding") && message.contains("1.4")),
        "{error}"
    );
    build_one(annot().with_quadding(2), PdfVersion::V1_4).expect("/Q at 1.4");
}

#[test]
fn free_text_rich_content_requires_pdf_1_5() {
    let annot = || {
        PdfAnnotation::new(AnnotationKind::FreeText, [0.0, 0.0, 10.0, 10.0])
            .with_default_appearance("/F1 12 Tf")
    };
    for annot in [
        annot().with_rich_content("<body/>"),
        annot().with_default_style("font: ArialMT 12pt;"),
    ] {
        let error = build_one(annot, PdfVersion::V1_4).expect_err("1.4 rejects /RC /DS");
        assert!(
            matches!(&error, WriteError::UnsupportedVersion(message)
                if message.contains("rich content") && message.contains("1.5")),
            "{error}"
        );
    }
    build_one(annot().with_rich_content("<body/>"), PdfVersion::V1_5).expect("/RC at 1.5");
}

#[test]
fn free_text_callout_entries_require_pdf_1_6() {
    let annot = || {
        PdfAnnotation::new(AnnotationKind::FreeText, [0.0, 0.0, 10.0, 10.0])
            .with_default_appearance("/F1 12 Tf")
    };
    let cases = vec![
        annot()
            .with_callout_line(CalloutLine::Straight([0.0, 0.0, 5.0, 5.0]))
            .with_free_text_intent(FreeTextIntent::FreeTextCallout),
        annot().with_free_text_intent(FreeTextIntent::FreeTextTypeWriter),
        annot().with_border_effect(BorderEffect::new(BorderEffectStyle::NoEffect)),
        annot().with_text_rectangle([0.0, 0.0, 0.0, 0.0]),
        annot()
            .with_callout_line(CalloutLine::Straight([0.0, 0.0, 5.0, 5.0]))
            .with_free_text_intent(FreeTextIntent::FreeTextCallout)
            .with_callout_ending(LineEndStyle::OpenArrow),
    ];
    for annot in cases {
        let error = build_one(annot, PdfVersion::V1_5).expect_err("1.5 rejects callout entries");
        assert!(
            matches!(&error, WriteError::UnsupportedVersion(message)
                if message.contains("callout") && message.contains("1.6")),
            "{error}"
        );
    }
    build_one(full_free_text_annotation(), PdfVersion::V1_6).expect("1.6 writes every entry");
    // PDF 1.5 也允许富文本：逐组门禁不把 `/RC` 与 `/CL` 混为一谈。
    build_one(annot().with_rich_content("<body/>"), PdfVersion::V1_5).expect("1.5 writes /RC");
}

#[test]
fn free_text_entries_record_their_features() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_6);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(p0, full_free_text_annotation());
    let doc = b.assemble_document().expect("assemble");
    let features = doc.features.borrow();
    for feature in [
        PdfFeature::FreeTextQuadding,
        PdfFeature::FreeTextRichContent,
        PdfFeature::FreeTextCallout,
        PdfFeature::MarkupAnnotations,
    ] {
        assert!(features.contains(feature), "{feature} must be recorded");
    }
}

#[test]
fn rejected_free_text_version_writes_no_bytes() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_5);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(p0, full_free_text_annotation());
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("1.5 must reject the callout entries");
    assert!(
        matches!(error, WriteError::UnsupportedVersion(_)),
        "{error}"
    );
    assert_eq!(out.len(), 0);
}
