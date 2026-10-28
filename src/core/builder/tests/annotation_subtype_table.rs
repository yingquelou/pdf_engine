//! 注解子类型表写端测试（ISO 32000-1 12.5.3 Table 169）：
//! 写端子集与读端权威表的名称/引入版本/标记类归属一致性，以及按 Table 169
//! `(PDF x.y)` 标注的子类型版本门禁。

use super::*;
use crate::core::reader::annotation::{AnnotationSubtype, TABLE_169};
use crate::core::types::PdfFeature;

/// 写端支持的 Table 169 子集（17 种）。
const WRITER_KINDS: [AnnotationKind; 17] = [
    AnnotationKind::Text,
    AnnotationKind::Link,
    AnnotationKind::FreeText,
    AnnotationKind::Line,
    AnnotationKind::Square,
    AnnotationKind::Circle,
    AnnotationKind::Polygon,
    AnnotationKind::PolyLine,
    AnnotationKind::Highlight,
    AnnotationKind::Underline,
    AnnotationKind::Squiggly,
    AnnotationKind::StrikeOut,
    AnnotationKind::Caret,
    AnnotationKind::Ink,
    AnnotationKind::Popup,
    AnnotationKind::FileAttachment,
    AnnotationKind::Stamp,
];

/// 把单个注解挂到单页文档并按 `version` 写出。
fn build_at(kind: AnnotationKind, version: PdfVersion) -> Result<Vec<u8>, WriteError> {
    let mut annot = PdfAnnotation::new(kind, [0.0, 0.0, 50.0, 50.0]);
    if kind == AnnotationKind::Line {
        annot = annot.with_line_endpoints([0.0, 0.0, 50.0, 50.0]);
    }
    let mut b = PdfBuilder::with_version(version);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(p0, annot);
    let mut out = Vec::new();
    b.build(&mut out)?;
    Ok(out)
}

#[test]
fn writer_annotation_kinds_match_the_reader_table() {
    for kind in WRITER_KINDS {
        let subtype = kind.subtype();
        assert!(
            !matches!(subtype, AnnotationSubtype::Unknown(_)),
            "{} 应是 Table 169 标准子类型",
            kind.name()
        );
        assert_eq!(kind.name(), subtype.name());
        let row = TABLE_169
            .iter()
            .find(|(name, _, _)| *name == kind.name())
            .unwrap_or_else(|| panic!("Table 169 无 {} 行", kind.name()));
        assert_eq!(kind.min_version(), row.1, "{} 引入版本", kind.name());
        assert_eq!(
            subtype.min_version(),
            Some(row.1),
            "{} 引入版本",
            kind.name()
        );
        assert_eq!(kind.is_markup(), row.2, "{} 标记类归属", kind.name());
        assert_eq!(subtype.is_markup(), row.2, "{} 标记类归属", kind.name());
    }
}

#[test]
fn corrected_subtype_versions_are_enforced_by_the_writer() {
    // Table 169 校准后的关键更正：`Ink` 是 PDF 1.3（此前误记 1.4），
    // `FreeText`/`Line`/`Square`/`Circle` 是 1.3，`Polygon`/`PolyLine`/`Caret` 是 1.5。
    assert_eq!(AnnotationKind::Ink.min_version(), PdfVersion::V1_3);
    for kind in [
        AnnotationKind::FreeText,
        AnnotationKind::Line,
        AnnotationKind::Square,
        AnnotationKind::Circle,
    ] {
        assert_eq!(kind.min_version(), PdfVersion::V1_3, "{}", kind.name());
        build_at(kind, PdfVersion::V1_3).unwrap_or_else(|error| panic!("{}: {error}", kind.name()));
        let error = build_at(kind, PdfVersion::V1_2).expect_err("1.2 rejects it");
        assert!(
            matches!(error, WriteError::UnsupportedVersion(_)),
            "{}: {error}",
            kind.name()
        );
    }
    for kind in [
        AnnotationKind::Polygon,
        AnnotationKind::PolyLine,
        AnnotationKind::Caret,
    ] {
        assert_eq!(kind.min_version(), PdfVersion::V1_5, "{}", kind.name());
        build_at(kind, PdfVersion::V1_5).unwrap_or_else(|error| panic!("{}: {error}", kind.name()));
        let error = build_at(kind, PdfVersion::V1_4).expect_err("1.4 rejects it");
        assert!(
            matches!(error, WriteError::UnsupportedVersion(_)),
            "{}: {error}",
            kind.name()
        );
    }
}

#[test]
fn markup_annotation_flags_follow_table_169() {
    // 标记类含 `FileAttachment`，不含 `Popup` 与 `Link`。
    assert!(AnnotationKind::FileAttachment.is_markup());
    assert!(AnnotationKind::Stamp.is_markup());
    assert!(AnnotationKind::Ink.is_markup());
    assert!(!AnnotationKind::Popup.is_markup());
    assert!(!AnnotationKind::Link.is_markup());
}

#[test]
fn rejected_subtype_version_writes_no_bytes() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_2);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Square, [0.0, 0.0, 50.0, 50.0]),
    );
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("1.2 must reject Square annotations");
    assert!(
        matches!(error, WriteError::UnsupportedVersion(_)),
        "{error}"
    );
    assert_eq!(out.len(), 0);
}

#[test]
fn ink_annotation_records_the_markup_feature_from_1_3() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_3);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Ink, [0.0, 0.0, 50.0, 50.0])
            .with_ink_path(vec![1.0, 2.0, 3.0, 4.0]),
    );
    let doc = b.assemble_document().expect("assemble at 1.3");
    assert!(
        doc.features
            .borrow()
            .contains(PdfFeature::MarkupAnnotations),
        "Ink 自 PDF 1.3 即标记类注解"
    );
}
