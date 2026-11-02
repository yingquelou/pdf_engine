//! 输出顺序：七段发现按模块声明的固定顺序依次追加。

use super::*;

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
