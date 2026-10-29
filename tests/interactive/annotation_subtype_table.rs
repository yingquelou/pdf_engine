//! E2：写端支持的 19 种子类型逐个写出后，读端解析的 `AnnotationSubtype`
//! 与写端 `AnnotationKind::subtype()` 一致（12.5.6.1 Table 169，读写共用单点表）。

use super::*;

/// 写端支持的 19 种子类型逐个写出后，读端解析的 `AnnotationSubtype`
/// 与写端 `AnnotationKind::subtype()` 一致，且名称、引入版本与标记类归属
/// 全部落在同一张 Table 169 上（读写共用单点表）。
#[test]
fn builder_annotation_subtype_table_roundtrips_through_parser() {
    use pdf_engine::core::builder::{
        AnnotationKind, EmbeddedFile, PageSize, PdfAnnotation, PdfBuilder,
    };
    use pdf_engine::core::reader::annotation::TABLE_169;

    const KINDS: [AnnotationKind; 19] = [
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
        AnnotationKind::Redact,
        AnnotationKind::Screen,
    ];

    let mut builder = PdfBuilder::new();
    let p0 = builder.page(PageSize::A4);
    // `FileAttachment` 注解必须带 `/FS` 文件规格句柄（E6 构建期校验）。
    let filespec = builder.embed_file(EmbeddedFile::new("note.txt", b"subtype table".to_vec()));
    for (index, kind) in KINDS.iter().enumerate() {
        let top = 740.0 - (index as f64) * 20.0;
        let rect = [72.0, top - 10.0, 200.0, top];
        let mut annot = PdfAnnotation::new(*kind, rect);
        if *kind == AnnotationKind::Line {
            annot = annot.with_line_endpoints(rect);
        }
        if *kind == AnnotationKind::FreeText {
            // `/DA` 是 Table 174 的必填条目。
            annot = annot.with_default_appearance("/Helv 12 Tf 0 g");
        }
        if *kind == AnnotationKind::FileAttachment {
            annot = annot.with_filespec(filespec);
        }
        if *kind == AnnotationKind::Popup {
            // `/Parent` 是 Table 183 必填条目：回指首个注解（Text），
            // Text 端的 `/Popup` 由写端自动补出。
            annot = annot.with_popup_parent(0);
        }
        if matches!(kind, AnnotationKind::Polygon | AnnotationKind::PolyLine) {
            // `/Vertices` 是 Table 178 的必填条目。
            annot = annot.with_vertices(vec![rect[0], rect[1], rect[2], rect[1], rect[2], rect[3]]);
        }
        if matches!(
            kind,
            AnnotationKind::Highlight
                | AnnotationKind::Underline
                | AnnotationKind::Squiggly
                | AnnotationKind::StrikeOut
        ) {
            // `/QuadPoints` 是 Table 179 的必填条目。
            annot = annot.with_quad_points(vec![
                rect[0], rect[3], rect[2], rect[3], rect[0], rect[1], rect[2], rect[1],
            ]);
        }
        builder.add_annotation(p0, annot);
    }
    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build 19 subtypes");

    let doc = parse_buf(&buf);
    let annotations = doc.page_annotations(0).expect("annotations");
    assert_eq!(annotations.len(), KINDS.len());
    for (kind, annotation) in KINDS.iter().zip(&annotations) {
        let row = TABLE_169
            .iter()
            .find(|(name, _, _)| *name == kind.name())
            .unwrap_or_else(|| panic!("Table 169 无 {} 行", kind.name()));
        assert_eq!(annotation.subtype, kind.subtype(), "{}", kind.name());
        assert_eq!(annotation.subtype.name(), kind.name());
        assert_eq!(
            annotation.subtype.min_version(),
            Some(row.1),
            "{}",
            kind.name()
        );
        assert_eq!(annotation.subtype.is_markup(), row.2, "{}", kind.name());
    }
    // 只声明 `/Parent` 的一端：Text 的 `/Popup` 自动指回该弹出框。
    let popup_ref = annotations[0].popup.expect("auto-paired /Popup");
    let popup_obj = doc
        .get_object(popup_ref.0)
        .expect("popup object")
        .expect("exist");
    assert_eq!(
        popup_obj
            .value
            .as_dict()
            .expect("popup dict")
            .get("Subtype"),
        Some(&Value::Name("Popup".into())),
        "auto-paired /Popup must reference the Popup annotation"
    );
}
