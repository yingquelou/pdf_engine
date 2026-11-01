//! Artifact 归属诊断的单元测试（ISO 32000-1 14.8.2.2.1、14.8.2.3.1）。

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

#[test]
fn artifact_with_mcid_is_flagged_as_marked_structure_content() {
    let content = b"/Artifact <</Type /Pagination /MCID 0>> BDC EMC \
                    /P <</MCID 1>> BDC EMC";
    let doc = document(content, root_with(vec![Value::Ref(DOC, 0)]));
    insert(
        &doc,
        DOC,
        Value::Dict(page_element("Document", ROOT).insert("K", arr(vec![Value::Integer(1)]))),
    );
    let found = violations(&doc, 0);
    assert_eq!(
        found,
        vec![violation(
            Some(0),
            None,
            None,
            ArtifactAttributionRule::ArtifactMarkedAsStructure
        )],
        "带合法 /MCID 的 Artifact 区段应被记为被赋予结构身份，且父树缺失时 element 降级为 None"
    );
    assert!(
        found[0].rule.is_mandatory(),
        "14.8.2.3.1 的 Artifact 结构身份是 shall 级发现"
    );
    assert!(
        !ArtifactAttributionRule::RealContentNotInStructure.is_mandatory(),
        "真实内容未入结构只是 14.8.2.2.1 的 should 级发现"
    );
}

#[test]
fn artifact_without_property_list_is_not_flagged() {
    let content = b"/Artifact BMC EMC /Artifact <</Type /Layout>> BDC EMC";
    let doc = document(content, struct_root(Dict::new()));
    assert_eq!(
        violations(&doc, 0),
        vec![],
        "无属性表或属性表不含 /MCID 的 Artifact 属于合法的生产残留，不应有任何发现"
    );
}

#[test]
fn artifact_mcid_resolves_to_parent_tree_element() {
    let content = b"/Artifact <</MCID 0>> BDC EMC /P <</MCID 1>> BDC EMC \
                    /Artifact <</MCID 2>> BDC EMC";
    let root = struct_root(
        Dict::new()
            .insert("K", arr(vec![Value::Ref(DOC, 0)]))
            .insert(
                "ParentTree",
                Value::Dict(Dict::new().insert(
                    "Nums",
                    arr(vec![
                        Value::Integer(0),
                        arr(vec![Value::Ref(DOC, 0), Value::Ref(CHILD, 0), Value::Null]),
                    ]),
                )),
            ),
    );
    let doc = document(content, root);
    insert(
        &doc,
        DOC,
        Value::Dict(page_element("Document", ROOT).insert("K", arr(vec![Value::Ref(CHILD, 0)]))),
    );
    insert(&doc, CHILD, Value::Dict(element("Span", DOC)));
    assert_eq!(
        violations(&doc, 0),
        vec![
            violation(
                Some(0),
                Some(0),
                None,
                ArtifactAttributionRule::ArtifactMarkedAsStructure
            ),
            violation(
                Some(2),
                None,
                None,
                ArtifactAttributionRule::ArtifactMarkedAsStructure
            ),
        ],
        "MCID 0 应经父树定位到 arena 元素 0，MCID 2 的父树空位应降级为 None"
    );
}

#[test]
fn structure_element_mcid_child_referencing_artifact_is_flagged() {
    let content = b"/Artifact <</MCID 0>> BDC EMC /P <</MCID 1>> BDC EMC";
    let doc = document(content, root_with(vec![Value::Ref(DOC, 0)]));
    insert(
        &doc,
        DOC,
        Value::Dict(page_element("Document", ROOT).insert("K", arr(vec![Value::Ref(CHILD, 0)]))),
    );
    insert(
        &doc,
        CHILD,
        Value::Dict(page_element("P", DOC).insert("K", arr(vec![Value::Integer(0)]))),
    );
    assert_eq!(
        violations(&doc, 0),
        vec![
            violation(
                Some(0),
                None,
                None,
                ArtifactAttributionRule::ArtifactMarkedAsStructure
            ),
            violation(
                Some(0),
                Some(1),
                Some(0),
                ArtifactAttributionRule::ArtifactInLogicalStructure
            ),
        ],
        "结构元素 /K 的整数 MCID 命中本页 Artifact 区段时应同时触发规则 1 与规则 2"
    );
    assert!(
        violations(&doc, 0)[1].rule.is_mandatory(),
        "14.8.2.2.1 禁止 artifact 进入逻辑结构，是 shall 级发现"
    );
}

