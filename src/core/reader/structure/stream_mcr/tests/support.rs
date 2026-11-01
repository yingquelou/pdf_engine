//! 测试辅助函数与常量。

pub(super) use super::super::super::tests::{arr, element, insert, name, struct_root};
pub use super::super::{MarkedContentParent, StreamMcrDiagnostic, StreamMcrRule};
use crate::core::types::{Dict, Document, StreamData, Trailer, Value};

pub const CATALOG: i64 = 1;
pub const ROOT: i64 = 2;
pub const PAGES: i64 = 3;
pub const PAGE: i64 = 4;
pub const ELEMENT: i64 = 5;
pub const OTHER_ELEMENT: i64 = 6;
pub const PAGE_TWO: i64 = 8;
pub const STREAM: i64 = 11;
pub const OWNER: i64 = 12;
pub const OTHER_STREAM: i64 = 13;
pub const OBJR_TARGET: i64 = 14;
/// 容器 `/StructParents` 与父树条目统一使用的键。
pub const KEY: i64 = 7;

/// 页面字典（`/Type /Page`，`/Parent` 由装配时补齐）。
pub fn page(entries: Dict) -> Dict {
    entries.insert("Type", name("Page"))
}

/// 顶层 `Figure` 元素：可选 `/Pg` 与 `/K` 子项。
pub fn figure(page: Option<i64>, kids: Vec<Value>) -> Dict {
    let dict = element("Figure", ROOT);
    let dict = match page {
        Some(page) => dict.insert("Pg", Value::Ref(page, 0)),
        None => dict,
    };
    if kids.is_empty() {
        dict
    } else {
        dict.insert("K", arr(kids))
    }
}

/// MCR 字典（Table 324）：按需给出 `/Pg`、`/Stm`、`/StmOwn` 与 `/MCID`。
pub fn mcr(page: Option<i64>, stream: Option<i64>, owner: Option<i64>, mcid: i64) -> Value {
    let mut dict = Dict::new()
        .insert("Type", name("MCR"))
        .insert("MCID", Value::Integer(mcid));
    if let Some(page) = page {
        dict = dict.insert("Pg", Value::Ref(page, 0));
    }
    if let Some(stream) = stream {
        dict = dict.insert("Stm", Value::Ref(stream, 0));
    }
    if let Some(owner) = owner {
        dict = dict.insert("StmOwn", Value::Ref(owner, 0));
    }
    Value::Dict(dict)
}

/// OBJR 字典（Table 325）。
pub fn objr(page: Option<i64>, object: i64) -> Value {
    let mut dict = Dict::new()
        .insert("Type", name("OBJR"))
        .insert("Obj", Value::Ref(object, 0));
    if let Some(page) = page {
        dict = dict.insert("Pg", Value::Ref(page, 0));
    }
    Value::Dict(dict)
}

/// `/Subtype /Form` 内容流（`/Stm` 的常见目标形态）。
pub fn form_stream(entries: Dict) -> Value {
    let dict = entries
        .insert("Type", name("XObject"))
        .insert("Subtype", name("Form"));
    Value::Stream(dict, StreamData::Owned(b"q Q".to_vec()))
}

/// 带 `/StructParents` 键的 `/Stm` 流（写端无所有者时的形态）。
pub fn keyed_stream() -> Value {
    form_stream(Dict::new().insert("StructParents", Value::Integer(KEY)))
}

/// 注解字典：`/AP /N` 指向 `appearance` 对象，可另带字典条目。
pub fn annotation(appearance: i64, entries: Dict) -> Value {
    Value::Dict(
        entries
            .insert("Type", name("Annot"))
            .insert("Subtype", name("Text"))
            .insert(
                "AP",
                Value::Dict(Dict::new().insert("N", Value::Ref(appearance, 0))),
            ),
    )
}

/// `/ParentTree /Nums` 里键 `KEY` 的数组条目。
pub fn key_entry(slots: Vec<Value>) -> Vec<Value> {
    vec![Value::Integer(KEY), arr(slots)]
}

/// 装配文档：页树、结构根（`/K` = 元素引用、`/ParentTree /Nums` = `nums`）与
/// 额外对象（流、注解、OBJR 目标）。
pub fn document_with(
    pages: Vec<(i64, Dict)>,
    elements: Vec<(i64, Dict)>,
    nums: Vec<Value>,
    objects: Vec<(i64, Value)>,
) -> Document {
    let mut doc = Document::new();
    let page_refs: Vec<Value> = pages
        .iter()
        .map(|(number, _)| Value::Ref(*number, 0))
        .collect();
    for (number, entries) in &pages {
        insert(
            &doc,
            *number,
            Value::Dict(entries.clone().insert("Parent", Value::Ref(PAGES, 0))),
        );
    }
    insert(
        &doc,
        PAGES,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Pages"))
                .insert("Kids", arr(page_refs))
                .insert("Count", Value::Integer(pages.len() as i64)),
        ),
    );
    let element_refs: Vec<Value> = elements
        .iter()
        .map(|(number, _)| Value::Ref(*number, 0))
        .collect();
    for (number, entries) in &elements {
        insert(&doc, *number, Value::Dict(entries.clone()));
    }
    let root = struct_root(Dict::new().insert("K", arr(element_refs))).insert(
        "ParentTree",
        Value::Dict(Dict::new().insert("Nums", arr(nums))),
    );
    insert(&doc, ROOT, Value::Dict(root));
    for (number, value) in objects {
        insert(&doc, number, value);
    }
    insert(
        &doc,
        CATALOG,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Catalog"))
                .insert("Pages", Value::Ref(PAGES, 0))
                .insert("StructTreeRoot", Value::Ref(ROOT, 0)),
        ),
    );
    doc.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(CATALOG, 0)),
    };
    doc
}

/// 单页 + 单元素 + `/Stm` 容器（`/MCID` 为 `mcid`）+ 父树 `nums` 的文档。
pub fn stream_document(mcid: i64, container: Value, nums: Vec<Value>) -> Document {
    document_with(
        vec![(PAGE, page(Dict::new()))],
        vec![(
            ELEMENT,
            figure(Some(PAGE), vec![mcr(Some(PAGE), Some(STREAM), None, mcid)]),
        )],
        nums,
        vec![(STREAM, container)],
    )
}

/// 正路文档：容器带键、父树数组的 MCID 0 槽位指向唯一元素。
pub fn positive_document(mcid: i64, container: Value) -> Document {
    stream_document(mcid, container, key_entry(vec![Value::Ref(ELEMENT, 0)]))
}

pub fn rules(parent: &MarkedContentParent) -> Vec<StreamMcrRule> {
    parent
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.rule)
        .collect()
}

pub fn first(doc: &Document, element_index: usize, kid_index: usize) -> MarkedContentParent {
    doc.marked_content_parent(element_index, kid_index)
        .unwrap_or_else(|error| panic!("element {element_index} kid {kid_index}: {error}"))
        .expect("the kid is an MCR")
}

pub fn sweep(doc: &Document) -> Vec<StreamMcrDiagnostic> {
    doc.stream_mcr_diagnostic().expect("stream mcr diagnostic")
}
