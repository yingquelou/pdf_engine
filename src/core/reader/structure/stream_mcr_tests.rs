//! 流形态 MCR 父树定位与 Table 326 容器层互斥的单元测试
//! （ISO 32000-1 14.7.4.2、14.7.4.4）。

use super::tests::{arr, element, insert, name, struct_root};
use super::{
    MarkedContentContainer, MarkedContentParent, StreamMcrDiagnostic, StreamMcrRule, StructureError,
};
use crate::core::types::{Dict, Document, StreamData, Trailer, Value};

const CATALOG: i64 = 1;
const ROOT: i64 = 2;
const PAGES: i64 = 3;
const PAGE: i64 = 4;
const ELEMENT: i64 = 5;
const OTHER_ELEMENT: i64 = 6;
const PAGE_TWO: i64 = 8;
const STREAM: i64 = 11;
const OWNER: i64 = 12;
const OTHER_STREAM: i64 = 13;
const OBJR_TARGET: i64 = 14;
/// 容器 `/StructParents` 与父树条目统一使用的键。
const KEY: i64 = 7;

/// 页面字典（`/Type /Page`，`/Parent` 由装配时补齐）。
fn page(entries: Dict) -> Dict {
    entries.insert("Type", name("Page"))
}

/// 顶层 `Figure` 元素：可选 `/Pg` 与 `/K` 子项。
fn figure(page: Option<i64>, kids: Vec<Value>) -> Dict {
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
fn mcr(page: Option<i64>, stream: Option<i64>, owner: Option<i64>, mcid: i64) -> Value {
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
fn objr(page: Option<i64>, object: i64) -> Value {
    let mut dict = Dict::new()
        .insert("Type", name("OBJR"))
        .insert("Obj", Value::Ref(object, 0));
    if let Some(page) = page {
        dict = dict.insert("Pg", Value::Ref(page, 0));
    }
    Value::Dict(dict)
}

/// `/Subtype /Form` 内容流（`/Stm` 的常见目标形态）。
fn form_stream(entries: Dict) -> Value {
    let dict = entries
        .insert("Type", name("XObject"))
        .insert("Subtype", name("Form"));
    Value::Stream(dict, StreamData::Owned(b"q Q".to_vec()))
}

/// 带 `/StructParents` 键的 `/Stm` 流（写端无所有者时的形态）。
fn keyed_stream() -> Value {
    form_stream(Dict::new().insert("StructParents", Value::Integer(KEY)))
}

/// 注解字典：`/AP /N` 指向 `appearance` 对象，可另带字典条目。
fn annotation(appearance: i64, entries: Dict) -> Value {
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
fn key_entry(slots: Vec<Value>) -> Vec<Value> {
    vec![Value::Integer(KEY), arr(slots)]
}

/// 装配文档：页树、结构根（`/K` = 元素引用、`/ParentTree /Nums` = `nums`）与
/// 额外对象（流、注解、OBJR 目标）。
fn document_with(
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
fn stream_document(mcid: i64, container: Value, nums: Vec<Value>) -> Document {
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
fn positive_document(mcid: i64, container: Value) -> Document {
    stream_document(mcid, container, key_entry(vec![Value::Ref(ELEMENT, 0)]))
}

fn rules(parent: &MarkedContentParent) -> Vec<StreamMcrRule> {
    parent
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.rule)
        .collect()
}

fn first(doc: &Document, element_index: usize, kid_index: usize) -> MarkedContentParent {
    doc.marked_content_parent(element_index, kid_index)
        .unwrap_or_else(|error| panic!("element {element_index} kid {kid_index}: {error}"))
        .expect("the kid is an MCR")
}

fn sweep(doc: &Document) -> Vec<StreamMcrDiagnostic> {
    doc.stream_mcr_diagnostic().expect("stream mcr diagnostic")
}

#[test]
fn stream_mcr_resolves_parent_through_container_struct_parents_key() {
    let doc = positive_document(0, keyed_stream());

    let parent = first(&doc, 0, 0);
    assert_eq!(parent.mcid, 0);
    assert_eq!(parent.stream, Some((STREAM, 0)), "/Stm 即序列所在流");
    assert_eq!(parent.container, Some((STREAM, 0)));
    assert_eq!(parent.level, MarkedContentContainer::Stream);
    assert_eq!(parent.parents_key, Some(KEY), "14.7.4.4 的键取自容器");
    assert_eq!(parent.parent, Some(0), "MCID 0 索引父树数组得到父元素");
    assert_eq!(parent.page, Some((PAGE, 0)));
    assert!(
        parent.diagnostics.is_empty(),
        "正路不应有发现：{:?}",
        parent.diagnostics
    );
    assert!(sweep(&doc).is_empty(), "写端形态不应命中互斥");
}

#[test]
fn stream_mcr_owner_annotation_supplies_container_key_and_appearance_link() {
    let doc = document_with(
        vec![(PAGE, page(Dict::new()))],
        vec![(
            ELEMENT,
            figure(
                Some(PAGE),
                vec![mcr(Some(PAGE), Some(STREAM), Some(OWNER), 0)],
            ),
        )],
        key_entry(vec![Value::Ref(ELEMENT, 0)]),
        vec![
            (STREAM, form_stream(Dict::new())),
            (
                OWNER,
                annotation(
                    STREAM,
                    Dict::new().insert("StructParents", Value::Integer(KEY)),
                ),
            ),
        ],
    );

    let parent = first(&doc, 0, 0);
    assert_eq!(parent.level, MarkedContentContainer::Owner);
    assert_eq!(
        parent.container,
        Some((OWNER, 0)),
        "写端把键标在 /StmOwn 所有者字典上"
    );
    assert_eq!(parent.parents_key, Some(KEY));
    assert_eq!(parent.parent, Some(0));
    assert_eq!(parent.stream, Some((STREAM, 0)));
    assert!(
        parent.diagnostics.is_empty(),
        "外观流与 /Stm 一致时不应有发现：{:?}",
        parent.diagnostics
    );
    assert!(sweep(&doc).is_empty());
}

#[test]
fn stream_mcr_falls_back_to_stream_key_when_owner_carries_none() {
    // 规范字面读法（键在序列所在的流上）同样能定位：比写端宽，但不误报。
    let doc = document_with(
        vec![(PAGE, page(Dict::new()))],
        vec![(
            ELEMENT,
            figure(
                Some(PAGE),
                vec![mcr(Some(PAGE), Some(STREAM), Some(OWNER), 0)],
            ),
        )],
        key_entry(vec![Value::Ref(ELEMENT, 0)]),
        vec![
            (STREAM, keyed_stream()),
            (OWNER, annotation(STREAM, Dict::new())),
        ],
    );

    let parent = first(&doc, 0, 0);
    assert_eq!(parent.level, MarkedContentContainer::Stream);
    assert_eq!(parent.container, Some((STREAM, 0)));
    assert_eq!(parent.parents_key, Some(KEY));
    assert_eq!(parent.parent, Some(0));
    assert!(
        parent.diagnostics.is_empty(),
        "回落取键不是违规：{:?}",
        parent.diagnostics
    );
}

#[test]
fn stream_mcr_rejects_owner_that_is_not_a_plain_dictionary() {
    // `/StmOwn` 指向流对象：不能作外观流的所有者（库策略）；
    // `/Stm` 又只是普通字典，不满足 Table 324 类型列的 `stream`。
    let doc = document_with(
        vec![(PAGE, page(Dict::new()))],
        vec![(
            ELEMENT,
            figure(
                Some(PAGE),
                vec![mcr(Some(PAGE), Some(STREAM), Some(OWNER), 0)],
            ),
        )],
        key_entry(vec![Value::Ref(ELEMENT, 0)]),
        vec![
            (
                STREAM,
                Value::Dict(Dict::new().insert("StructParents", Value::Integer(KEY))),
            ),
            (OWNER, form_stream(Dict::new())),
        ],
    );

    let parent = first(&doc, 0, 0);
    assert_eq!(
        rules(&parent),
        vec![
            StreamMcrRule::OwnerNotDictionary,
            StreamMcrRule::StmTargetNotStream
        ]
    );
    assert_eq!(
        parent
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.object)
            .collect::<Vec<_>>(),
        vec![Some(OWNER), Some(STREAM)]
    );
    for diagnostic in &parent.diagnostics {
        assert!(
            !diagnostic.rule.is_mandatory(),
            "库策略档位：{}",
            diagnostic.rule
        );
        assert_eq!(diagnostic.mcid, Some(0));
    }
    // 键仍可取，父元素照样定位到。
    assert_eq!(parent.parents_key, Some(KEY));
    assert_eq!(parent.parent, Some(0));
}

#[test]
fn stream_mcr_rejects_owner_whose_appearance_does_not_reference_stream() {
    let doc = document_with(
        vec![(PAGE, page(Dict::new()))],
        vec![(
            ELEMENT,
            figure(
                Some(PAGE),
                vec![mcr(Some(PAGE), Some(STREAM), Some(OWNER), 0)],
            ),
        )],
        key_entry(vec![Value::Ref(ELEMENT, 0)]),
        vec![
            (
                STREAM,
                form_stream(Dict::new().insert("StructParents", Value::Integer(KEY))),
            ),
            (OWNER, annotation(OTHER_STREAM, Dict::new())),
            (OTHER_STREAM, form_stream(Dict::new())),
        ],
    );

    let parent = first(&doc, 0, 0);
    assert_eq!(rules(&parent), vec![StreamMcrRule::OwnerAppearanceMismatch]);
    let diagnostic = &parent.diagnostics[0];
    assert_eq!(diagnostic.object, Some(OWNER), "发现落在所有者对象上");
    assert_eq!(diagnostic.element, Some(0));
    assert_eq!(diagnostic.kid, Some(0));
    assert_eq!(diagnostic.page, Some(0), "生效页解析为页索引 0");
    assert!(
        !diagnostic.rule.is_mandatory(),
        "Table 324 只有 description"
    );
    assert_eq!(parent.level, MarkedContentContainer::Stream);
    assert_eq!(parent.parents_key, Some(KEY));
    assert_eq!(parent.parent, Some(0), "外观不符不阻断父树定位");
}

#[test]
fn stream_mcr_reports_owner_without_stream_reference() {
    // `/StmOwn` 的定义是「owning the stream identified by Stm」：没有 /Stm
    // 时键既不在所有者也不在流上，父树定位只能报缺键。
    let doc = document_with(
        vec![(PAGE, page(Dict::new()))],
        vec![(
            ELEMENT,
            figure(Some(PAGE), vec![mcr(Some(PAGE), None, Some(OWNER), 0)]),
        )],
        key_entry(vec![Value::Ref(ELEMENT, 0)]),
        vec![(OWNER, Value::Dict(Dict::new()))],
    );

    let parent = first(&doc, 0, 0);
    assert_eq!(
        rules(&parent),
        vec![
            StreamMcrRule::OwnerWithoutStream,
            StreamMcrRule::MissingStructParents
        ]
    );
    assert!(!parent.diagnostics[0].rule.is_mandatory());
    assert!(parent.diagnostics[1].rule.is_mandatory());
    assert_eq!(parent.container, Some((OWNER, 0)));
    assert_eq!(parent.level, MarkedContentContainer::Owner);
    assert_eq!(parent.parents_key, None);
    assert_eq!(parent.parent, None);
}

#[test]
fn stream_mcr_without_stream_resolves_parent_through_page_struct_parents() {
    let doc = document_with(
        vec![(
            PAGE,
            page(Dict::new().insert("StructParents", Value::Integer(KEY))),
        )],
        vec![(ELEMENT, figure(Some(PAGE), vec![mcr(None, None, None, 0)]))],
        key_entry(vec![Value::Ref(ELEMENT, 0)]),
        Vec::new(),
    );

    let parent = first(&doc, 0, 0);
    assert_eq!(parent.level, MarkedContentContainer::Page);
    assert_eq!(parent.container, Some((PAGE, 0)));
    assert_eq!(parent.parents_key, Some(KEY));
    assert_eq!(parent.parent, Some(0));
    assert_eq!(parent.stream, None);
    assert_eq!(parent.page, Some((PAGE, 0)));
    assert!(parent.diagnostics.is_empty(), "{:?}", parent.diagnostics);
    // 页面层的既有行为不回归。
    let parents = doc
        .page_struct_parents(0)
        .expect("page parents")
        .expect("page declares /StructParents");
    assert_eq!(parents.key, KEY);
    assert_eq!(parents.parents, vec![Some(0)]);
    assert!(sweep(&doc).is_empty(), "页面层序列不参与容器扫描");
}

#[test]
fn stream_mcr_pg_overrides_element_page_when_locating_container() {
    // 元素的 `/Pg` 指向没有键的页 4，MCR 的 `/Pg` 覆盖为带键的页 8。
    let elements = vec![(
        ELEMENT,
        figure(Some(PAGE), vec![mcr(Some(PAGE_TWO), None, None, 0)]),
    )];
    let doc = document_with(
        vec![
            (PAGE, page(Dict::new())),
            (
                PAGE_TWO,
                page(Dict::new().insert("StructParents", Value::Integer(KEY))),
            ),
        ],
        elements,
        key_entry(vec![Value::Ref(ELEMENT, 0)]),
        Vec::new(),
    );

    let parent = first(&doc, 0, 0);
    assert_eq!(
        parent.page,
        Some((PAGE_TWO, 0)),
        "Table 324 的 /Pg 覆盖元素"
    );
    assert_eq!(parent.container, Some((PAGE_TWO, 0)));
    assert_eq!(parent.level, MarkedContentContainer::Page);
    assert_eq!(parent.parents_key, Some(KEY));
    assert_eq!(parent.parent, Some(0));
    assert!(parent.diagnostics.is_empty(), "{:?}", parent.diagnostics);

    // 覆盖后的页没有键时，报出的页索引取覆盖后的页（1），不是元素的页。
    let uncovered = document_with(
        vec![
            (
                PAGE,
                page(Dict::new().insert("StructParents", Value::Integer(KEY))),
            ),
            (PAGE_TWO, page(Dict::new())),
        ],
        vec![(
            ELEMENT,
            figure(Some(PAGE), vec![mcr(Some(PAGE_TWO), None, None, 0)]),
        )],
        key_entry(vec![Value::Ref(ELEMENT, 0)]),
        Vec::new(),
    );
    let parent = first(&uncovered, 0, 0);
    assert_eq!(rules(&parent), vec![StreamMcrRule::MissingStructParents]);
    assert_eq!(parent.diagnostics[0].page, Some(1), "生效页是页索引 1");
    assert_eq!(parent.diagnostics[0].object, Some(PAGE_TWO));
    assert_eq!(parent.parent, None);
}

#[test]
fn stream_mcr_reports_missing_struct_parents_on_container() {
    let doc = positive_document(0, form_stream(Dict::new()));

    let parent = first(&doc, 0, 0);
    assert_eq!(rules(&parent), vec![StreamMcrRule::MissingStructParents]);
    let diagnostic = &parent.diagnostics[0];
    assert!(diagnostic.rule.is_mandatory(), "14.7.4.4 shall");
    assert_eq!(diagnostic.object, Some(STREAM));
    assert_eq!(diagnostic.key, None);
    assert_eq!(diagnostic.mcid, Some(0));
    assert_eq!(diagnostic.page, Some(0));
    assert_eq!(parent.parents_key, None, "键无处可取");
    assert_eq!(parent.parent, None);
    assert_eq!(parent.container, Some((STREAM, 0)), "容器仍是 /Stm 流");
    assert_eq!(
        sweep(&doc),
        parent.diagnostics.clone(),
        "扫描按同一坐标报出同一条发现"
    );
}

#[test]
fn table_326_rejects_container_with_struct_parent_and_struct_parents() {
    let doc = positive_document(
        0,
        form_stream(
            Dict::new()
                .insert("StructParents", Value::Integer(KEY))
                .insert("StructParent", Value::Integer(KEY)),
        ),
    );

    let parent = first(&doc, 0, 0);
    assert_eq!(rules(&parent), vec![StreamMcrRule::BothParentEntries]);
    let diagnostic = &parent.diagnostics[0];
    assert!(diagnostic.rule.is_mandatory(), "Table 326 shall");
    assert_eq!(diagnostic.object, Some(STREAM));
    assert_eq!(diagnostic.element, Some(0));
    assert_eq!(diagnostic.kid, Some(0));
    assert_eq!(diagnostic.mcid, Some(0));
    // 双键仍按 `/StructParents` 取键，读端只报告不改判。
    assert_eq!(parent.parents_key, Some(KEY));
    assert_eq!(parent.parent, Some(0));
    assert_eq!(sweep(&doc), parent.diagnostics);
}

#[test]
fn table_326_rejects_container_with_only_struct_parent() {
    let doc = positive_document(
        0,
        form_stream(Dict::new().insert("StructParent", Value::Integer(KEY))),
    );

    let parent = first(&doc, 0, 0);
    assert_eq!(rules(&parent), vec![StreamMcrRule::ContainerIsContentItem]);
    assert!(
        parent.diagnostics[0].rule.is_mandatory(),
        "内容项与容器不可兼得 shall"
    );
    assert_eq!(parent.parents_key, None);
    assert_eq!(parent.parent, None);
    assert_eq!(sweep(&doc), parent.diagnostics);
}

#[test]
fn stream_mcr_reports_struct_parents_that_is_not_a_non_negative_integer() {
    let doc = positive_document(
        0,
        form_stream(Dict::new().insert("StructParents", name("seven"))),
    );

    let parent = first(&doc, 0, 0);
    assert_eq!(rules(&parent), vec![StreamMcrRule::StructParentsNotInteger]);
    assert!(parent.diagnostics[0].rule.is_mandatory());
    assert_eq!(parent.parents_key, None);
    assert_eq!(parent.parent, None);

    // 负数键同样非法。
    let negative = positive_document(
        0,
        form_stream(Dict::new().insert("StructParents", Value::Integer(-1))),
    );
    assert_eq!(
        rules(&first(&negative, 0, 0)),
        vec![StreamMcrRule::StructParentsNotInteger]
    );
}

#[test]
fn stream_mcr_reports_parent_tree_key_missing_and_object_shaped_entry() {
    // 父树里没有键 7 的条目。
    let missing = first(&stream_document(0, keyed_stream(), Vec::new()), 0, 0);
    assert_eq!(
        rules(&missing),
        vec![StreamMcrRule::ParentTreeKeyMissing],
        "容器缺键与键不在父树是两种失败"
    );
    assert_eq!(missing.diagnostics[0].key, Some(KEY));
    assert!(missing.diagnostics[0].rule.is_mandatory());
    assert_eq!(missing.parents_key, Some(KEY));
    assert_eq!(missing.parent, None);

    // 键存在但条目是 OBJR 用的单元引用，不是 MCID 数组。
    let shaped = stream_document(
        0,
        keyed_stream(),
        vec![Value::Integer(KEY), Value::Ref(ELEMENT, 0)],
    );
    let shaped = first(&shaped, 0, 0);
    assert_eq!(rules(&shaped), vec![StreamMcrRule::ParentEntryNotMcidArray]);
    assert!(
        shaped.diagnostics[0].rule.is_mandatory(),
        "14.7.4.4 shall：条目应为数组"
    );
    assert_eq!(shaped.parents_key, Some(KEY));
    assert_eq!(shaped.parent, None);
}

#[test]
fn stream_mcr_reports_empty_and_out_of_range_mcid_slots() {
    let doc = stream_document(0, keyed_stream(), key_entry(vec![Value::Null]));
    let empty = first(&doc, 0, 0);
    assert_eq!(rules(&empty), vec![StreamMcrRule::McidSlotEmpty]);
    assert_eq!(empty.parents_key, Some(KEY));
    assert_eq!(empty.parent, None, "null 槽位没有父元素");

    let out_of_range = first(&positive_document(3, keyed_stream()), 0, 0);
    assert_eq!(
        rules(&out_of_range),
        vec![StreamMcrRule::McidIndexOutOfBounds]
    );
    assert!(out_of_range.diagnostics[0].rule.is_mandatory());
    assert_eq!(out_of_range.parents_key, Some(KEY));
    assert_eq!(out_of_range.parent, None);
}

#[test]
fn table_326_rejects_stream_container_also_claimed_by_object_reference() {
    // 对象 11 既是 `/Stm` 容器（键在其流字典上），又被另一元素的 OBJR 声称。
    let doc = document_with(
        vec![(PAGE, page(Dict::new()))],
        vec![
            (
                ELEMENT,
                figure(Some(PAGE), vec![mcr(Some(PAGE), Some(STREAM), None, 0)]),
            ),
            (
                OTHER_ELEMENT,
                figure(Some(PAGE), vec![objr(Some(PAGE), STREAM)]),
            ),
        ],
        key_entry(vec![Value::Ref(ELEMENT, 0)]),
        vec![(STREAM, keyed_stream())],
    );

    let found = sweep(&doc);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].rule, StreamMcrRule::ContainerAlsoObjectReferenced);
    assert!(found[0].rule.is_mandatory(), "Table 326 not both shall");
    assert_eq!(found[0].object, Some(STREAM));
    assert_eq!(
        (found[0].element, found[0].kid),
        (Some(0), Some(0)),
        "定位取首个声称该对象的子项"
    );
    // 逐子项定位本身不报错：父元素照样取得，容器仍带键。
    let parent = first(&doc, 0, 0);
    assert!(parent.diagnostics.is_empty(), "{:?}", parent.diagnostics);
    assert_eq!(parent.parent, Some(0));
}

#[test]
fn table_326_rejects_object_reference_target_with_both_parent_entries() {
    // OBJR 目标不是容器：双键互斥由容器层扫描补上（页面层之外的对象）。
    let doc = document_with(
        vec![(
            PAGE,
            page(Dict::new().insert("StructParents", Value::Integer(KEY))),
        )],
        vec![(
            ELEMENT,
            figure(
                Some(PAGE),
                vec![
                    objr(Some(PAGE), OBJR_TARGET),
                    mcr(Some(PAGE), None, None, 0),
                ],
            ),
        )],
        key_entry(vec![Value::Null]),
        vec![(
            OBJR_TARGET,
            Value::Dict(
                Dict::new()
                    .insert("Subtype", name("Link"))
                    .insert("StructParent", Value::Integer(9))
                    .insert("StructParents", Value::Integer(8)),
            ),
        )],
    );

    let found = sweep(&doc);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].rule, StreamMcrRule::BothParentEntries);
    assert_eq!(found[0].object, Some(OBJR_TARGET));
    assert_eq!((found[0].element, found[0].kid), (Some(0), Some(0)));
    assert!(found[0].mcid.is_none(), "对象层互斥与 MCID 无关");
    // 页面层序列仍按页上的键定位（MCID 0 槽位为 null）。
    let parent = first(&doc, 0, 1);
    assert_eq!(parent.level, MarkedContentContainer::Page);
    assert_eq!(parent.parents_key, Some(KEY));
    assert_eq!(rules(&parent), vec![StreamMcrRule::McidSlotEmpty]);
}

