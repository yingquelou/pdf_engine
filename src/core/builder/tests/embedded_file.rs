//! 嵌入文件写端测试（7.11.2–7.11.4）：文件规格 + 嵌入流对象对、
//! 目录 `/Names /EmbeddedFiles` 名称树、`/FS` 注解联动与版本门禁。

use super::*;
use crate::core::builder::navigation;
use crate::core::reader::navigation::Destination;
use md5::{Digest, Md5};

fn object_dict(doc: &Document, num: i64) -> Dict {
    doc.get_object(num)
        .expect("object lookup")
        .expect("object exists")
        .value
        .as_dict()
        .expect("dict object")
        .clone()
}

/// 从目录取 `/Names /EmbeddedFiles` 唯一条目，返回（键, 文件规格对象号, 字典）。
fn single_embedded_entry(doc: &Document) -> (Value, i64, Dict) {
    let catalog = object_dict(doc, 1);
    let names = catalog
        .get("Names")
        .and_then(Value::as_dict)
        .expect("/Names");
    let node = names
        .get("EmbeddedFiles")
        .and_then(Value::as_dict)
        .expect("/EmbeddedFiles node");
    let array = node
        .get("Names")
        .and_then(Value::as_array)
        .expect("leaf /Names");
    assert_eq!(array.0.len(), 2, "单条目叶子应为 [key value]");
    let (num, _) = array.0[1]
        .as_ref()
        .expect("filespec should be an indirect reference");
    (array.0[0].clone(), num, object_dict(doc, num))
}

/// 取文件规格 `/EF` 载体字典（`/F`）引用的流对象字典与字节。
fn ef_stream(doc: &Document, filespec: &Dict) -> (i64, Dict, Vec<u8>) {
    let (num, _) = filespec
        .get("EF")
        .and_then(Value::as_dict)
        .and_then(|carriers| carriers.get("F"))
        .and_then(Value::as_ref)
        .expect("/EF /F should reference the embedded stream");
    let object = doc
        .get_object(num)
        .expect("stream lookup")
        .expect("stream object");
    match &object.value {
        Value::Stream(dict, StreamData::Owned(bytes)) => (num, dict.clone(), bytes.clone()),
        other => panic!("embedded file should be an owned stream, got {other:?}"),
    }
}

/// 文本串一律经 `text_string_value` 写成十六进制。
fn assert_text(value: &Value, key: &str) {
    assert!(
        matches!(value, Value::HexString(_) | Value::String(_)),
        "expected text string at {key}, got {value:?}"
    );
}

fn sample_file() -> EmbeddedFile {
    EmbeddedFile::new("notes.txt", b"a b\tc\n".to_vec())
        .with_subtype("text/plain")
        .with_description("Meeting notes")
        .with_creation_date("D:20261028050000Z")
        .with_modification_date("D:20261028060000Z")
}

#[test]
fn embedded_file_writes_filespec_stream_pair_and_name_tree() {
    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    b.embed_file(sample_file());
    let doc = b.assemble_document().expect("assemble");

    let (key, filespec_num, filespec) = single_embedded_entry(&doc);
    assert_text(&key, "name tree key");
    assert_eq!(filespec.get("Type"), Some(&Value::Name("Filespec".into())));
    assert_text(filespec.get("F").expect("/F"), "/F");
    assert_text(filespec.get("UF").expect("/UF"), "/UF");
    assert_text(filespec.get("Desc").expect("/Desc"), "/Desc");

    let (stream_num, stream_dict, data) = ef_stream(&doc, &filespec);
    assert_eq!(data, b"a b\tc\n");
    assert_eq!(
        stream_dict.get("Type"),
        Some(&Value::Name("EmbeddedFile".into()))
    );
    assert_eq!(
        stream_dict.get("Subtype"),
        Some(&Value::Name("text/plain".into()))
    );
    let params = stream_dict
        .get("Params")
        .and_then(Value::as_dict)
        .expect("/Params");
    assert_eq!(params.get("Size"), Some(&Value::Integer(6)));
    assert_text(
        params.get("CreationDate").expect("/CreationDate"),
        "/CreationDate",
    );
    assert_text(params.get("ModDate").expect("/ModDate"), "/ModDate");
    // 文件规格在前、流在后成对分配。
    assert_eq!(stream_num, filespec_num + 1);
}