#[test]
fn structure_mcr_with_stream_is_not_attributed_to_page() {
    let content = b"/Artifact <</MCID 0>> BDC EMC /P <</MCID 1>> BDC EMC";
    let doc = document(content, root_with(vec![Value::Ref(DOC, 0)]));
    insert(
        &doc,
        DOC,
        Value::Dict(page_element("Document", ROOT).insert(
            "K",
            arr(vec![
                mcr(Some("Stm"), 0),
                mcr(Some("StmOwn"), 0),
                mcr(None, 0),
            ]),
        )),
    );
    insert(&doc, STM, Value::Dict(Dict::new()));
    assert_eq!(
        violations(&doc, 0),
        vec![
            violation(
                Some(0),
                None,
                None,
                ArtifactAttributionRule::ArtifactMarkedAsStructure
            ),
            violation(
                Some(0),
                Some(0),
                Some(2),
                ArtifactAttributionRule::ArtifactInLogicalStructure
            ),
        ],
        "带 /Stm 或 /StmOwn 的 MCR 序列不在页面流里，应只有无流 MCR 命中归属"
    );
}

#[test]
fn structure_mcr_page_entry_without_element_page_is_checked() {
    let content = b"/Artifact <</MCID 0>> BDC EMC /P <</MCID 1>> BDC EMC";
    let doc = document(content, root_with(vec![Value::Ref(DOC, 0)]));
    insert(
        &doc,
        DOC,
        Value::Dict(element("Document", ROOT).insert("K", arr(vec![Value::Ref(CHILD, 0)]))),
    );
    insert(
        &doc,
        CHILD,
        Value::Dict(element("P", DOC).insert("K", arr(vec![mcr(None, 0)]))),
    );
    assert_eq!(
        violations(&doc, 0),
        vec![
            violation(
                Some(0),
                None,
                None,
                ArtifactAttributionRule::ArtifactMarkedAsStructure
            ),
            violation(
                Some(0),
                Some(1),
                Some(0),
                ArtifactAttributionRule::ArtifactInLogicalStructure
            ),
        ],
        "元素自身没有 /Pg 时应按 MCR 的 /Pg 判定生效页，规则 2 仍要命中"
    );
}

#[test]
fn structure_mcid_child_without_element_page_is_not_checked() {
    let content = b"/Artifact <</MCID 0>> BDC EMC";
    let doc = document(content, root_with(vec![Value::Ref(DOC, 0)]));
    insert(
        &doc,
        DOC,
        Value::Dict(element("Document", ROOT).insert("K", arr(vec![Value::Integer(0)]))),
    );
    assert_eq!(
        violations(&doc, 0),
        vec![violation(
            Some(0),
            None,
            None,
            ArtifactAttributionRule::ArtifactMarkedAsStructure
        )],
        "整数 MCID 子项没有自己的页，元素缺 /Pg 时无法归属到本页"
    );
}

#[test]
fn structure_element_on_other_page_is_not_checked() {
    let content = b"/Artifact <</MCID 0>> BDC EMC";
    let doc = document(content, root_with(vec![Value::Ref(DOC, 0)]));
    insert(
        &doc,
        DOC,
        Value::Dict(
            element("Document", ROOT)
                .insert("Pg", Value::Ref(PAGE2, 0))
                .insert("K", arr(vec![Value::Integer(0)])),
        ),
    );
    insert(
        &doc,
        PAGE2,
        Value::Dict(Dict::new().insert("Type", name("Page"))),
    );
    assert_eq!(
        violations(&doc, 0),
        vec![violation(
            Some(0),
            None,
            None,
            ArtifactAttributionRule::ArtifactMarkedAsStructure
        )],
        "元素 /Pg 指向其它页时规则 2 不应检查该元素"
    );
}

