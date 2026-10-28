//! Line 注解专有条目写端测试（ISO 32000-1 12.5.6.13，Table 175/176）：
//! 端点 `/L`、端点样式 `/LE`、引线条目 `/LL` `/LLE` `/Cap` `/IT` 与
//! 题注条目 `/LLO` `/CP` `/CO` 的写出、归属与取值校验、逐组版本门禁。

use super::support::annot_object;
use super::*;
use crate::core::types::{PdfFeature, Value};

/// 带全部 Line 专有条目的注解，供写出与门禁测试复用。
fn full_line_annotation() -> PdfAnnotation {
    PdfAnnotation::new(AnnotationKind::Line, [72.0, 700.0, 300.0, 650.0])
        .with_line_endpoints([72.0, 700.5, 300.0, 650.0])
        .with_line_endings(LineEndStyle::ClosedArrow, LineEndStyle::OpenArrow)
        .with_interior_color(vec![1.0, 0.0, 0.0])
        .with_leader_length(12.5)
        .with_leader_extension(3.0)
        .with_caption(true)
        .with_line_intent(LineIntent::LineDimension)
        .with_leader_line_offset(6.0)
        .with_caption_position(CaptionPosition::Top)
        .with_caption_offset([-1.5, 20.0])
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
fn line_annotation_writes_endpoints_and_arrow_endings() {
    let mut b = PdfBuilder::new();
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Line, [0.0, 0.0, 100.0, 50.0])
            .with_line_endpoints([10.0, 20.5, 30.0, 40.0])
            .with_line_endings(LineEndStyle::Square, LineEndStyle::RClosedArrow),
    );
    let doc = b.assemble_document().expect("assemble");
    let dict = annot_object(&doc, 3, 0);
    let endpoints = dict.get("L").and_then(|v| v.as_array()).expect("/L");
    assert_eq!(
        endpoints.0,
        vec![
            Value::Float(10.0),
            Value::Float(20.5),
            Value::Float(30.0),
            Value::Float(40.0)
        ]
    );
    let endings = dict.get("LE").and_then(|v| v.as_array()).expect("/LE");
    assert_eq!(
        endings.0,
        vec![
            Value::Name("Square".into()),
            Value::Name("RClosedArrow".into())
        ]
    );
    assert!(dict.get("LL").is_none(), "no leader entries declared");
    assert!(dict.get("Cap").is_none());
}

#[test]
fn line_annotation_writes_leader_and_caption_entries() {
    let mut b = PdfBuilder::new();
    let p0 = b.page(PageSize::A4);
    b.add_annotation(p0, full_line_annotation());
    let doc = b.assemble_document().expect("assemble");
    let dict = annot_object(&doc, 3, 0);
    assert_eq!(dict.get("LL"), Some(&Value::Float(12.5)));
    assert_eq!(dict.get("LLE"), Some(&Value::Float(3.0)));
    assert_eq!(dict.get("Cap"), Some(&Value::Bool(true)));
    assert_eq!(dict.get("IT"), Some(&Value::Name("LineDimension".into())));
    assert_eq!(dict.get("LLO"), Some(&Value::Float(6.0)));
    assert_eq!(dict.get("CP"), Some(&Value::Name("Top".into())));
    let offset = dict.get("CO").and_then(|v| v.as_array()).expect("/CO");
    assert_eq!(offset.0, vec![Value::Float(-1.5), Value::Float(20.0)]);
    assert_eq!(
        dict.get("IC")
            .and_then(|v| v.as_array())
            .expect("/IC")
            .0
            .len(),
        3,
        "/IC 由通用条目写出"
    );
    let features = doc.features.borrow();
    for feature in [
        PdfFeature::LineAnnotationArrowEndings,
        PdfFeature::LineAnnotationLeader,
        PdfFeature::LineAnnotationCaption,
    ] {
        assert!(features.contains(feature), "{feature} must be recorded");
    }
}

