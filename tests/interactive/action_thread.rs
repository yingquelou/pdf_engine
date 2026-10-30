//! Thread 动作写出 → 解析 → 回读往返（12.6.4.6，Table 205）。

use super::*;

/// 写入 → 重新解析：带两 bead 的文章线上，Link 注解携带 Thread 动作，
/// `/D` 以线程索引、`/B` 以 bead 索引写出；读端 `action` 还原为
/// `Thread(Index, Index)`，索引可解析到目录 `/Threads` 与线程 bead。
#[test]
fn builder_thread_action_roundtrips_through_parser() {
    use pdf_engine::core::builder::{
        Action, AnnotationKind, ArticleBead, ArticleThread, PageSize, PdfAnnotation, PdfBuilder,
        ThreadAction,
    };
    use pdf_engine::core::reader::action::{Action as ReadAction, BeadTarget, ThreadTarget};

    let mut builder = PdfBuilder::new();
    let p0 = builder.page(PageSize::A4);
    builder.page(PageSize::A4);
    builder.add_article_thread(ArticleThread::new(vec![
        ArticleBead {
            page: 0,
            rect: [10.0, 20.0, 30.0, 40.0],
        },
        ArticleBead {
            page: 1,
            rect: [50.0, 60.0, 70.0, 80.0],
        },
    ]));
    builder.add_annotation(
        p0,
        PdfAnnotation::new(AnnotationKind::Link, [72.0, 600.0, 300.0, 620.0])
            .with_action(Action::Thread(ThreadAction::new(0).with_bead(1))),
    );

    let mut buf = Vec::new();
    builder.build(&mut buf).expect("build thread action");

    let doc = parse_buf(&buf);
    let annotations = doc.page_annotations(0).expect("annotations");
    assert_eq!(annotations.len(), 1);
    match doc
        .action(annotations[0].action.as_ref().expect("/A"))
        .expect("parse thread action")
    {
        ReadAction::Thread(action) => {
            assert!(action.file.is_none());
            assert_eq!(action.thread, ThreadTarget::Index(0));
            assert_eq!(action.bead, Some(BeadTarget::Index(1)));
        }
        other => panic!("unexpected action: {other:?}"),
    }

    let threads = doc.article_threads().expect("article threads");
    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].beads.len(), 2);
}