#[test]
fn real_content_without_mcid_is_flagged_as_unstructured() {
    let content = b"/P <</MCID 0>> BDC EMC /Span BMC EMC";
    let doc = document(content, struct_root(Dict::new()));
    assert_eq!(
        violations(&doc, 0),
        vec![violation(
            Some(1),
            None,
            None,
            ArtifactAttributionRule::RealContentNotInStructure
        )],
        "自身与祖先都没有合法 /MCID 的 Span 区段应被记为真实内容未入结构"
    );

    let content = b"/P BMC /Span <</MCID 0>> DP EMC";
    let doc = document(content, struct_root(Dict::new()));
    assert_eq!(
        violations(&doc, 0),
        vec![violation(
            Some(1),
            None,
            None,
            ArtifactAttributionRule::RealContentNotInStructure
        )],
        "DP 点与其它区段同样处理：带合法 /MCID 的点豁免，外层 /P 无 MCID 仍被记"
    );
}

#[test]
fn nested_real_content_inherits_mcid_from_enclosing_section() {
    let content = b"/P <</MCID 0>> BDC /Span BMC EMC EMC";
    let doc = document(content, struct_root(Dict::new()));
    assert_eq!(
        violations(&doc, 0),
        vec![],
        "嵌套的 Span 应沿 parent 链继承外层 /P 的合法 MCID 身份"
    );

    let content = b"/P BMC /Span <</MCID 1.5>> BDC EMC EMC";
    let doc = document(content, struct_root(Dict::new()));
    assert_eq!(
        violations(&doc, 0),
        vec![
            violation(
                Some(0),
                None,
                None,
                ArtifactAttributionRule::RealContentNotInStructure
            ),
            violation(
                Some(1),
                None,
                None,
                ArtifactAttributionRule::RealContentNotInStructure
            ),
        ],
        "整条链都没有合法 MCID 时内外两层区段都应被记，且按区段下标升序输出"
    );
}

#[test]
fn tag_suspect_subtree_is_exempt_from_real_content_rule() {
    // 写成 14.8.2.3.1 的规范形态（带 `/TagSuspect /Ordering` 属性表的 BDC，
    // 目录声明 `/MarkInfo << /Suspects true >>`），剩下的发现只可能来自
    // 真实内容规则，从而证明豁免确实生效。
    let content = b"/TagSuspect <</TagSuspect /Ordering>> BDC /Span BMC EMC EMC";
    let doc = document(content, struct_root(Dict::new()));
    with_suspects(&doc);
    assert_eq!(
        violations(&doc, 0),
        vec![],
        "TagSuspect 区段及其整个子树按本切片豁免真实内容规则"
    );
}

#[test]
fn tag_suspect_section_without_property_is_reported() {
    // `BMC` 完全没有属性表；`BDC` 的属性表缺 `/TagSuspect` 条目。
    let content = b"/TagSuspect BMC EMC /TagSuspect <</Type /Layout>> BDC EMC";
    let doc = document(content, struct_root(Dict::new()));
    with_suspects(&doc);
    assert_eq!(
        violations(&doc, 0),
        vec![
            violation(
                Some(0),
                None,
                None,
                ArtifactAttributionRule::TagSuspectMissingProperty
            ),
            violation(
                Some(1),
                None,
                None,
                ArtifactAttributionRule::TagSuspectMissingProperty
            ),
        ],
        "无属性表与缺 /TagSuspect 条目的区段都应按区段下标升序各记一条发现"
    );
    assert!(
        ArtifactAttributionRule::TagSuspectMissingProperty.is_mandatory()
            && ArtifactAttributionRule::TagSuspectSuspectsFlagNotSet.is_mandatory(),
        "14.8.2.3.1 的 tag suspects 两条规则都是 shall 级发现"
    );
}