#[test]
fn line_entries_are_rejected_on_other_subtypes() {
    let cases = vec![
        PdfAnnotation::new(AnnotationKind::Square, [0.0, 0.0, 10.0, 10.0])
            .with_line_endpoints([0.0, 0.0, 5.0, 5.0]),
        PdfAnnotation::new(AnnotationKind::Highlight, [0.0, 0.0, 10.0, 10.0])
            .with_line_endings(LineEndStyle::None, LineEndStyle::None),
        PdfAnnotation::new(AnnotationKind::Text, [0.0, 0.0, 10.0, 10.0]).with_caption(true),
        PdfAnnotation::new(AnnotationKind::Ink, [0.0, 0.0, 10.0, 10.0]).with_leader_length(4.0),
    ];
    for annot in cases {
        let error =
            build_one(annot, PdfVersion::V1_7).expect_err("Line entries on a non-Line subtype");
        let message = error.to_string();
        assert!(matches!(error, WriteError::InvalidObject(_)), "{message}");
        assert!(message.contains("specific to Line"), "{message}");
    }
}

#[test]
fn line_annotation_requires_endpoint_coordinates() {
    // `/L` 是 Table 175 的必填条目；仅有 `/Rect` 不足以写出 Line 注解。
    let error = build_one(
        PdfAnnotation::new(AnnotationKind::Line, [0.0, 0.0, 10.0, 10.0]),
        PdfVersion::V1_7,
    )
    .expect_err("Line without /L");
    let message = error.to_string();
    assert!(matches!(error, WriteError::InvalidObject(_)), "{message}");
    assert!(message.contains("requires /L"), "{message}");
}

#[test]
fn invalid_line_entry_values_are_rejected() {
    let base = || {
        PdfAnnotation::new(AnnotationKind::Line, [0.0, 0.0, 10.0, 10.0])
            .with_line_endpoints([0.0, 0.0, 10.0, 10.0])
    };
    let cases = vec![
        // /L 含非有限数
        PdfAnnotation::new(AnnotationKind::Line, [0.0, 0.0, 10.0, 10.0]).with_line_endpoints([
            0.0,
            f64::NAN,
            10.0,
            10.0,
        ]),
        // /LL 负值
        base().with_leader_length(-1.0),
        // /LLE 未提供 /LL
        base().with_leader_extension(2.0),
        // /LLO 负值
        base().with_caption(true).with_leader_line_offset(-0.5),
        // 题注条目要求 /Cap 为真
        base().with_caption_position(CaptionPosition::Top),
        base().with_caption_offset([1.0, 2.0]),
        base().with_caption(false).with_leader_line_offset(3.0),
        // /CO 含非有限数
        base()
            .with_caption(true)
            .with_caption_offset([1.0, f64::INFINITY]),
    ];
    for annot in cases {
        let error = build_one(annot, PdfVersion::V1_7).expect_err("invalid Line entry value");
        assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
    }
}

#[test]
fn line_arrow_endings_require_pdf_1_4() {
    let error = build_one(
        PdfAnnotation::new(AnnotationKind::Line, [0.0, 0.0, 10.0, 10.0])
            .with_line_endpoints([0.0, 0.0, 10.0, 10.0])
            .with_line_endings(LineEndStyle::OpenArrow, LineEndStyle::None),
        PdfVersion::V1_3,
    )
    .expect_err("1.3 must reject /LE");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(message)
            if message.contains("arrow endings") && message.contains("1.4")),
        "{error}"
    );
}

