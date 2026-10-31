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
        rule,
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