#[test]
fn tag_suspect_property_with_wrong_value_is_reported() {
    let cases = [
        b"/TagSuspect <</TagSuspect /Sequence>> BDC EMC".as_slice(),
        b"/TagSuspect <</TagSuspect 1>> BDC EMC".as_slice(),
        b"/TagSuspect <</TagSuspect (Ordering)>> BDC EMC".as_slice(),
        b"/TagSuspect <</TagSuspect null>> BDC EMC".as_slice(),
    ];
    for content in cases {
        let doc = document(content, struct_root(Dict::new()));
        with_suspects(&doc);
        assert_eq!(
            violations(&doc, 0),
            vec![violation(
                Some(0),
                None,
                None,
                ArtifactAttributionRule::TagSuspectMissingProperty
            )],
            "`/TagSuspect` 条目值不是名字 Ordering 时应记一条属性发现: {:?}",
            String::from_utf8_lossy(content)
        );
    }
}

#[test]
fn tag_suspect_without_mark_info_suspects_flag_is_reported() {
    let content = b"/TagSuspect <</TagSuspect /Ordering>> BDC EMC";
    let flags = [
        // 目录没有 /MarkInfo。
        None,
        // 有 /MarkInfo 但 /Suspects 为 false。
        Some(
            Dict::new()
                .insert("Marked", Value::Bool(true))
                .insert("Suspects", Value::Bool(false)),
        ),
        // 有 /MarkInfo 但没有 /Suspects 条目。
        Some(Dict::new().insert("Marked", Value::Bool(true))),
        // /Suspects 非布尔：读 /MarkInfo 报错时按「未声明」降级，诊断不失败。
        Some(Dict::new().insert("Suspects", name("yes"))),
    ];
    for flag in &flags {
        let doc = document(content, struct_root(Dict::new()));
        if let Some(dict) = flag {
            set_mark_info(&doc, Value::Dict(dict.clone()));
        }
        assert_eq!(
            violations(&doc, 0),
            vec![violation(
                None,
                None,
                None,
                ArtifactAttributionRule::TagSuspectSuspectsFlagNotSet
            )],
            "属性表合规、但 /MarkInfo 没有 /Suspects true 时应只记一条 flag 发现: {flag:?}"
        );
    }
}

#[test]
fn tag_suspect_with_flag_and_property_is_clean() {
    let content = b"/TagSuspect <</TagSuspect /Ordering>> BDC /Span BMC EMC EMC";
    let doc = document(content, struct_root(Dict::new()));
    with_suspects(&doc);
    assert_eq!(
        violations(&doc, 0),
        vec![],
        "规范写法的 tag suspect 区段不应产生任何归属发现"
    );
}

#[test]
fn tag_suspect_missing_flag_reported_once_per_page() {
    let content = b"/TagSuspect BMC EMC /P <</MCID 0>> BDC EMC \
                    /TagSuspect <</TagSuspect /Sequence>> BDC EMC";
    let doc = document(content, struct_root(Dict::new()));
    let found = violations(&doc, 0);
    assert_eq!(
        found,
        vec![
            violation(
                Some(0),
                None,
                None,
                ArtifactAttributionRule::TagSuspectMissingProperty
            ),
            violation(
                Some(2),
                None,
                None,
                ArtifactAttributionRule::TagSuspectMissingProperty
            ),
            violation(
                None,
                None,
                None,
                ArtifactAttributionRule::TagSuspectSuspectsFlagNotSet
            ),
        ],
        "两个不合规区段按升序各记一条属性发现，flag 发现整页只有一条且排在最后"
    );
}

