//! MCR 子项的 /Stm、/StmOwn 与元素/MCR /Pg 覆盖归属边界。

use super::*;

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