#[test]
fn marked_content_parent_requires_valid_kid_and_mcr_shape() {
    let doc = positive_document(0, keyed_stream());
    // 子项不是 MCR（整数 MCID）不是错误，只是不在本通道。
    let integer_kid = document_with(
        vec![(PAGE, page(Dict::new()))],
        vec![(ELEMENT, figure(Some(PAGE), vec![Value::Integer(0)]))],
        key_entry(vec![Value::Ref(ELEMENT, 0)]),
        Vec::new(),
    );
    assert_eq!(integer_kid.marked_content_parent(0, 0), Ok(None));

    for (element, kid) in [(9, 0), (0, 9)] {
        let error = doc
            .marked_content_parent(element, kid)
            .expect_err("越界下标应报错");
        assert!(matches!(error, StructureError::Malformed(_)), "{error:?}");
    }

    // 没有结构树的文档：既没有父元素也没有发现，读取阶段不报错。
    let mut plain = Document::new();
    insert(
        &plain,
        CATALOG,
        Value::Dict(Dict::new().insert("Type", name("Catalog"))),
    );
    plain.trailer = Trailer {
        dict: Dict::new().insert("Root", Value::Ref(CATALOG, 0)),
    };
    assert_eq!(plain.marked_content_parent(0, 0), Ok(None));
    assert!(plain.stream_mcr_diagnostic().expect("no tree").is_empty());
}