#[test]
fn tag_suspect_findings_do_not_affect_other_pages() {
    let doc = two_page_document(
        b"/P <</MCID 0>> BDC EMC",
        b"/TagSuspect <</TagSuspect /Ordering>> BDC EMC",
    );
    // 页 1 有 tag suspects 而目录未声明：flag 发现只落在页 1。
    assert_eq!(
        violations(&doc, 0),
        vec![],
        "没有 TagSuspect 区段的页面不应因其他页的 tag suspects 被记发现"
    );
    assert_eq!(
        violations(&doc, 1),
        vec![violation_on(
            1,
            None,
            ArtifactAttributionRule::TagSuspectSuspectsFlagNotSet
        )],
        "flag 发现应带自己的页索引"
    );

    // 声明后：页 0 依然干净，页 1 的属性表不合规只记在该页。
    let doc = two_page_document(b"/P <</MCID 0>> BDC EMC", b"/TagSuspect BMC EMC");
    with_suspects(&doc);
    assert_eq!(
        violations(&doc, 0),
        vec![],
        "同一份 /MarkInfo 声明下页 0 仍不应出现任何发现"
    );
    assert_eq!(
        violations(&doc, 1),
        vec![violation_on(
            1,
            Some(0),
            ArtifactAttributionRule::TagSuspectMissingProperty
        )],
        "属性表发现只属于含该区段的页"
    );
}

#[test]
fn invalid_mcid_forms_are_not_treated_as_structure_identity() {
    let content = b"/Artifact <</MCID 1.5>> BDC EMC /P <</MCID -1>> BDC EMC \
                    /Artifact <</MCID (0)>> BDC EMC /P <</MCID /2>> BDC EMC";
    let doc = document(content, struct_root(Dict::new()));
    assert_eq!(
        violations(&doc, 0),
        vec![
            violation(
                Some(1),
                None,
                None,
                ArtifactAttributionRule::RealContentNotInStructure
            ),
            violation(
                Some(3),
                None,
                None,
                ArtifactAttributionRule::RealContentNotInStructure
            ),
        ],
        "非整数、负数、字符串与名字的 /MCID 都不构成结构身份：Artifact 不记规则 1，真实内容照记规则 3"
    );
}

#[test]
fn artifact_attribution_violations_reports_error_for_unknown_page() {
    let content = b"/P <</MCID 0>> BDC EMC";
    let doc = document(content, struct_root(Dict::new()));
    let result = doc.artifact_attribution_violations(1);
    assert!(
        matches!(result, Err(ArtifactAttributionError::MarkedContent(_))),
        "越界页码应返回标记内容错误，got {result:?}"
    );
}

#[test]
fn visible_annotation_not_claimed_by_structure_is_reported() {
    // 规则 6：有 `/AP /N`、`/F` 未置 Hidden（即 14.8.2.2.1 前两个条件成立），
    // 结构树存在却没有任何 OBJR 声称它——第三个条件缺失。
    let doc = document(b"", root_with(vec![Value::Ref(DOC, 0)]));
    insert(&doc, DOC, Value::Dict(page_element("Document", ROOT)));
    insert(&doc, ANNOT, Value::Dict(normal_appearance_annotation()));
    attach_annotations(&doc, vec![Value::Ref(ANNOT, 0)]);
    let found = violations(&doc, 0);
    assert_eq!(
        found,
        vec![visible_annotation_flagged(0)],
        "可见注解未进入逻辑结构应记一条规则 6 发现，annotation 取 /Annots 下标"
    );
    assert!(
        !found[0].rule.is_mandatory(),
        "14.8.2.2.1 的注解条件整体是定义式表述，规则 6 应与规则 3 同为 should 级"
    );
}

#[test]
fn annotation_without_normal_appearance_is_not_required_in_structure() {
    // 14.8.2.2.1 第一条件不成立（没有含 `/N` 的外观流）：不属于真实内容，
    // 未入结构也不记发现。
    let doc = document(b"", root_with(vec![Value::Ref(DOC, 0)]));
    insert(&doc, DOC, Value::Dict(page_element("Document", ROOT)));
    insert(&doc, ANNOT, Value::Dict(appearanceless_annotation()));
    attach_annotations(&doc, vec![Value::Ref(ANNOT, 0)]);
    assert_eq!(
        violations(&doc, 0),
        vec![],
        "无常态外观的注解不要求进入逻辑结构"
    );
}

