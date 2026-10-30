//! 文章线写出 → 解析 → 回读往返（12.4.3 Articles、Table 160/161）。

use super::*;

/// 写入 → 重新解析：两 bead 文章线（首页 + 次页）带线程信息，读端
/// `article_threads` 还原链序、页索引与矩形，页面 `/B` 与目录 `/Threads`
/// 同时可读；特性画像登记文章线。
#[test]
fn builder_article_thread_roundtrips_through_parser() {
    use pdf_engine::core::builder::{
        ArticleBead, ArticleThread, DocumentInfo, PageSize, PdfBuilder,
    };
    use pdf_engine::core::types::{PdfFeature, PdfVersion, Value};

    let mut builder = PdfBuilder::with_version(PdfVersion::V1_7);
    builder.page(PageSize::A4);
    builder.page(PageSize::A4);
    builder.add_article_thread(
        ArticleThread::new(vec![
            ArticleBead {
                page: 0,
                rect: [10.0, 20.0, 30.0, 40.0],
            },
            ArticleBead {
                page: 1,
                rect: [50.0, 60.0, 70.0, 80.0],
            },
        ])
        .with_info(DocumentInfo::new().title("Story")),
    );

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build article thread");

    let doc = parse_buf(&buf);
    let threads = doc.article_threads().expect("article threads");
    let features: Vec<PdfFeature> = doc.features.borrow().iter().copied().collect();
    assert!(features.contains(&PdfFeature::ArticleThreads));
    assert_eq!(threads.len(), 1);
    let thread = &threads[0];
    assert_eq!(thread.beads.len(), 2);
    assert_eq!(thread.beads[0].page, 0);
    assert_eq!(thread.beads[0].rect, [10.0, 20.0, 30.0, 40.0]);
    assert_eq!(thread.beads[1].page, 1);
    assert_eq!(thread.beads[1].rect, [50.0, 60.0, 70.0, 80.0]);
    let info = thread.info.as_ref().expect("thread /I");
    assert_eq!(
        info.get("Title"),
        Some(&Value::HexString("53746F7279".into()))
    );

    let (catalog_num, _) = doc
        .trailer
        .dict
        .get("Root")
        .and_then(Value::as_ref)
        .expect("/Root");
    let catalog = doc
        .get_object(catalog_num)
        .expect("read catalog")
        .expect("catalog exists")
        .value;
    let pinfo = catalog.as_dict().expect("catalog dict");
    let threads = pinfo
        .get("Threads")
        .and_then(Value::as_array)
        .expect("catalog /Threads");
    let (thread_num, _) = threads.0[0].as_ref().expect("thread ref");
    let thread = doc
        .get_object(thread_num)
        .expect("read thread")
        .expect("thread exists")
        .value;
    let thread = thread.as_dict().expect("thread dict");
    let (first_bead, _) = thread.get("F").and_then(Value::as_ref).expect("/F");
    let first = doc
        .get_object(first_bead)
        .expect("read bead")
        .expect("bead exists")
        .value;
    let first = first.as_dict().expect("bead dict");
    let (page_num, _) = first.get("P").and_then(Value::as_ref).expect("/P");
    let page = doc
        .get_object(page_num)
        .expect("read page")
        .expect("page exists")
        .value;
    let page = page.as_dict().expect("page dict");
    let beads = page.get("B").and_then(Value::as_array).expect("page /B");
    assert_eq!(beads.0, vec![Value::Ref(first_bead, 0)]);
}
