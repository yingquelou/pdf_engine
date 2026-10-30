//! 文章线写端测试（ISO 32000-1 12.4.3，Table 160/161）：线程与 bead
//! 对象、`/N` `/V` 闭合链、页面 `/B`、目录 `/Threads` 与 PDF 1.1 门禁。

use super::*;
use crate::core::types::{Array, PdfFeature, PdfTextString};

fn page_dict(doc: &Document, num: i64) -> Dict {
    object_dict(doc, num)
}

fn object_dict(doc: &Document, num: i64) -> Dict {
    doc.get_object(num)
        .expect("object")
        .expect("object exists")
        .value
        .as_dict()
        .expect("object dict")
        .clone()
}

fn catalog(doc: &Document) -> Dict {
    let (root, _generation) = doc
        .trailer
        .dict
        .get("Root")
        .and_then(Value::as_ref)
        .expect("/Root");
    object_dict(doc, root)
}

fn hex_of(text: &str) -> String {
    PdfTextString::from(text)
        .encode()
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect()
}

fn bead(page: usize, rect: [f64; 4]) -> ArticleBead {
    ArticleBead { page, rect }
}

/// 导航后处理只在 `build` 中执行；写出后重新解析以检查对象图。
fn build_and_parse(b: &mut PdfBuilder) -> Document {
    let mut out = Vec::new();
    b.build(&mut out).expect("build");
    // 与集成测试一致：沙箱禁止写系统临时目录，临时文件落在 target/test-tmp。
    let base = std::env::current_exe()
        .ok()
        .and_then(|p| p.ancestors().nth(3).map(|t| t.to_path_buf()))
        .filter(|p| p.file_name() == Some(std::ffi::OsStr::new("target")))
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join("test-tmp");
    std::fs::create_dir_all(&dir).expect("create test-tmp");
    let path = dir.join(format!(
        "pdf-engine-articles-{}-{}.pdf",
        std::process::id(),
        out.len()
    ));
    std::fs::write(&path, &out).expect("write temp pdf");
    let file = std::fs::File::open(&path).expect("open temp pdf");
    let mut parser = crate::core::reader::Parser::new(file).expect("parser");
    let doc = parser.parse_document().expect("parse");
    let _ = std::fs::remove_file(&path);
    doc
}

#[test]
fn article_threads_are_written_and_chained() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    b.page(PageSize::A4);
    b.page(PageSize::A4);
    b.page(PageSize::A4);
    b.add_article_thread(
        ArticleThread::new(vec![
            bead(0, [10.0, 20.0, 30.0, 40.0]),
            bead(2, [50.0, 60.0, 70.0, 80.0]),
        ])
        .with_info(DocumentInfo::new().title("Story")),
    );
    b.add_article_thread(ArticleThread::new(vec![bead(0, [5.0, 6.0, 7.0, 8.0])]));

    let doc = build_and_parse(&mut b);

    let threads = catalog(&doc)
        .get("Threads")
        .and_then(Value::as_array)
        .expect("/Threads")
        .clone();
    let features: Vec<PdfFeature> = doc.features.borrow().iter().copied().collect();
    assert!(features.contains(&PdfFeature::ArticleThreads));
    assert_eq!(threads.len(), 2);
    let (first_thread, _) = threads.0[0].as_ref().expect("thread ref");
    let (second_thread, _) = threads.0[1].as_ref().expect("thread ref");

    let thread = object_dict(&doc, first_thread);
    assert_eq!(thread.get("Type"), Some(&Value::Name("Thread".into())));
    let (first_bead, _) = thread.get("F").and_then(Value::as_ref).expect("/F");
    let info = thread.get("I").and_then(Value::as_dict).expect("thread /I");
    assert_eq!(info.get("Title"), Some(&Value::HexString(hex_of("Story"))));

    let first = object_dict(&doc, first_bead);
    assert_eq!(first.get("Type"), Some(&Value::Name("Bead".into())));
    assert_eq!(first.get("T"), Some(&Value::Ref(first_thread, 0)));
    assert_eq!(first.get("P"), Some(&Value::Ref(3, 0)), "bead page 0 ref");
    assert_eq!(
        first.get("R"),
        Some(&Value::Array(Array(vec![
            Value::Float(10.0),
            Value::Float(20.0),
            Value::Float(30.0),
            Value::Float(40.0),
        ])))
    );
    let (second_bead, _) = first.get("N").and_then(Value::as_ref).expect("/N");
    let second = object_dict(&doc, second_bead);
    assert_eq!(second.get("T"), None, "/T 只写在首个 bead");
    assert_eq!(second.get("P"), Some(&Value::Ref(7, 0)), "bead page 2 ref");
    assert_eq!(second.get("N"), Some(&Value::Ref(first_bead, 0)));
    assert_eq!(second.get("V"), Some(&Value::Ref(first_bead, 0)));
    assert_eq!(first.get("V"), Some(&Value::Ref(second_bead, 0)));

    let first_page = page_dict(&doc, 3);
    let page_beads = first_page
        .get("B")
        .and_then(Value::as_array)
        .expect("page /B");
    let (second_thread_bead, _) = object_dict(&doc, second_thread)
        .get("F")
        .and_then(Value::as_ref)
        .expect("second thread /F");
    assert_eq!(
        page_beads.0,
        vec![Value::Ref(first_bead, 0), Value::Ref(second_thread_bead, 0)],
        "同一页面上的 bead 按线程注册顺序排列"
    );

    assert!(page_dict(&doc, 5).get("B").is_none(), "页面 1 无 bead");
    assert_eq!(
        page_dict(&doc, 7)
            .get("B")
            .and_then(Value::as_array)
            .map(|a| a.0.clone()),
        Some(vec![Value::Ref(second_bead, 0)])
    );
}