#[test]
fn hidden_annotation_with_normal_appearance_is_not_required_in_structure() {
    // 14.8.2.2.1 第二条件不成立（`/F` 的 Hidden 位，Table 165 的 bit 1）。
    let doc = document(b"", root_with(vec![Value::Ref(DOC, 0)]));
    insert(&doc, DOC, Value::Dict(page_element("Document", ROOT)));
    insert(
        &doc,
        ANNOT,
        Value::Dict(normal_appearance_annotation().insert("F", Value::Integer(2))),
    );
    attach_annotations(&doc, vec![Value::Ref(ANNOT, 0)]);
    assert_eq!(
        violations(&doc, 0),
        vec![],
        "置了 Hidden 位的注解即使带 `/AP /N` 也不要求进入逻辑结构"
    );
}

#[test]
fn visible_annotation_claimed_by_object_reference_is_clean() {
    // 三条件齐备：`/AP /N` + 未 Hidden + 被 OBJR 声称且 `/StructParent`
    // 与父树单元条目一致——两条注解规则都应干净。
    let root = struct_root(
        Dict::new()
            .insert("K", arr(vec![Value::Ref(DOC, 0)]))
            .insert(
                "ParentTree",
                Value::Dict(
                    Dict::new().insert("Nums", arr(vec![Value::Integer(0), Value::Ref(DOC, 0)])),
                ),
            ),
    );
    let doc = document(b"", root);
    insert(
        &doc,
        DOC,
        Value::Dict(page_element("Document", ROOT).insert("K", arr(vec![objr(ANNOT)]))),
    );
    insert(
        &doc,
        ANNOT,
        Value::Dict(normal_appearance_annotation().insert("StructParent", Value::Integer(0))),
    );
    attach_annotations(&doc, vec![Value::Ref(ANNOT, 0)]);
    assert_eq!(
        violations(&doc, 0),
        vec![],
        "被 OBJR 正确声称的可见注解不应产生任何归属发现"
    );
}

#[test]
fn annotation_object_reference_without_struct_parent_is_reported() {
    // 规则 7：OBJR 声称了本页注解，注解字典却没有 `/StructParent`
    // （Table 326 的 shall）。
    let root = struct_root(
        Dict::new()
            .insert("K", arr(vec![Value::Ref(DOC, 0)]))
            .insert(
                "ParentTree",
                Value::Dict(
                    Dict::new().insert("Nums", arr(vec![Value::Integer(0), Value::Ref(DOC, 0)])),
                ),
            ),
    );
    let doc = document(b"", root);
    insert(
        &doc,
        DOC,
        Value::Dict(page_element("Document", ROOT).insert("K", arr(vec![objr(ANNOT)]))),
    );
    insert(&doc, ANNOT, Value::Dict(normal_appearance_annotation()));
    attach_annotations(&doc, vec![Value::Ref(ANNOT, 0)]);
    let found = violations(&doc, 0);
    assert_eq!(
        found,
        vec![objr_parent_tree_mismatch(0, 0, 0)],
        "缺 /StructParent 应记规则 7 发现并带元素、子项与注解下标；可见但已被声称，规则 6 不记"
    );
    assert!(
        found[0].rule.is_mandatory(),
        "14.7.4.4 与 Table 326 的父树键是 shall 级"
    );
}

