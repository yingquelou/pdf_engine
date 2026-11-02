//! 14.8.2.2.1 注解入逻辑结构条件：规则 6（可见未声称）与规则 7（OBJR 父树链接）及干净、降级路径。

use super::*;

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