#[test]
fn article_threads_are_optional() {
    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    let doc = b.assemble_document().expect("assemble");

    let features: Vec<PdfFeature> = doc.features.borrow().iter().copied().collect();
    assert!(!features.contains(&PdfFeature::ArticleThreads));
    assert!(catalog(&doc).get("Threads").is_none());
    assert!(page_dict(&doc, 3).get("B").is_none());
}

#[test]
fn invalid_article_threads_are_rejected() {
    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    b.add_article_thread(ArticleThread::new(Vec::new()));
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("empty thread must be rejected");
    assert!(
        matches!(error, WriteError::InvalidObject(_)),
        "unexpected error {error:?}"
    );
    assert!(out.is_empty(), "非法输入必须零字节");

    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    b.add_article_thread(ArticleThread::new(vec![bead(9, [0.0, 0.0, 1.0, 1.0])]));
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("out-of-range page must be rejected");
    assert!(
        matches!(error, WriteError::InvalidObject(_)),
        "unexpected error {error:?}"
    );
    assert!(out.is_empty());

    let mut b = PdfBuilder::new();
    b.page(PageSize::A4);
    b.add_article_thread(ArticleThread::new(vec![bead(0, [0.0, f64::NAN, 1.0, 1.0])]));
    let mut out = Vec::new();
    let error = b
        .build(&mut out)
        .expect_err("non-finite /R must be rejected");
    assert!(
        matches!(error, WriteError::InvalidObject(_)),
        "unexpected error {error:?}"
    );
    assert!(out.is_empty());
}

#[test]
fn article_threads_require_pdf_1_1() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_0);
    b.page(PageSize::A4);
    b.add_article_thread(ArticleThread::new(vec![bead(0, [0.0, 0.0, 1.0, 1.0])]));
    let mut out = Vec::new();
    let error = b.build(&mut out).expect_err("PDF 1.0 must reject threads");
    assert!(
        matches!(error, WriteError::UnsupportedVersion(_)),
        "unexpected error {error:?}"
    );
    assert!(out.is_empty(), "版本门禁失败必须零字节");

    let mut b = PdfBuilder::with_version(PdfVersion::V1_1);
    b.page(PageSize::A4);
    b.add_article_thread(ArticleThread::new(vec![bead(0, [0.0, 0.0, 1.0, 1.0])]));
    let mut out = Vec::new();
    b.build(&mut out).expect("PDF 1.1 supports threads");
    assert!(!out.is_empty());
}