#[test]
fn annotation_object_reference_parent_tree_must_point_at_claiming_element() {
    // `/StructParent` 键存在时，父树该键必须是「指向声称元素的单元条目」：
    // 键缺失、页面数组形态、单元指向别的元素，三种都不满足 14.7.4.4。
    let build = |parent_tree: Option<Value>, struct_parent: Option<i64>, objr_owner: i64| {
        let mut entries = Dict::new().insert("K", arr(vec![Value::Ref(DOC, 0)]));
        if let Some(tree) = parent_tree {
            entries = entries.insert("ParentTree", tree);
        }
        let doc = document(b"", struct_root(entries));
        let child_kids = arr(vec![objr(ANNOT)]);
        let child = Value::Dict(element("Note", DOC).insert("K", child_kids));
        let doc_kids = if objr_owner == DOC {
            arr(vec![objr(ANNOT)])
        } else {
            insert(&doc, CHILD, child);
            arr(vec![Value::Ref(CHILD, 0)])
        };
        insert(
            &doc,
            DOC,
            Value::Dict(page_element("Document", ROOT).insert("K", doc_kids)),
        );
        let mut annotation = normal_appearance_annotation();
        if let Some(key) = struct_parent {
            annotation = annotation.insert("StructParent", Value::Integer(key));
        }
        insert(&doc, ANNOT, Value::Dict(annotation));
        attach_annotations(&doc, vec![Value::Ref(ANNOT, 0)]);
        doc
    };
    let nums = |values: Vec<Value>| Some(Value::Dict(Dict::new().insert("Nums", arr(values))));
    // 键 0 单元条目指向 DOC（arena 0）；OBJR 在 CHILD（arena 1）时不匹配。
    let cases = vec![
        // 父树整棵缺失：/StructParent 0 无键可查。
        (None, Some(0), DOC, 0),
        // 键 0 是页面数组形态，不是单元条目。
        (
            nums(vec![Value::Integer(0), arr(vec![Value::Ref(DOC, 0)])]),
            Some(0),
            DOC,
            0,
        ),
        // 键 0 单元条目指向 DOC，但声称注解的是 CHILD（arena 1）。
        (
            nums(vec![Value::Integer(0), Value::Ref(DOC, 0)]),
            Some(0),
            CHILD,
            1,
        ),
    ];
    for (parent_tree, struct_parent, objr_owner, expected_element) in cases {
        let doc = build(parent_tree.clone(), struct_parent, objr_owner);
        assert_eq!(
            violations(&doc, 0),
            vec![objr_parent_tree_mismatch(expected_element, 0, 0)],
            "父树链接与声称元素不符应各记一条规则 7（可见但已被声称，规则 6 不记）"
        );
    }
}

#[test]
fn object_reference_to_non_annotation_object_is_not_checked() {
    // OBJR 引用页外对象（如 XObject）：不属于本页注解诊断。
    let doc = document(b"", root_with(vec![Value::Ref(DOC, 0)]));
    insert(
        &doc,
        DOC,
        Value::Dict(page_element("Document", ROOT).insert("K", arr(vec![objr(STM)]))),
    );
    insert(&doc, STM, Value::Dict(Dict::new()));
    assert_eq!(
        violations(&doc, 0),
        vec![],
        "指向非注解对象的 OBJR 不产生注解归属发现"
    );
}

#[test]
fn directly_embedded_visible_annotation_is_reported() {
    // 直接嵌入 `/Annots` 的注解字典没有间接引用，无法被 Table 325 要求的
    // （间接）`/Obj` 声称：可见时同样记规则 6。
    let doc = document(b"", root_with(vec![Value::Ref(DOC, 0)]));
    insert(&doc, DOC, Value::Dict(page_element("Document", ROOT)));
    attach_annotations(&doc, vec![Value::Dict(normal_appearance_annotation())]);
    assert_eq!(
        violations(&doc, 0),
        vec![visible_annotation_flagged(0)],
        "直接嵌入的可见注解无法进入逻辑结构，应记规则 6"
    );
}

