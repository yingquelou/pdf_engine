//! E6 嵌入文件：文件规格、嵌入流与 `/EmbeddedFiles` 名称树的写出→解析往返。

use super::*;

/// 写入 → 重新解析：`/EmbeddedFiles` 名称树、文件规格（`/F` `/UF` `/Desc`
/// `/EF`）、`/Params`（`/Size` `/CheckSum`）与 FileAttachment 注解 `/FS`
/// 应经 `Document::embedded_files` 完整回读（7.11.2–7.11.4、12.5.6.14）。
#[test]
fn builder_embedded_files_roundtrip_through_parser() {
    use pdf_engine::core::builder::{
        AnnotationKind, EmbeddedFile, PageSize, PdfAnnotation, PdfBuilder,
    };
    use pdf_engine::core::reader::annotation::AnnotationSubtype;

    let notes_payload = b"line one \t line two\n".to_vec();
    let report_payload = b"%PDF-1.4 fake report bytes".to_vec();

    let mut builder = PdfBuilder::new();
    let p0 = builder.page(PageSize::A4);
    let notes_handle = builder.embed_file(
        EmbeddedFile::new("notes.txt", notes_payload.clone())
            .with_subtype("text/plain")
            .with_creation_date("D:20261028050000Z"),
    );
    let report_handle = builder.embed_file(
        EmbeddedFile::new("报告.pdf", report_payload.clone())
            .with_short_name("report.pdf")
            .with_subtype("application/pdf")
            .with_description("季度报告"),
    );
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::FileAttachment, [72.0, 700.0, 92.0, 720.0])
            .with_filespec(report_handle),
    );
    let _ = notes_handle;

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build");
    let doc = parse_buf(&buf);

    let files = doc.embedded_files().expect("embedded files");
    assert_eq!(files.len(), 2, "两个注册项应各占一个名称树条目");

    let notes = files
        .get("notes.txt")
        .expect("ASCII 名按解码键命中")
        .clone();
    assert_eq!(notes.short_name.as_deref(), Some("notes.txt"));
    assert_eq!(notes.unicode_name.as_deref(), Some("notes.txt"));
    assert!(notes.description.is_none());
    let notes_stream = notes.primary().expect("notes 嵌入流");
    assert_eq!(notes_stream.subtype.as_deref(), Some("text/plain"));
    assert_eq!(notes_stream.data, notes_payload, "字节应原样往返");
    assert_eq!(notes_stream.size_matches(), Some(true));
    assert_eq!(notes_stream.checksum_matches(), Some(true));
    assert!(notes_stream.params.creation_date.is_some());

    let report = files
        .get("报告.pdf")
        .expect("UTF-16BE 键应解码命中")
        .clone();
    assert_eq!(report.unicode_name.as_deref(), Some("报告.pdf"));
    assert_eq!(report.short_name.as_deref(), Some("report.pdf"));
    assert_eq!(report.description.as_deref(), Some("季度报告"));
    let report_stream = report.primary().expect("report 嵌入流");
    assert_eq!(report_stream.subtype.as_deref(), Some("application/pdf"));
    assert_eq!(&report_stream.data, &b"%PDF-1.4 fake report bytes"[..]);
    assert_eq!(report_stream.size_matches(), Some(true));
    assert_eq!(report_stream.checksum_matches(), Some(true));

    // `/F` 短名回退查找。
    let by_short = doc
        .embedded_file("report.pdf")
        .expect("lookup")
        .expect("短名应命中");
    assert_eq!(by_short.name(), "报告.pdf");

    // FileAttachment 注解 `/FS` → 同一文件规格。
    let annotations = doc.page_annotations(0).expect("page annotations");
    assert_eq!(annotations.len(), 1);
    assert_eq!(annotations[0].subtype, AnnotationSubtype::FileAttachment);
    let filespec = doc
        .annotation_filespec(&annotations[0])
        .expect("filespec")
        .expect("附件注解有 /FS");
    assert_eq!(filespec.name(), "报告.pdf");
    assert_eq!(
        filespec.primary().expect("stream").data,
        b"%PDF-1.4 fake report bytes".to_vec()
    );
}
