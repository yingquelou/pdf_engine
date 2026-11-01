//! 共享夹具：最小目录/页面树/结构树与断言辅助。

use crate::core::reader::accessibility::{AnnotationOrderBasis, ContentOrderItem};
use crate::core::types::{Array, Dict, Document, Object, StreamData, Trailer, Value};

pub(super) const CATALOG: i64 = 1;
pub(super) const PAGES: i64 = 2;
pub(super) const PAGE: i64 = 3;
pub(super) const ROOT: i64 = 4;
pub(super) const DOC: i64 = 5;
pub(super) const FIRST: i64 = 6;
pub(super) const SECOND: i64 = 7;
pub(super) const PAGE2: i64 = 8;
pub(super) const ANNOT: i64 = 9;
pub(super) const ANNOT2: i64 = 10;
pub(super) const THIRD: i64 = 12;

pub(super) fn name(value: &str) -> Value {
    Value::Name(value.to_owned())
}

pub(super) fn arr(values: Vec<Value>) -> Value {
    Value::Array(Array(values))
}

pub(super) fn insert(doc: &Document, num: i64, value: Value) {
    doc.set_object(Object::new(num, 0, value));
}

/// 顶层结构元素（`/P` 指向结构树根对象）。
pub(super) fn root_element(structure_type: &str, kids: Vec<Value>) -> Dict {
    Dict::new()
        .insert("Type", name("StructElem"))
        .insert("S", name(structure_type))
        .insert("P", Value::Ref(ROOT, 0))
        .insert("K", arr(kids))
}

/// 挂在 `parent` 下、生效页为本页（对象 3）的结构元素。
pub(super) fn element(structure_type: &str, parent: i64, kids: Vec<Value>) -> Dict {
    root_element(structure_type, kids)
        .insert("P", Value::Ref(parent, 0))
        .insert("Pg", Value::Ref(PAGE, 0))
}

/// 生效页由 `page` 指定的结构元素。
pub(super) fn element_on(structure_type: &str, parent: i64, page: i64, kids: Vec<Value>) -> Dict {
    root_element(structure_type, kids)
        .insert("P", Value::Ref(parent, 0))
        .insert("Pg", Value::Ref(page, 0))
}

/// 没有 `/Pg` 的结构元素（生效页靠父链或直接嵌入的 OBJR/MCID 落地）。
pub(super) fn element_unpaged(structure_type: &str, parent: i64, kids: Vec<Value>) -> Dict {
    root_element(structure_type, kids).insert("P", Value::Ref(parent, 0))
}

/// 指向本页的 OBJR（Table 325）。
pub(super) fn objr(annotation: i64) -> Value {
    Value::Dict(
        Dict::new()
            .insert("Type", name("OBJR"))
            .insert("Obj", Value::Ref(annotation, 0)),
    )
}

/// 带显式 `/Pg` 的 OBJR。
pub(super) fn objr_on(page: i64, annotation: i64) -> Value {
    Value::Dict(
        Dict::new()
            .insert("Type", name("OBJR"))
            .insert("Pg", Value::Ref(page, 0))
            .insert("Obj", Value::Ref(annotation, 0)),
    )
}

/// 指向本页的 MCR（Table 324）。
pub(super) fn mcr(mcid: i64) -> Value {
    Value::Dict(
        Dict::new()
            .insert("Type", name("MCR"))
            .insert("Pg", Value::Ref(PAGE, 0))
            .insert("MCID", Value::Integer(mcid)),
    )
}

/// 不带 `/Pg` 的 MCR：生效页按 Table 324 沿用元素的声明。
pub(super) fn mcr_unpaged(mcid: i64) -> Value {
    Value::Dict(
        Dict::new()
            .insert("Type", name("MCR"))
            .insert("MCID", Value::Integer(mcid)),
    )
}

/// 带 `/Stm` 的 MCR：序列不在页面流里。
pub(super) fn mcr_in_stream(mcid: i64) -> Value {
    let dict = mcr(mcid)
        .as_dict()
        .unwrap()
        .clone()
        .insert("Stm", Value::Ref(120, 0));
    Value::Dict(dict)
}

/// 只有 `/Type /Annot` 与 `/Rect` 的最小注解字典。
pub(super) fn annotation_dict() -> Dict {
    Dict::new()
        .insert("Type", name("Annot"))
        .insert("Subtype", name("Text"))
        .insert(
            "Rect",
            arr(vec![
                Value::Integer(72),
                Value::Integer(660),
                Value::Integer(92),
                Value::Integer(680),
            ]),
        )
}

/// `mcids` 各生成一段 `/T << /MCID n >> BDC (…) Tj EMC`。
pub(super) fn content(mcids: &[i64]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for mcid in mcids {
        bytes.extend_from_slice(format!("/T << /MCID {mcid} >> BDC ").as_bytes());
        bytes.extend_from_slice(b"BT (x) Tj EMC\n");
    }
    bytes
}

