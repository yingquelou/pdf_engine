//! 注解 `/Measure` measure 字典写端测试（12.9，Table 261–263）：
//! 直角坐标条目与数字格式数组的写出、子类型归属与取值校验、PDF 1.7 门禁。

use super::support::annot_object;
use super::*;
use crate::core::builder::types::text_string_value;
use crate::core::types::{PdfFeature, Value};

/// 带全部可选条目的直角坐标 measure 字典。
fn full_measure() -> Measure {
    Measure::new(
        "1 in = 1 ft",
        vec![
            NumberFormat::new("in", 72.0)
                .with_precision(100)
                .with_thousands_separator(",")
                .with_decimal_separator(".")
                .with_label_order(NumberLabelOrder::Suffix),
        ],
        vec![
            NumberFormat::new("ft", 12.0)
                .with_fraction(NumberFraction::Fraction)
                .with_precision(16)
                .with_force_denominator(true),
            NumberFormat::new("in", 1.0),
        ],
        vec![NumberFormat::new("sq ft", 144.0)],
    )
    .with_y(vec![NumberFormat::new("cm", 2.54)])
    .with_angle(vec![NumberFormat::new("deg", 1.0)])
    .with_slope(vec![NumberFormat::new("slope", 1.0)])
    .with_origin([10.0, 20.5])
    .with_conversion_yx(1.0)
}

/// 只含必填条目的 measure 字典。
fn minimal_measure() -> Measure {
    let formats = || vec![NumberFormat::new("pt", 1.0)];
    Measure::new("1:1", formats(), formats(), formats())
}

fn line_with(measure: Measure) -> PdfAnnotation {
    PdfAnnotation::new(AnnotationKind::Line, [10.0, 10.0, 110.0, 60.0])
        .with_line_endpoints([10.0, 20.0, 110.0, 60.0])
        .with_measure(measure)
}

fn polygon_with(measure: Measure) -> PdfAnnotation {
    PdfAnnotation::new(AnnotationKind::Polygon, [0.0, 0.0, 100.0, 100.0])
        .with_vertices(vec![0.0, 0.0, 100.0, 0.0, 50.0, 80.0])
        .with_measure(measure)
}

fn build_one(annot: PdfAnnotation, version: PdfVersion) -> Result<Vec<u8>, WriteError> {
    let mut b = PdfBuilder::with_version(version);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(p0, annot);
    let mut out = Vec::new();
    b.build(&mut out)?;
    Ok(out)
}

#[test]
fn line_annotation_writes_measure_dictionary() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(p0, line_with(full_measure()));
    let doc = b.assemble_document().expect("assemble");
    let dict = annot_object(&doc, 3, 0);

    let measure = dict
        .get("Measure")
        .and_then(|value| value.as_dict())
        .expect("/Measure");
    assert_eq!(measure.get("Type"), Some(&Value::Name("Measure".into())));
    assert_eq!(measure.get("Subtype"), Some(&Value::Name("RL".into())));
    assert_eq!(measure.get("R"), Some(&text_string_value("1 in = 1 ft")));

    let x = measure
        .get("X")
        .and_then(|value| value.as_array())
        .expect("/X");
    let first = x.0[0].as_dict().expect("/X element");
    assert_eq!(first.get("U"), Some(&text_string_value("in")));
    assert_eq!(first.get("C"), Some(&Value::Float(72.0)));
    assert_eq!(first.get("D"), Some(&Value::Integer(100)));
    assert_eq!(first.get("RT"), Some(&text_string_value(",")));
    assert_eq!(first.get("RD"), Some(&text_string_value(".")));
    assert_eq!(first.get("O"), Some(&Value::Name("S".into())));
    assert!(first.get("F").is_none(), "缺省 F 不写");

    let distance = measure
        .get("D")
        .and_then(|value| value.as_array())
        .expect("/D");
    assert_eq!(distance.0.len(), 2);
    let feet = distance.0[0].as_dict().expect("/D element");
    assert_eq!(feet.get("F"), Some(&Value::Name("F".into())));
    assert_eq!(feet.get("D"), Some(&Value::Integer(16)));
    assert_eq!(feet.get("FD"), Some(&Value::Bool(true)));

    assert_eq!(
        measure
            .get("Y")
            .and_then(|value| value.as_array())
            .map(|a| a.0.len()),
        Some(1)
    );
    assert_eq!(
        measure
            .get("T")
            .and_then(|value| value.as_array())
            .map(|a| a.0.len()),
        Some(1)
    );
    assert_eq!(
        measure
            .get("S")
            .and_then(|value| value.as_array())
            .map(|a| a.0.len()),
        Some(1)
    );
    let origin = measure
        .get("O")
        .and_then(|value| value.as_array())
        .expect("/O origin");
    assert_eq!(origin.0, vec![Value::Float(10.0), Value::Float(20.5)]);
    assert_eq!(measure.get("CYX"), Some(&Value::Float(1.0)));

    assert!(
        doc.features
            .borrow()
            .contains(PdfFeature::AnnotationMeasure),
        "feature must be recorded"
    );
}

