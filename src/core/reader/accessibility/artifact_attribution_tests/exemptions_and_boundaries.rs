//! 豁免与边界：真实内容规则的 MCID 继承豁免、非法 /MCID 形态、无属性表残留与越界页错误。

use super::*;

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
