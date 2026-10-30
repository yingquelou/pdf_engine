//! 文章线读端测试：`/Threads`、线程字典与 bead 双向链。

use crate::core::types::{Array, Dict, Object, Trailer, Value};

use super::*;

const PAGES_OBJECT: i64 = 2;
const PAGE_BASE: i64 = 10;
const THREAD_OBJECT: i64 = 30;
const FIRST_BEAD: i64 = 31;
const SECOND_BEAD: i64 = 32;

fn page_ref(index: usize) -> Value {
    Value::Ref(PAGE_BASE + index as i64, 0)
}

fn insert(doc: &Document, num: i64, value: Value) {
    doc.set_object(Object::new(num, 0, value));
}

fn rect(values: [f64; 4]) -> Value {
    Value::Array(Array(values.iter().map(|v| Value::Float(*v)).collect()))
}

fn bead(thread: Option<i64>, prev: i64, next: i64, page: usize) -> Dict {
    let mut dict = Dict::new()
        .insert("Type", Value::Name("Bead".into()))
        .insert("N", Value::Ref(next, 0))
        .insert("V", Value::Ref(prev, 0))
        .insert("P", page_ref(page))
        .insert("R", rect([1.0, 2.0, 3.0, 4.0]));
    if let Some(thread) = thread {
        dict = dict.insert("T", Value::Ref(thread, 0));
    }
    dict
}

/// 含 `page_count` 个页面的文档；`catalog` 条目并入目录。
fn document_with_pages(page_count: usize, catalog: Dict) -> Document {
    let mut doc = Document::new();
    let catalog = catalog
        .insert("Type", Value::Name("Catalog".into()))
        .insert("Pages", Value::Ref(PAGES_OBJECT, 0));
    let mut kids = Array::new();
    for index in 0..page_count {
        let num = PAGE_BASE + index as i64;
        kids = kids.push(Value::Ref(num, 0));
        insert(
            &doc,
            num,
            Value::Dict(
                Dict::new()
                    .insert("Type", Value::Name("Page".into()))
                    .insert("Parent", Value::Ref(PAGES_OBJECT, 0)),
            ),
        );
    }
    insert(
        &doc,
        PAGES_OBJECT,
        Value::Dict(
            Dict::new()
                .insert("Type", Value::Name("Pages".into()))
                .insert("Kids", Value::Array(kids))
                .insert("Count", Value::Integer(page_count as i64)),
        ),
    );
    insert(&doc, 1, Value::Dict(catalog));
    doc.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(1, 0)),
    };
    doc
}

fn threads_catalog() -> Dict {
    Dict::new().insert(
        "Threads",
        Value::Array(Array(vec![Value::Ref(THREAD_OBJECT, 0)])),
    )
}

fn thread_dict(first: i64, info: bool) -> Dict {
    let mut dict = Dict::new()
        .insert("Type", Value::Name("Thread".into()))
        .insert("F", Value::Ref(first, 0));
    if info {
        dict = dict.insert(
            "I",
            Value::Dict(Dict::new().insert("Title", Value::String("Man Bites Dog".into()))),
        );
    }
    dict
}

fn two_bead_document() -> Document {
    let doc = document_with_pages(3, threads_catalog());
    insert(
        &doc,
        THREAD_OBJECT,
        Value::Dict(thread_dict(FIRST_BEAD, true)),
    );
    insert(
        &doc,
        FIRST_BEAD,
        Value::Dict(bead(Some(THREAD_OBJECT), SECOND_BEAD, SECOND_BEAD, 0)),
    );
    insert(
        &doc,
        SECOND_BEAD,
        Value::Dict(bead(None, FIRST_BEAD, FIRST_BEAD, 2)),
    );
    doc
}

#[test]
fn article_threads_follow_the_next_chain() {
    let doc = two_bead_document();
    let threads = doc.article_threads().expect("threads");
    assert_eq!(threads.len(), 1);
    let thread = &threads[0];
    assert_eq!(thread.beads.len(), 2);
    assert_eq!(thread.beads[0].page, 0);
    assert_eq!(thread.beads[0].rect, [1.0, 2.0, 3.0, 4.0]);
    assert_eq!(thread.beads[1].page, 2);
    assert_eq!(thread.beads[1].rect, [1.0, 2.0, 3.0, 4.0]);
    let info = thread.info.as_ref().expect("thread /I");
    assert_eq!(
        info.get("Title"),
        Some(&Value::String("Man Bites Dog".into()))
    );
}

