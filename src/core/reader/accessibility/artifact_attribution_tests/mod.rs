//! Artifact 归属诊断的单元测试（ISO 32000-1 14.8.2.2.1、14.8.2.3.1）。
//!
//! 按主题目录化拆分（与 `builder/tests/shading/` 的目录化形态一致）：
//! - 本文件保留头部文档与跨主题共享夹具（对象号常量、结构树/页面/MCR/OBJR/注解
//!   字典构造器与发现期望值辅助）；
//! - `shall_rules`：规则 1/2 的 shall 级诊断（Artifact 被赋结构身份、artifact
//!   进入逻辑结构，含父树 `/MCID` 定位）；
//! - `mcr_page_coverage`：MCR 的 `/Stm`/`/StmOwn` 与元素/MCR `/Pg` 覆盖归属；
//! - `tag_suspects`：14.8.2.3.1 tag suspects 两条 shall 规则与子树豁免；
//! - `annotation_rules`：14.8.2.2.1 注解条件（规则 6/7）及其干净、降级与越界路径；
//! - `exemptions_and_boundaries`：真实内容规则的继承豁免、非法 `/MCID` 形态与
//!   越界页错误；
//! - `output_order`：七段发现的固定输出顺序。
//!
//! 断言、夹具与注释自原单文件逐行搬运，测试叶名零改动。

use crate::core::reader::accessibility::{
    ArtifactAttributionError, ArtifactAttributionRule, ArtifactAttributionViolation,
};
use crate::core::types::{Array, Dict, Document, Object, StreamData, Trailer, Value};

const CATALOG: i64 = 1;
const PAGES: i64 = 2;
const PAGE: i64 = 3;
const ROOT: i64 = 4;
const DOC: i64 = 5;
const CHILD: i64 = 6;
const STM: i64 = 7;
const PAGE2: i64 = 8;
const ANNOT: i64 = 9;
const ANNOT2: i64 = 10;

fn name(value: &str) -> Value {
    Value::Name(value.to_owned())
}

fn arr(values: Vec<Value>) -> Value {
    Value::Array(Array(values))
}

fn insert(doc: &Document, num: i64, value: Value) {
    doc.set_object(Object::new(num, 0, value));
}

fn element(structure_type: &str, parent: i64) -> Dict {
    Dict::new()
        .insert("Type", name("StructElem"))
        .insert("S", name(structure_type))
        .insert("P", Value::Ref(parent, 0))
}

/// 挂在本页的结构元素（带 `/Pg`，规则 2 只看这类元素）。
fn page_element(structure_type: &str, parent: i64) -> Dict {
    element(structure_type, parent).insert("Pg", Value::Ref(PAGE, 0))
}

fn struct_root(entries: Dict) -> Dict {
    entries.insert("Type", name("StructTreeRoot"))
}

/// 只带 `/K` 顶层子项的结构树根。
fn root_with(kids: Vec<Value>) -> Dict {
    struct_root(Dict::new().insert("K", arr(kids)))
}

fn mcr(stream_key: Option<&str>, mc_id: i64) -> Value {
    let mut dict = Dict::new()
        .insert("Type", name("MCR"))
        .insert("Pg", Value::Ref(PAGE, 0))
        .insert("MCID", Value::Integer(mc_id));
    if let Some(key) = stream_key {
        dict = dict.insert(key, Value::Ref(STM, 0));
    }
    Value::Dict(dict)
}

/// 单页文档：页 0 即对象 3，带 `/StructParents 0` 与内容流。
fn document(content: &[u8], root: Dict) -> Document {
    let mut doc = Document::new();
    insert(&doc, ROOT, Value::Dict(root));
    insert(
        &doc,
        PAGES,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Pages"))
                .insert("Kids", arr(vec![Value::Ref(PAGE, 0)]))
                .insert("Count", Value::Integer(1)),
        ),
    );
    insert(
        &doc,
        PAGE,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Page"))
                .insert("Parent", Value::Ref(PAGES, 0))
                .insert("StructParents", Value::Integer(0))
                .insert(
                    "Contents",
                    Value::Stream(Dict::new(), StreamData::Owned(content.to_vec())),
                ),
        ),
    );
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

fn violations(doc: &Document, page_index: usize) -> Vec<ArtifactAttributionViolation> {
    doc.artifact_attribution_violations(page_index)
        .unwrap_or_else(|error| panic!("页 {page_index} 的归属诊断应成功: {error}"))
}

fn violation(
    section: Option<usize>,
    element: Option<usize>,
    kid: Option<usize>,
    rule: ArtifactAttributionRule,
) -> ArtifactAttributionViolation {
    ArtifactAttributionViolation {
        page: 0,
        section,
        element,
        kid,
        annotation: None,
        rule,
    }
}

/// 与页/层级无关的发现（tag suspect 两条规则只带 `section`）。
fn violation_on(
    page: usize,
    section: Option<usize>,
    rule: ArtifactAttributionRule,
) -> ArtifactAttributionViolation {
    ArtifactAttributionViolation {
        page,
        section,
        element: None,
        kid: None,
        annotation: None,
        rule,
    }
}

