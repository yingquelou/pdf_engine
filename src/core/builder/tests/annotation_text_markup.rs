//! E2 文本标记与图章写端测试（ISO 32000-1 12.5.6.10/12.5.6.12）：
//! Table 179 `/QuadPoints` 分组写出与取值契约、Table 181 Stamp `/Name`
//! 标准名类型化与未知名原样保留、`/Name` 子类型归属与名字形态校验、
//! 以及随子类型生效的版本门禁（Table 169；`/Name` 与 `/QuadPoints` 条目
//! 本身在 Table 181/179 无 PDF 版本标注，不单设门禁）。
//! 全部失败须在分配对象号前拒绝且零字节写出。

use super::support::annot_object;
use super::*;
use crate::core::reader::annotation::{AnnotationSubtype, StampIcon as ReaderStampIcon};
use crate::core::types::Document;
use std::sync::atomic::{AtomicUsize, Ordering};

/// 单组四边形坐标（逆时针四点，Table 179）。
const ONE_QUAD: [f64; 8] = [72.0, 712.0, 200.0, 712.0, 72.0, 700.0, 200.0, 700.0];

/// 把单条注解挂到一页并写出。
fn build_one(annot: PdfAnnotation, version: PdfVersion) -> Result<Vec<u8>, WriteError> {
    let mut b = PdfBuilder::with_version(version);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(p0, annot);
    let mut out = Vec::new();
    b.build(&mut out)?;
    Ok(out)
}

/// 写出并重新解析（经 target 目录临时文件，避开沙箱对 %TEMP% 的拦截）。
fn build_and_parse(annot: PdfAnnotation, version: PdfVersion) -> (Vec<u8>, Document) {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let out = build_one(annot, version).expect("build");
    let base = std::env::current_exe()
        .ok()
        .and_then(|p| p.ancestors().nth(3).map(|t| t.to_path_buf()))
        .filter(|p| p.file_name() == Some(std::ffi::OsStr::new("target")))
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join("test-tmp");
    std::fs::create_dir_all(&dir).expect("create test-tmp");
    let path = dir.join(format!(
        "pdf-engine-text-markup-{}-{}.pdf",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&path, &out).expect("write temp pdf");
    let file = std::fs::File::open(&path).expect("open temp pdf");
    let mut parser = crate::core::reader::Parser::new(file).expect("parser");
    let doc = parser.parse_document().expect("parse");
    let _ = std::fs::remove_file(&path);
    (out, doc)
}

#[test]
fn stamp_writes_standard_icon_name_as_pdf_name() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_3);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Stamp, [72.0, 700.0, 144.0, 720.0])
            .with_stamp_icon(StampIcon::TopSecret),
    );
    let doc = b.assemble_document().expect("assemble");
    let dict = annot_object(&doc, 3, 0);
    assert_eq!(dict.get("Subtype"), Some(&Value::Name("Stamp".into())));
    assert_eq!(dict.get("Name"), Some(&Value::Name("TopSecret".into())));
    // 缺省值 Draft 不代填：未声明 `/Name` 时不写条目。
    let mut plain = PdfBuilder::with_version(PdfVersion::V1_3);
    let p0 = plain.page(PageSize::A4);
    plain.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Stamp, [0.0, 0.0, 10.0, 10.0]),
    );
    let dict = annot_object(&plain.assemble_document().expect("assemble2"), 3, 0);
    assert!(
        dict.get("Name").is_none(),
        "default Draft must not be materialized"
    );
}

#[test]
fn stamp_icon_roundtrips_through_reader_as_typed_variant() {
    let (_out, doc) = build_and_parse(
        PdfAnnotation::new(AnnotationKind::Stamp, [72.0, 700.0, 144.0, 720.0])
            .with_stamp_icon(StampIcon::NotForPublicRelease),
        PdfVersion::V1_3,
    );
    let annotation = &doc.page_annotations(0).expect("annotations")[0];
    assert_eq!(annotation.subtype, AnnotationSubtype::Stamp);
    assert_eq!(
        annotation.stamp_icon(),
        Some(ReaderStampIcon::NotForPublicRelease)
    );
}

#[test]
fn stamp_keeps_unknown_icon_name_verbatim_and_distinguishable() {
    let (_out, doc) = build_and_parse(
        PdfAnnotation::new(AnnotationKind::Stamp, [72.0, 700.0, 144.0, 720.0])
            .with_icon_name("RubberDuck"),
        PdfVersion::V1_3,
    );
    let annotation = &doc.page_annotations(0).expect("annotations")[0];
    assert_eq!(
        annotation.stamp_icon(),
        Some(ReaderStampIcon::Other("RubberDuck".into()))
    );
    assert!(!annotation.stamp_icon().unwrap().is_standard());
}

#[test]
fn stamp_icon_name_rejects_malformed_names_with_zero_bytes() {
    for bad in ["", "With Space", "Bad/Slash"] {
        let mut b = PdfBuilder::with_version(PdfVersion::V1_3);
        let p0 = b.page(PageSize::A4);
        b.add_annotation(
            p0,
            PdfAnnotation::new(AnnotationKind::Stamp, [0.0, 0.0, 10.0, 10.0]).with_icon_name(bad),
        );
        let mut out = Vec::new();
        let error = b.build(&mut out).expect_err("invalid /Name value");
        assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
        assert!(error.to_string().contains("annotation /Name"), "{error}");
        assert_eq!(out.len(), 0, "zero-byte contract for {bad:?}");
    }
}

