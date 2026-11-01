//! 正路父树定位与容器候选顺序测试。

use super::super::{MarkedContentContainer, StreamMcrRule};
use super::support::*;
use crate::core::types::{Dict, Value};

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