/// 规则 6：只带本页 `/Annots` 下标的可见注解未入结构发现。
fn visible_annotation_flagged(annotation: usize) -> ArtifactAttributionViolation {
    ArtifactAttributionViolation {
        page: 0,
        section: None,
        element: None,
        kid: None,
        annotation: Some(annotation),
        rule: ArtifactAttributionRule::VisibleAnnotationNotInStructure,
    }
}

/// 规则 7：OBJR 声称元素与父树/`/StructParent` 链接不符的发现。
fn objr_parent_tree_mismatch(
    element: usize,
    kid: usize,
    annotation: usize,
) -> ArtifactAttributionViolation {
    ArtifactAttributionViolation {
        page: 0,
        section: None,
        element: Some(element),
        kid: Some(kid),
        annotation: Some(annotation),
        rule: ArtifactAttributionRule::AnnotationObjrParentTreeMismatch,
    }
}

/// 把目录 `/MarkInfo` 覆盖为给定值（Table 321）。
fn set_mark_info(doc: &Document, info: Value) {
    let catalog = doc
        .get_object(CATALOG)
        .expect("目录应可读")
        .expect("目录对象应存在")
        .value
        .as_dict()
        .cloned()
        .expect("目录应为字典");
    doc.set_object(Object::new(
        CATALOG,
        0,
        Value::Dict(catalog.insert("MarkInfo", info)),
    ));
}

/// 声明文档含 tag suspects：`/MarkInfo << /Suspects true >>`。
fn with_suspects(doc: &Document) {
    set_mark_info(
        doc,
        Value::Dict(Dict::new().insert("Suspects", Value::Bool(true))),
    );
}

/// 把 `refs` 挂到 fixture 页的 `/Annots` 数组（对象本身由调用方插入）。
fn attach_annotations(doc: &Document, refs: Vec<Value>) {
    let page = doc
        .get_object(PAGE)
        .expect("页面应可读")
        .expect("页面对象应存在")
        .value
        .as_dict()
        .cloned()
        .expect("页面应为字典");
    doc.set_object(Object::new(
        PAGE,
        0,
        Value::Dict(page.insert("Annots", arr(refs))),
    ));
}

/// 带 `/AP /N` 常态外观的 Text 注解字典（14.8.2.2.1 前两个可见条件）。
fn normal_appearance_annotation() -> Dict {
    Dict::new()
        .insert("Type", name("Annot"))
        .insert("Subtype", name("Text"))
        .insert(
            "Rect",
            arr(vec![
                Value::Integer(0),
                Value::Integer(0),
                Value::Integer(10),
                Value::Integer(10),
            ]),
        )
        .insert(
            "AP",
            Value::Dict(Dict::new().insert("N", Value::Ref(STM, 0))),
        )
}

/// 没有 `/AP` 外观流的 Text 注解字典。
fn appearanceless_annotation() -> Dict {
    Dict::new()
        .insert("Type", name("Annot"))
        .insert("Subtype", name("Text"))
        .insert(
            "Rect",
            arr(vec![
                Value::Integer(0),
                Value::Integer(0),
                Value::Integer(10),
                Value::Integer(10),
            ]),
        )
}

/// OBJR 对象引用字典（Table 325）：`/Pg` 本页、`/Obj` 给定间接引用。
fn objr(object: i64) -> Value {
    Value::Dict(
        Dict::new()
            .insert("Type", name("OBJR"))
            .insert("Pg", Value::Ref(PAGE, 0))
            .insert("Obj", Value::Ref(object, 0)),
    )
}

/// 单页文档（内容同 `document`）但目录没有 `/StructTreeRoot`。
fn untagged_document(content: &[u8]) -> Document {
    let doc = document(content, struct_root(Dict::new()));
    doc.set_object(Object::new(
        CATALOG,
        0,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Catalog"))
                .insert("Pages", Value::Ref(PAGES, 0)),
        ),
    ));
    doc
}

/// 单页字典：`/StructParents 0` 加给定内容流。
fn page_dict(content: &[u8]) -> Value {
    Value::Dict(
        Dict::new()
            .insert("Type", name("Page"))
            .insert("Parent", Value::Ref(PAGES, 0))
            .insert("StructParents", Value::Integer(0))
            .insert(
                "Contents",
                Value::Stream(Dict::new(), StreamData::Owned(content.to_vec())),
            ),
    )
}

/// 两页文档：页 0 为对象 3、页 1 为对象 8，各自带内容流，结构树为空根。
fn two_page_document(first: &[u8], second: &[u8]) -> Document {
    let mut doc = Document::new();
    insert(&doc, ROOT, Value::Dict(struct_root(Dict::new())));
    insert(
        &doc,
        PAGES,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Pages"))
                .insert("Kids", arr(vec![Value::Ref(PAGE, 0), Value::Ref(PAGE2, 0)]))
                .insert("Count", Value::Integer(2)),
        ),
    );
    insert(&doc, PAGE, page_dict(first));
    insert(&doc, PAGE2, page_dict(second));
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

mod annotation_rules;
mod exemptions_and_boundaries;
mod mcr_page_coverage;
mod output_order;
mod shall_rules;
mod tag_suspects;