#[test]
fn new_line_end_styles_require_their_own_versions() {
    // `Butt`/`ROpenArrow`/`RClosedArrow` 自 1.5，`Slash` 自 1.6（Table 176）。
    for (style, version) in [
        (LineEndStyle::Butt, PdfVersion::V1_4),
        (LineEndStyle::RClosedArrow, PdfVersion::V1_4),
        (LineEndStyle::Slash, PdfVersion::V1_5),
    ] {
        let error = build_one(
            PdfAnnotation::new(AnnotationKind::Line, [0.0, 0.0, 10.0, 10.0])
                .with_line_endpoints([0.0, 0.0, 10.0, 10.0])
                .with_line_endings(style, LineEndStyle::None),
            version,
        )
        .expect_err("too new end style for version");
        assert!(
            matches!(&error, WriteError::UnsupportedVersion(message)
                if message.contains("/LE")),
            "{error}"
        );
    }
    // 同一样式在其门起版本可写出。
    for (style, version) in [
        (LineEndStyle::Butt, PdfVersion::V1_5),
        (LineEndStyle::Slash, PdfVersion::V1_6),
    ] {
        build_one(
            PdfAnnotation::new(AnnotationKind::Line, [0.0, 0.0, 10.0, 10.0])
                .with_line_endpoints([0.0, 0.0, 10.0, 10.0])
                .with_line_endings(LineEndStyle::None, style),
            version,
        )
        .expect("end style at its own minimum version");
    }
}

#[test]
fn line_leader_entries_require_pdf_1_6() {
    let error = build_one(
        PdfAnnotation::new(AnnotationKind::Line, [0.0, 0.0, 10.0, 10.0])
            .with_line_endpoints([0.0, 0.0, 10.0, 10.0])
            .with_leader_length(8.0),
        PdfVersion::V1_5,
    )
    .expect_err("1.5 must reject /LL");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(message)
            if message.contains("leader settings") && message.contains("1.6")),
        "{error}"
    );
    // `/IT` 同组；`/Cap` 亦同组。
    for annot in [
        PdfAnnotation::new(AnnotationKind::Line, [0.0, 0.0, 10.0, 10.0])
            .with_line_endpoints([0.0, 0.0, 10.0, 10.0])
            .with_line_intent(LineIntent::LineDimension),
        PdfAnnotation::new(AnnotationKind::Line, [0.0, 0.0, 10.0, 10.0])
            .with_line_endpoints([0.0, 0.0, 10.0, 10.0])
            .with_caption(true),
    ] {
        let error = build_one(annot, PdfVersion::V1_5).expect_err("1.5 must reject 1.6 entries");
        assert!(
            matches!(&error, WriteError::UnsupportedVersion(_)),
            "{error}"
        );
    }
}

#[test]
fn line_caption_entries_require_pdf_1_7() {
    for annot in [
        PdfAnnotation::new(AnnotationKind::Line, [0.0, 0.0, 10.0, 10.0])
            .with_line_endpoints([0.0, 0.0, 10.0, 10.0])
            .with_caption(true)
            .with_caption_position(CaptionPosition::Top),
        PdfAnnotation::new(AnnotationKind::Line, [0.0, 0.0, 10.0, 10.0])
            .with_line_endpoints([0.0, 0.0, 10.0, 10.0])
            .with_caption(true)
            .with_leader_line_offset(4.0),
    ] {
        let error = build_one(annot, PdfVersion::V1_6).expect_err("1.6 must reject /CP /LLO");
        assert!(
            matches!(&error, WriteError::UnsupportedVersion(message)
                if message.contains("caption settings") && message.contains("1.7")),
            "{error}"
        );
    }
    build_one(full_line_annotation(), PdfVersion::V1_7).expect("1.7 writes every Line entry");
    // 1.4 只允许 `/LE`：PDF 1.4 文档写出仅含端点样式的 Line 注解。
    build_one(
        PdfAnnotation::new(AnnotationKind::Line, [0.0, 0.0, 10.0, 10.0])
            .with_line_endpoints([0.0, 0.0, 10.0, 10.0])
            .with_line_endings(LineEndStyle::Diamond, LineEndStyle::Diamond),
        PdfVersion::V1_4,
    )
    .expect("1.4 writes /LE");
}

#[test]
fn line_annotations_record_markup_feature() {
    let mut b = PdfBuilder::new();
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Line, [0.0, 0.0, 10.0, 10.0])
            .with_line_endpoints([0.0, 0.0, 10.0, 10.0]),
    );
    let doc = b.assemble_document().expect("assemble");
    assert!(
        doc.features
            .borrow()
            .contains(PdfFeature::MarkupAnnotations),
        "Line is a markup annotation kind (Table 170)"
    );
}
