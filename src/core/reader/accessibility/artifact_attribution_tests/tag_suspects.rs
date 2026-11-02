//! 14.8.2.3.1 tag suspects：属性表与 /MarkInfo /Suspects 两条 shall 规则、子树豁免与跨页隔离。

use super::*;

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