#[test]
fn document_without_structure_tree_reports_no_annotation_findings() {
    // 生效范围：目录没有 `/StructTreeRoot` 时不评估注解规则，
    // 可见注解本身不构成注解归属发现。
    let doc = untagged_document(b"");
    insert(&doc, ANNOT, Value::Dict(normal_appearance_annotation()));
    attach_annotations(&doc, vec![Value::Ref(ANNOT, 0)]);
    assert_eq!(
        violations(&doc, 0),
        vec![],
        "无结构树文档不因可见注解产生注解归属发现"
    );
}

#[test]
fn corrupt_annotation_array_produces_no_annotation_findings() {
    // 降级路径：注解字典 `/AP` 条目不是字典时 `page_annotations` 整体
    // 报错，规则 6/7 不产出发现，且诊断不报错（`violations` 辅助函数
    // 对 Err 直接 panic）。
    let doc = document(b"", root_with(vec![Value::Ref(DOC, 0)]));
    insert(&doc, DOC, Value::Dict(page_element("Document", ROOT)));
    insert(
        &doc,
        ANNOT,
        Value::Dict(appearanceless_annotation().insert("AP", Value::Integer(3))),
    );
    attach_annotations(&doc, vec![Value::Ref(ANNOT, 0)]);
    assert_eq!(
        violations(&doc, 0),
        vec![],
        "注解读取失败应降级为无发现而非报错"
    );
}

#[test]
fn annotation_findings_append_after_the_existing_five_rules() {
    // 输出顺序：既有五段之后先 `VisibleAnnotationNotInStructure`
    // （`/Annots` 下标升序），再 `AnnotationObjrParentTreeMismatch`
    // （元素 arena 下标、子项位置升序）。
    let content = b"/Artifact <</MCID 0>> BDC EMC /Span BMC EMC /TagSuspect BMC EMC";
    let root = struct_root(
        Dict::new()
            .insert("K", arr(vec![Value::Ref(DOC, 0)]))
            .insert(
                "ParentTree",
                Value::Dict(Dict::new().insert(
                    "Nums",
                    arr(vec![
                        Value::Integer(0),
                        arr(vec![Value::Ref(DOC, 0), Value::Null]),
                        Value::Integer(1),
                        Value::Ref(CHILD, 0),
                    ]),
                )),
            ),
    );
    let doc = document(content, root);
    insert(
        &doc,
        DOC,
        Value::Dict(
            page_element("Document", ROOT)
                .insert("K", arr(vec![Value::Integer(0), Value::Ref(CHILD, 0)])),
        ),
    );
    insert(
        &doc,
        CHILD,
        Value::Dict(element("Note", DOC).insert("K", arr(vec![objr(ANNOT2)]))),
    );
    // 注解 0：可见、未被声称 → 规则 6；注解 1：被 CHILD 的 OBJR 声称但缺
    // `/StructParent` → 规则 7，规则 6 不记。
    insert(&doc, ANNOT, Value::Dict(normal_appearance_annotation()));
    insert(&doc, ANNOT2, Value::Dict(normal_appearance_annotation()));
    attach_annotations(&doc, vec![Value::Ref(ANNOT, 0), Value::Ref(ANNOT2, 0)]);
    assert_eq!(
        violations(&doc, 0),
        vec![
            violation(
                Some(0),
                Some(0),
                None,
                ArtifactAttributionRule::ArtifactMarkedAsStructure
            ),
            violation(
                Some(0),
                Some(0),
                Some(0),
                ArtifactAttributionRule::ArtifactInLogicalStructure
            ),
            violation(
                Some(1),
                None,
                None,
                ArtifactAttributionRule::RealContentNotInStructure
            ),
            violation(
                Some(2),
                None,
                None,
                ArtifactAttributionRule::TagSuspectMissingProperty
            ),
            violation(
                None,
                None,
                None,
                ArtifactAttributionRule::TagSuspectSuspectsFlagNotSet
            ),
            visible_annotation_flagged(0),
            objr_parent_tree_mismatch(1, 0, 1),
        ],
        "七段发现应按模块文档声明的固定顺序依次输出"
    );
}