#[test]
fn missing_threads_reads_as_an_empty_list() {
    let doc = document_with_pages(1, Dict::new());
    assert!(doc.article_threads().expect("threads").is_empty());
}

#[test]
fn broken_bead_chains_are_rejected() {
    // /N 缺失。
    let doc = document_with_pages(3, threads_catalog());
    insert(
        &doc,
        THREAD_OBJECT,
        Value::Dict(thread_dict(FIRST_BEAD, false)),
    );
    let mut missing_next = bead(Some(THREAD_OBJECT), FIRST_BEAD, SECOND_BEAD, 0);
    missing_next.0.remove("N");
    insert(&doc, FIRST_BEAD, Value::Dict(missing_next));
    assert!(
        matches!(doc.article_threads(), Err(NavigationError::Malformed(_))),
        "missing /N must be rejected"
    );

    // /V 与前一个 bead 不一致。
    let doc = two_bead_document();
    insert(
        &doc,
        SECOND_BEAD,
        Value::Dict(bead(None, SECOND_BEAD, FIRST_BEAD, 2)),
    );
    let outcome = doc.article_threads();
    assert!(
        matches!(&outcome, Err(NavigationError::Malformed(message)) if message.contains("/V")),
        "mismatched /V must be rejected: {outcome:?}"
    );

    // 首个 bead 缺少 /T（Table 161 要求首 bead 必须声明）。
    let doc = two_bead_document();
    insert(
        &doc,
        FIRST_BEAD,
        Value::Dict(bead(None, SECOND_BEAD, SECOND_BEAD, 0)),
    );
    let outcome = doc.article_threads();
    assert!(
        matches!(&outcome, Err(NavigationError::Malformed(message)) if message.contains("/T")),
        "first bead without /T must be rejected: {outcome:?}"
    );

    // /N 在链中形成子环。
    let doc = two_bead_document();
    insert(
        &doc,
        SECOND_BEAD,
        Value::Dict(bead(None, FIRST_BEAD, SECOND_BEAD, 2)),
    );
    let outcome = doc.article_threads();
    assert!(
        matches!(&outcome, Err(NavigationError::Malformed(message)) if message.contains("revisits")),
        "bead sub-cycle must be rejected: {outcome:?}"
    );
}

#[test]
fn bead_thread_ownership_and_page_references_are_validated() {
    // /T 指向另一条线程。
    let doc = two_bead_document();
    insert(
        &doc,
        SECOND_BEAD,
        Value::Dict(bead(Some(THREAD_OBJECT + 1), FIRST_BEAD, FIRST_BEAD, 2)),
    );
    let outcome = doc.article_threads();
    assert!(
        matches!(&outcome, Err(NavigationError::Malformed(message)) if message.contains("thread")),
        "foreign /T must be rejected: {outcome:?}"
    );

    // /P 不在页面树中。
    let doc = two_bead_document();
    insert(
        &doc,
        SECOND_BEAD,
        Value::Dict(bead(None, FIRST_BEAD, FIRST_BEAD, 9)),
    );
    assert!(matches!(
        doc.article_threads(),
        Err(NavigationError::PageNotFound)
    ));

    // /R 不是四个数字。
    let doc = two_bead_document();
    insert(
        &doc,
        SECOND_BEAD,
        Value::Dict(
            bead(None, FIRST_BEAD, FIRST_BEAD, 2)
                .insert("R", Value::Array(Array(vec![Value::Float(1.0)]))),
        ),
    );
    assert!(matches!(
        doc.article_threads(),
        Err(NavigationError::Malformed(_))
    ));
}

#[test]
fn threads_entry_must_be_an_array() {
    let doc = document_with_pages(1, Dict::new().insert("Threads", Value::Integer(3)));
    assert!(matches!(
        doc.article_threads(),
        Err(NavigationError::Malformed(_))
    ));
}