/// 只带 `/K` 顶层子项的结构树根。
pub(super) fn tree(kids: Vec<Value>) -> Dict {
    Dict::new()
        .insert("Type", name("StructTreeRoot"))
        .insert("K", arr(kids))
}

pub(super) fn page_dict(annots: &[i64], stream: &[u8]) -> Dict {
    let mut dict = Dict::new()
        .insert("Type", name("Page"))
        .insert("Parent", Value::Ref(PAGES, 0))
        .insert(
            "Contents",
            Value::Stream(Dict::new(), StreamData::Owned(stream.to_vec())),
        );
    if !annots.is_empty() {
        dict = dict.insert(
            "Annots",
            arr(annots.iter().map(|number| Value::Ref(*number, 0)).collect()),
        );
    }
    dict
}

/// 单页文档：页 0 即对象 3，`root` 为 `None` 时目录不写 `/StructTreeRoot`。
pub(super) fn document(root: Option<Dict>, content: &[u8], annots: &[i64]) -> Document {
    two_page(root, content, annots, &[], &[])
}

/// 双页文档：页 0 = 对象 3、页 1 = 对象 8；注解对象按需注册。
pub(super) fn two_page(
    root: Option<Dict>,
    content0: &[u8],
    annots0: &[i64],
    content1: &[u8],
    annots1: &[i64],
) -> Document {
    let has_tree = root.is_some();
    let mut doc = Document::new();
    if let Some(root) = root {
        insert(&doc, ROOT, Value::Dict(root));
    }
    insert(
        &doc,
        PAGES,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Pages"))
                .insert("Count", Value::Integer(2))
                .insert("Kids", arr(vec![Value::Ref(PAGE, 0), Value::Ref(PAGE2, 0)])),
        ),
    );
    insert(&doc, PAGE, Value::Dict(page_dict(annots0, content0)));
    insert(&doc, PAGE2, Value::Dict(page_dict(annots1, content1)));
    for number in annots0.iter().chain(annots1) {
        insert(&doc, *number, Value::Dict(annotation_dict()));
    }
    insert(
        &doc,
        CATALOG,
        Value::Dict(
            Dict::new()
                .insert("Type", name("Catalog"))
                .insert("Pages", Value::Ref(PAGES, 0))
                .insert_opt("StructTreeRoot", has_tree.then_some(Value::Ref(ROOT, 0))),
        ),
    );
    insert(
        &doc,
        120,
        Value::Dict(Dict::new().insert("Type", name("XObject"))),
    );
    doc.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(CATALOG, 0)),
    };
    doc
}

/// 结构树根的辅助：`Dict` 没有条件插入，这里用扩展补齐。
trait InsertOpt {
    fn insert_opt(self, key: &str, value: Option<Value>) -> Self;
}

impl InsertOpt for Dict {
    fn insert_opt(self, key: &str, value: Option<Value>) -> Self {
        match value {
            Some(value) => self.insert(key, value),
            None => self,
        }
    }
}

/// 把序列压成 `(区段/注解)` 简写，便于断言整体顺序。
pub(super) fn shape(items: &[ContentOrderItem]) -> Vec<String> {
    items
        .iter()
        .map(|item| match item {
            ContentOrderItem::MarkedContent(position) => format!("MC{}", position.section),
            ContentOrderItem::Annotation(position) => format!("A{}", position.annotation),
        })
        .collect()
}

pub(super) fn order(doc: &Document, page: usize) -> Vec<ContentOrderItem> {
    doc.annotation_content_order(page)
        .unwrap_or_else(|error| panic!("页 {page} 的内容序列应成功: {error}"))
}

/// 取序列里注解条目的依据（区段条目忽略）。
pub(super) fn bases(
    items: &[ContentOrderItem],
) -> Vec<(usize, AnnotationOrderBasis, Option<usize>)> {
    items
        .iter()
        .filter_map(|item| match item {
            ContentOrderItem::MarkedContent(_) => None,
            ContentOrderItem::Annotation(position) => Some((
                position.annotation,
                position.basis,
                position.associated_sequence,
            )),
        })
        .collect()
}

/// 单页 + `Document` 根元素（子项为 `kids`）+ 自定义内容流与 `/Annots`。
pub(super) fn one_page_document(kids: Vec<Value>, stream: &[u8], annots: &[i64]) -> Document {
    let doc = document(Some(tree(vec![Value::Ref(DOC, 0)])), stream, annots);
    insert(&doc, DOC, Value::Dict(root_element("Document", kids)));
    doc
}

/// 单页 + 两段标记内容（MCID 0、1）+ 一个注解的常用夹具。
pub(super) fn two_sequences_and_one_annotation(kids: Vec<Value>) -> Document {
    one_page_document(kids, &content(&[0, 1]), &[ANNOT])
}