#[test]
fn embedded_checksum_is_md5_of_whitespace_stripped_data() {
    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    b.embed_file(sample_file());
    let doc = b.assemble_document().expect("assemble");
    let (_, _, filespec) = single_embedded_entry(&doc);
    let (_, stream_dict, data) = ef_stream(&doc, &filespec);

    let params = stream_dict
        .get("Params")
        .and_then(Value::as_dict)
        .expect("/Params");
    let expected = Md5::digest(
        data.iter()
            .copied()
            .filter(|byte| !matches!(byte, 0 | 0o11 | 0o12 | 0o14 | 0o15 | b' '))
            .collect::<Vec<u8>>(),
    );
    let expected_hex = expected
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<String>();
    assert_eq!(
        params.get("CheckSum"),
        Some(&Value::HexString(expected_hex)),
        "/CheckSum 应为去空白数据的 MD5（7.11.4.3）"
    );
}

#[test]
fn file_attachment_annotation_links_filespec_handle() {
    let mut b = PdfBuilder::new();
    let p0 = b.page(PageSize::A4);
    let handle = b.embed_file(EmbeddedFile::new("data.csv", b"1,2,3".to_vec()));
    b.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::FileAttachment, [72.0, 700.0, 92.0, 720.0])
            .with_filespec(handle)
            .with_contents("attached data"),
    );
    let doc = b.assemble_document().expect("assemble");

    let page = object_dict(&doc, 3);
    let annots = page
        .get("Annots")
        .and_then(Value::as_array)
        .expect("/Annots");
    let (annot_num, _) = annots.0[0].as_ref().expect("annot ref");
    let annot = object_dict(&doc, annot_num);
    assert_eq!(
        annot.get("Subtype"),
        Some(&Value::Name("FileAttachment".into()))
    );
    let (filespec_num, _) = annot.get("FS").and_then(Value::as_ref).expect("/FS ref");
    let filespec = object_dict(&doc, filespec_num);
    assert_eq!(filespec.get("Type"), Some(&Value::Name("Filespec".into())));
    let (_, _, data) = ef_stream(&doc, &filespec);
    assert_eq!(data, b"1,2,3");
}

#[test]
fn file_attachment_requires_known_filespec_handle() {
    // FileAttachment 缺少 `/FS` → InvalidObject，零字节。
    let mut b = PdfBuilder::new();
    let p0 = b.page(PageSize::A4);
    b.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::FileAttachment, [0.0, 0.0, 10.0, 10.0]),
    );
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("FileAttachment without /FS must be rejected");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(error.to_string().contains("requires /FS"), "{error}");
    assert!(out.is_empty(), "构建失败时不应写出任何字节");

    // `/FS` 句柄越界 → InvalidObject，零字节。
    let mut b = PdfBuilder::new();
    let p0 = b.page(PageSize::A4);
    b.embed_file(EmbeddedFile::new("a.txt", b"x".to_vec()));
    b.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::FileAttachment, [0.0, 0.0, 10.0, 10.0])
            .with_filespec(99),
    );
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("unknown /FS handle must be rejected");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(
        error.to_string().contains("/FS references unknown"),
        "{error}"
    );
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

#[test]
fn embedded_files_are_version_gated() {
    // 1.2 拒绝任何嵌入文件。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_2);
    b.page(PageSize::A4);
    b.embed_file(EmbeddedFile::new("a.txt", b"x".to_vec()));
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("PDF 1.2 must reject embedded files");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(_)),
        "错误应为 UnsupportedVersion: {error}"
    );
    assert!(error.to_string().contains("embedded files"), "{error}");
    assert!(out.is_empty(), "构建失败时不应写出任何字节");

    // 1.3 拒绝 `/Desc`（PDF 1.6 引入）。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_3);
    b.page(PageSize::A4);
    b.embed_file(EmbeddedFile::new("a.txt", b"x".to_vec()).with_description("note"));
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("PDF 1.3 must reject /Desc");
    assert!(
        matches!(&error, WriteError::UnsupportedVersion(_)),
        "错误应为 UnsupportedVersion: {error}"
    );
    assert!(error.to_string().contains("/Desc"), "{error}");
    assert!(out.is_empty(), "构建失败时不应写出任何字节");

    // 1.5 仍拒绝 `/Desc`。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_5);
    b.page(PageSize::A4);
    b.embed_file(EmbeddedFile::new("a.txt", b"x".to_vec()).with_description("note"));
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("PDF 1.5 must reject /Desc");
    assert!(error.to_string().contains("/Desc"), "{error}");
    assert!(out.is_empty(), "构建失败时不应写出任何字节");

    // 1.6 接受 `/Desc`，但不写 `/UF`。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_6);
    b.page(PageSize::A4);
    b.embed_file(EmbeddedFile::new("a.txt", b"x".to_vec()).with_description("note"));
    let s = render(&mut b);
    assert!(s.contains("/Desc "), "1.6 写出 /Desc: {s}");
    assert!(!s.contains("/UF "), "1.6 不写 /UF: {s}");

    // 1.3 拒绝非 ASCII 展示名（即便显式给出 `/F` 短名）。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_3);
    b.page(PageSize::A4);
    b.embed_file(EmbeddedFile::new("报告.pdf", b"x".to_vec()).with_short_name("report.pdf"));
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("PDF 1.3 must reject /UF names");
    assert!(error.to_string().contains("/UF"), "{error}");
    assert!(out.is_empty(), "构建失败时不应写出任何字节");

    // 1.6 仍拒绝非 ASCII 展示名（`/UF` 为 PDF 1.7）。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_6);
    b.page(PageSize::A4);
    b.embed_file(EmbeddedFile::new("报告.pdf", b"x".to_vec()).with_short_name("report.pdf"));
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("PDF 1.6 must reject /UF names");
    assert!(error.to_string().contains("/UF"), "{error}");
    assert!(out.is_empty(), "构建失败时不应写出任何字节");

    // 1.3 接受纯 ASCII 名：写出 `/F`，不写 `/UF`。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_3);
    b.page(PageSize::A4);
    b.embed_file(EmbeddedFile::new("a.txt", b"x".to_vec()));
    let s = render(&mut b);
    assert!(s.contains("/Type /Filespec"), "{s}");
    assert!(!s.contains("/UF "), "1.3 不写 /UF: {s}");

    // 1.7 写出 `/UF`，MIME 名中的 `/` 以 `#2F` 转义。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    b.page(PageSize::A4);
    b.embed_file(
        EmbeddedFile::new("报告.pdf", b"x".to_vec())
            .with_short_name("report.pdf")
            .with_subtype("application/pdf"),
    );
    let s = render(&mut b);
    assert!(s.contains("/UF "), "1.7 写出 /UF: {s}");
    assert!(s.contains("/F "), "{s}");
    assert!(
        s.contains("/Subtype /application#2Fpdf"),
        "MIME 名应转义 solidus: {s}"
    );
}