#[test]
fn icon_name_rejected_on_text_markup_annotations() {
    for kind in [
        AnnotationKind::Highlight,
        AnnotationKind::Underline,
        AnnotationKind::Squiggly,
        AnnotationKind::StrikeOut,
    ] {
        let mut b = PdfBuilder::with_version(PdfVersion::V1_4);
        let p0 = b.page(PageSize::A4);
        b.add_annotation(
            p0,
            PdfAnnotation::new(kind, [0.0, 0.0, 10.0, 10.0])
                .with_quad_point_groups(vec![ONE_QUAD])
                .with_icon_name("Comment"),
        );
        let mut out = Vec::new();
        let error = b.build(&mut out).expect_err("/Name not allowed here");
        assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
        assert!(
            error
                .to_string()
                .contains("specific to Text or Sound or Stamp annotations"),
            "{error}"
        );
        assert_eq!(out.len(), 0);
    }
}

#[test]
fn text_markup_writes_quad_point_groups_flattened() {
    let second = [72.0, 692.0, 200.0, 692.0, 72.0, 680.0, 200.0, 680.0];
    let mut b = PdfBuilder::with_version(PdfVersion::V1_3);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Underline, [72.0, 680.0, 200.0, 712.0])
            .with_quad_point_groups(vec![ONE_QUAD, second]),
    );
    let doc = b.assemble_document().expect("assemble");
    let dict = annot_object(&doc, 3, 0);
    let quad = dict
        .get("QuadPoints")
        .and_then(|v| v.as_array())
        .expect("/QuadPoints array");
    assert_eq!(quad.len(), 16, "two groups of eight numbers");
    let expected: Vec<f64> = ONE_QUAD.into_iter().chain(second).collect();
    for (index, item) in quad.iter().enumerate() {
        let value = match item {
            Value::Float(v) => *v,
            Value::Integer(v) => *v as f64,
            other => panic!("non numeric /QuadPoints entry {other:?}"),
        };
        assert_eq!(value, expected[index]);
    }
}

#[test]
fn text_markup_quad_groups_roundtrip_through_reader() {
    let (_out, doc) = build_and_parse(
        PdfAnnotation::new(AnnotationKind::StrikeOut, [72.0, 680.0, 200.0, 712.0])
            .with_quad_point_groups(vec![ONE_QUAD]),
        PdfVersion::V1_3,
    );
    let annotation = &doc.page_annotations(0).expect("annotations")[0];
    assert_eq!(
        annotation.text_markup_quad_groups().expect("groups"),
        vec![ONE_QUAD]
    );
}

#[test]
fn text_markup_rejects_nonfinite_and_misshaped_quad_points_with_zero_bytes() {
    let mut bad_group = ONE_QUAD;
    bad_group[3] = f64::NAN;
    for case in [
        // 非 8 的倍数（Table 179：8 × n 个数）。
        PdfAnnotation::new(AnnotationKind::Highlight, [0.0, 0.0, 10.0, 10.0])
            .with_quad_points(vec![0.0, 1.0, 2.0]),
        // 非有限坐标。
        PdfAnnotation::new(AnnotationKind::Highlight, [0.0, 0.0, 10.0, 10.0])
            .with_quad_point_groups(vec![bad_group]),
    ] {
        let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
        let p0 = b.page(PageSize::A4);
        b.add_annotation(p0, case);
        let mut out = Vec::new();
        let error = b.build(&mut out).expect_err("invalid /QuadPoints");
        assert!(matches!(error, WriteError::InvalidObject(_)), "{error}");
        assert!(error.to_string().contains("/QuadPoints"), "{error}");
        assert_eq!(out.len(), 0, "zero-byte contract");
    }
}

#[test]
fn stamp_and_text_markup_versions_follow_table_169_subtype_gate() {
    // Stamp（PDF 1.3）与 `/Name`（Table 181 无版本标注）：1.3 可写，1.2 拒绝。
    build_one(
        PdfAnnotation::new(AnnotationKind::Stamp, [0.0, 0.0, 10.0, 10.0])
            .with_stamp_icon(StampIcon::Approved),
        PdfVersion::V1_3,
    )
    .expect("stamp at 1.3");
    let mut b = PdfBuilder::with_version(PdfVersion::V1_2);
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Stamp, [0.0, 0.0, 10.0, 10.0])
            .with_stamp_icon(StampIcon::Approved),
    );
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("stamp rejected at 1.2");
    assert!(
        matches!(error, WriteError::UnsupportedVersion(_)),
        "{error}"
    );
    assert_eq!(out.len(), 0);

    // Squiggly（PDF 1.4）：1.3 拒绝、1.4 可写；`/QuadPoints` 条目本身
    // （Table 179 无版本标注）随子类型门禁，不单设 PDF 1.6 门。
    for (version, expected) in [(PdfVersion::V1_3, false), (PdfVersion::V1_4, true)] {
        let result = build_one(
            PdfAnnotation::new(AnnotationKind::Squiggly, [0.0, 0.0, 10.0, 10.0])
                .with_quad_point_groups(vec![ONE_QUAD]),
            version,
        );
        assert_eq!(result.is_ok(), expected, "{version:?} squiggly");
    }
    // Highlight 在 PDF 1.3 写出 `/QuadPoints` 不再被误判为 1.6 条目。
    build_one(
        PdfAnnotation::new(AnnotationKind::Highlight, [0.0, 0.0, 10.0, 10.0])
            .with_quad_point_groups(vec![ONE_QUAD]),
        PdfVersion::V1_3,
    )
    .expect("highlight quad points at 1.3");
}
