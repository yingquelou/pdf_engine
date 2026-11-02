//! 规则 1/2 的 shall 级诊断：Artifact 被赋予结构身份与 artifact 进入逻辑结构。

use super::*;

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
