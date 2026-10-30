//! Thread 动作写端测试（12.6.4.6，Table 205）：`/D` 线程索引与 `/B`
//! bead 索引的写出、索引越界校验与 PDF 1.1 门禁。

use super::support::annot_object;
use super::*;
use crate::core::types::PdfFeature;

fn link_with_action(action: Action) -> PdfAnnotation {
    PdfAnnotation::new(AnnotationKind::Link, [72.0, 600.0, 300.0, 620.0]).with_action(action)
}

fn bead(page: usize, rect: [f64; 4]) -> ArticleBead {
    ArticleBead { page, rect }
}

#[test]
fn thread_action_writes_thread_and_bead_indices() {
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let p0 = b.page(PageSize::A4);
    b.page(PageSize::A4);
    b.add_article_thread(ArticleThread::new(vec![
        bead(0, [10.0, 20.0, 30.0, 40.0]),
        bead(1, [50.0, 60.0, 70.0, 80.0]),
    ]));
    b.add_article_thread(ArticleThread::new(vec![bead(1, [5.0, 6.0, 7.0, 8.0])]));
    b.add_annotation(
        p0,
        link_with_action(Action::Thread(ThreadAction::new(1).with_bead(0))),
    );

    let doc = b.assemble_document().expect("assemble");
    let link = annot_object(&doc, 3, 0);
    let action = link.get("A").and_then(Value::as_dict).expect("/A");
    assert_eq!(action.get("S"), Some(&Value::Name("Thread".into())));
    assert_eq!(action.get("D"), Some(&Value::Integer(1)));
    assert_eq!(action.get("B"), Some(&Value::Integer(0)));
    assert!(action.get("F").is_none(), "未声明的 /F 不物化");

    // 缺省 bead 时省略 /B。
    let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
    let p0 = b.page(PageSize::A4);
    b.add_article_thread(ArticleThread::new(vec![bead(0, [0.0, 0.0, 1.0, 1.0])]));
    b.add_annotation(p0, link_with_action(Action::Thread(ThreadAction::new(0))));
    let doc = b.assemble_document().expect("assemble");
    let link = annot_object(&doc, 3, 0);
    let action = link.get("A").and_then(Value::as_dict).expect("/A");
    assert_eq!(action.get("D"), Some(&Value::Integer(0)));
    assert!(action.get("B").is_none(), "缺省 /B 不物化");

    let features: Vec<PdfFeature> = doc.features.borrow().iter().copied().collect();
    assert!(features.contains(&PdfFeature::ThreadAction));
}

#[test]
fn thread_action_rejects_unknown_thread_or_bead() {
    let cases = [
        // 线程索引越界。
        (Vec::new(), ThreadAction::new(0)),
        // bead 索引越界。
        (
            vec![vec![bead(0, [0.0, 0.0, 1.0, 1.0])]],
            ThreadAction::new(0).with_bead(1),
        ),
    ];
    for (threads, action) in cases {
        let mut out = Vec::new();
        let mut b = PdfBuilder::with_version(PdfVersion::V1_7);
        let p0 = b.page(PageSize::A4);
        for beads in threads {
            b.add_article_thread(ArticleThread::new(beads));
        }
        b.add_annotation(p0, link_with_action(Action::Thread(action.clone())));
        let error = b
            .build(&mut out)
            .expect_err("unresolved thread action indices");
        assert!(
            matches!(error, WriteError::InvalidObject(_)),
            "{action:?}: {error}"
        );
        assert_eq!(out.len(), 0, "{action:?}");
    }
}

#[test]
fn thread_action_requires_pdf_1_1() {
    let mut out = Vec::new();
    let mut b = PdfBuilder::with_version(PdfVersion::V1_0);
    let p0 = b.page(PageSize::A4);
    b.add_article_thread(ArticleThread::new(vec![bead(0, [0.0, 0.0, 1.0, 1.0])]));
    b.add_annotation(p0, link_with_action(Action::Thread(ThreadAction::new(0))));
    let error = b.build(&mut out).expect_err("Thread action needs PDF 1.1");
    assert!(matches!(error, WriteError::UnsupportedVersion(_)));
    assert!(format!("{error}").contains("thread action"), "{error}");
    assert_eq!(out.len(), 0);
}
