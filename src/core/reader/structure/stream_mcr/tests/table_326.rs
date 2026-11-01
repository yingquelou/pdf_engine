//! Table 326 容器层互斥校验测试。

use super::super::{MarkedContentContainer, StreamMcrRule};
use super::support::*;
use crate::core::types::{Dict, Value};

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