#[test]
fn measure_writes_only_declared_entries() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(p0, polygon_with(minimal_measure()));
    let doc = b.assemble_document().expect("assemble");
    let dict = annot_object(&doc, 3, 0);

    let measure = dict
        .get("Measure")
        .and_then(|value| value.as_dict())
        .expect("/Measure");
    for key in ["Y", "T", "S", "O", "CYX"] {
        assert!(measure.get(key).is_none(), "/{key} must stay unwritten");
    }
    let x = measure
        .get("X")
        .and_then(|value| value.as_array())
        .expect("/X");
    let first = x.0[0].as_dict().expect("/X element");
    for key in ["F", "D", "FD", "RT", "RD", "PS", "SS", "O"] {
        assert!(
            first.get(key).is_none(),
            "number format /{key} must stay unwritten"
        );
    }
}

#[test]
fn measure_entries_are_rejected_on_other_subtypes() {
    let cases = vec![
        PdfAnnotation::new(AnnotationKind::Square, [0.0, 0.0, 10.0, 10.0])
            .with_measure(minimal_measure()),
        PdfAnnotation::new(AnnotationKind::FreeText, [0.0, 0.0, 10.0, 10.0])
            .with_default_appearance("/F1 12 Tf 0 g")
            .with_measure(minimal_measure()),
        PdfAnnotation::new(AnnotationKind::Text, [0.0, 0.0, 10.0, 10.0])
            .with_measure(minimal_measure()),
    ];
    for annot in cases {
        let error = build_one(annot, PdfVersion::V1_7).expect_err("/Measure on a foreign subtype");
        let message = error.to_string();
        assert!(matches!(error, WriteError::InvalidObject(_)), "{message}");
        assert!(message.contains("specific to"), "{message}");
    }
}

#[test]
fn invalid_measure_values_are_rejected() {
    let formats = || vec![NumberFormat::new("pt", 1.0)];
    let cases = vec![
        line_with(Measure::new("", formats(), formats(), formats())),
        line_with(Measure::new("1:1", Vec::new(), formats(), formats())),
        line_with(Measure::new("1:1", formats(), Vec::new(), formats())),
        line_with(Measure::new("1:1", formats(), formats(), Vec::new())),
        line_with(minimal_measure().with_y(Vec::new())),
        line_with(minimal_measure().with_angle(Vec::new())),
        line_with(minimal_measure().with_slope(Vec::new())),
        line_with(Measure::new(
            "1:1",
            vec![NumberFormat::new("", 1.0)],
            formats(),
            formats(),
        )),
        line_with(Measure::new(
            "1:1",
            vec![NumberFormat::new("pt", f64::NAN)],
            formats(),
            formats(),
        )),
        line_with(Measure::new(
            "1:1",
            vec![NumberFormat::new("pt", 1.0).with_precision(0)],
            formats(),
            formats(),
        )),
        // `/F` 缺省为 D 时 `/D` 须为 10 的倍数。
        line_with(Measure::new(
            "1:1",
            vec![NumberFormat::new("pt", 1.0).with_precision(25)],
            formats(),
            formats(),
        )),
        // `/FD` 依赖 `/D`。
        line_with(Measure::new(
            "1:1",
            vec![NumberFormat::new("pt", 1.0).with_force_denominator(true)],
            formats(),
            formats(),
        )),
        line_with(minimal_measure().with_origin([f64::NAN, 0.0])),
        // `/CYX` 仅在 `/Y` 存在时有意义。
        line_with(minimal_measure().with_conversion_yx(1.0)),
    ];
    for annot in cases {
        let error = build_one(annot, PdfVersion::V1_7).expect_err("invalid measure value");
        assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    }

    // `/F` 为分数时 `/D` 是分母，不必是 10 的倍数。
    build_one(
        line_with(Measure::new(
            "1:1",
            vec![
                NumberFormat::new("pt", 1.0)
                    .with_fraction(NumberFraction::Fraction)
                    .with_precision(25),
            ],
            formats(),
            formats(),
        )),
        PdfVersion::V1_7,
    )
    .expect("fraction denominator 25 is valid");
}

#[test]
fn measure_requires_pdf_1_7() {
    let error =
        build_one(line_with(full_measure()), PdfVersion::V1_6).expect_err("1.6 rejects /Measure");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(message)
            if message.contains("measure") && message.contains("1.7")),
        "{error}"
    );
    build_one(line_with(full_measure()), PdfVersion::V1_7).expect("/Measure at 1.7");

    let mut b = PdfBuilder::with_version(PdfVersion::V1_6);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(p0, line_with(full_measure()));
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("1.6 must reject /Measure before writing");
    assert!(
        matches!(error, WriteError::UnsupportedVersion(_)),
        "{error}"
    );
    assert_eq!(out.len(), 0);
}