#[test]
fn invalid_embedded_names_are_rejected() {
    let cases: Vec<(EmbeddedFile, &str)> = vec![
        (EmbeddedFile::new("", b"x".to_vec()), "must not be empty"),
        (
            EmbeddedFile::new("dir/a.txt", b"x".to_vec()),
            "must not contain path separators",
        ),
        (
            EmbeddedFile::new("报告.pdf", b"x".to_vec()),
            "requires an explicit ASCII short name",
        ),
        (
            EmbeddedFile::new("a.txt", b"x".to_vec()).with_short_name("非ASCII短名.txt"),
            "/F short name must be pure ASCII",
        ),
    ];
    for (file, expected) in cases {
        let mut b = PdfBuilder::new();
        b.page(PageSize::A4);
        b.embed_file(file);
        let mut out = Vec::new();
        let error = b.build(&mut out).expect_err("非法嵌入文件名应被拒绝");
        assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
        assert!(error.to_string().contains(expected), "{error}");
        assert!(out.is_empty(), "构建失败时不应写出任何字节");
    }

    // 展示名重复 → InvalidObject。
    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    b.embed_file(EmbeddedFile::new("a.txt", b"x".to_vec()));
    b.embed_file(EmbeddedFile::new("a.txt", b"y".to_vec()));
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("duplicate name must be rejected");
    assert!(matches!(&error, WriteError::InvalidObject(_)), "{error}");
    assert!(error.to_string().contains("duplicate"), "{error}");
    assert!(out.is_empty(), "构建失败时不应写出任何字节");
}

#[test]
fn embedded_names_tree_merges_with_navigation_entries() {
    let mut b = PdfBuilder::new();
    let p0 = b.page(PageSize::A4);
    let _ = b.embed_file(EmbeddedFile::new("a.txt", b"x".to_vec()));
    b.named_destination("intro", Destination::Fit { page: p0 });
    let mut doc = b.assemble_document().expect("assemble");
    navigation::apply(
        &mut doc,
        b.version,
        b.pages.len(),
        &b.navigation,
        crate::core::builder::types::ActionRefs {
            annotation_nums: &b.annotation_nums,
            ocg_nums: &b.ocg_nums,
            sound_nums: &b.sound_nums,
            form_nums: &b.form_nums,
            embedded_nums: &b.embedded_nums,
        },
    )
    .expect("apply");

    let catalog = object_dict(&doc, 1);
    let names = catalog
        .get("Names")
        .and_then(Value::as_dict)
        .expect("/Names 应与嵌入文件条目合并");
    assert!(
        names.get("Dests").and_then(Value::as_dict).is_some(),
        "/Dests 应保留"
    );
    assert!(
        names
            .get("EmbeddedFiles")
            .and_then(Value::as_dict)
            .is_some(),
        "/EmbeddedFiles 不应被覆盖"
    );
}
