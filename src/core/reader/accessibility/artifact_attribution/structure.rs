use std::collections::{BTreeMap, BTreeSet};

use super::super::span::mcid_value;
use super::model::{ArtifactAttributionRule, ArtifactAttributionViolation};
use crate::core::reader::content::MarkedSection;
use crate::core::reader::structure::{StructureKid, StructureTree};
use crate::core::types::{Document, Value};

impl Document {
    /// 规则 2：本页结构元素的 MCID 子项归属到 `Artifact` 区段。
    pub(super) fn collect_structure_attribution_violations(
        &self,
        page_index: usize,
        sections: &[MarkedSection],
        tree: Option<&StructureTree>,
        violations: &mut Vec<ArtifactAttributionViolation>,
    ) {
        let Some(tree) = tree else {
            return;
        };
        let Some(page_number) = self
            .locate_page_node_with_ref(page_index)
            .ok()
            .and_then(|(_, reference)| reference)
            .map(|(number, _)| number)
        else {
            return;
        };
        let mut artifacts: BTreeMap<i64, usize> = BTreeMap::new();
        for (index, section) in sections.iter().enumerate() {
            if section.tag != "Artifact" {
                continue;
            }
            if let Some(properties) = section.properties.as_ref()
                && let Some(mcid) = mcid_value(properties)
            {
                artifacts.entry(mcid).or_insert(index);
            }
        }
        for (index, element) in tree.elements().iter().enumerate() {
            for (kid, child) in element.kids.iter().enumerate() {
                let (mcid, kid_page) = match child {
                    // 整数 MCID 子项没有自己的页，只能沿用元素的 `/Pg`。
                    StructureKid::Mcid(mcid) => (*mcid, element.page),
                    StructureKid::MarkedContent(reference) => {
                        // 序列不在本页页面流里时无法归属。
                        if reference.stream.is_some() || reference.stream_own.is_some() {
                            continue;
                        }
                        // MCR 的 `/Pg` 覆盖元素的 `/Pg`（Table 324）。
                        (reference.mcid, reference.page.or(element.page))
                    }
                    _ => continue,
                };
                if kid_page.map(|(number, _)| number) != Some(page_number) {
                    continue;
                }
                if let Some(&section) = artifacts.get(&mcid) {
                    violations.push(ArtifactAttributionViolation {
                        page: page_index,
                        section: Some(section),
                        element: Some(index),
                        kid: Some(kid),
                        annotation: None,
                        rule: ArtifactAttributionRule::ArtifactInLogicalStructure,
                    });
                }
            }
        }
    }

    /// 规则 6/7：注解与逻辑结构的归属（14.8.2.2.1 与 14.7.4.3–14.7.4.4）。
    ///
    /// 仅当目录存在可读的逻辑结构树时评估；注解读取失败一律降级为
    /// 「不产生这两条规则的发现」，与模块文档的生效范围一致。
    pub(super) fn collect_annotation_attribution_violations(
        &self,
        page_index: usize,
        tree: Option<&StructureTree>,
        violations: &mut Vec<ArtifactAttributionViolation>,
    ) {
        // 14.8.2.2.1 的第三个条件是「注解包含在文档的逻辑结构中」；
        // 没有结构树就无所谓「纳入」，不为非 Tagged 文档制造注解噪音。
        let Some(tree) = tree else {
            return;
        };
        // 注解数组损坏、注解字典类型错误等读取失败：整组降级为无发现。
        let Ok(annotations) = self.page_annotations(page_index) else {
            return;
        };
        if annotations.is_empty() {
            return;
        }
        // 全树收集 OBJR 声称：规则 6 的「纳入逻辑结构」判据，兼作规则 7
        // 的候选（元素 arena 下标、子项位置升序，与规则 2 的输出约定一致）。
        let mut claimed: BTreeSet<i64> = BTreeSet::new();
        let mut object_references: Vec<(usize, usize, i64)> = Vec::new();
        for (element_index, element) in tree.elements().iter().enumerate() {
            for (kid_index, kid) in element.kids.iter().enumerate() {
                if let StructureKid::ObjectReference(objr) = kid {
                    claimed.insert(objr.object.0);
                    object_references.push((element_index, kid_index, objr.object.0));
                }
            }
        }
        // 本页间接注解的对象号 → `/Annots` 下标；重复对象号取最小下标。
        let mut on_page: BTreeMap<i64, usize> = BTreeMap::new();
        for (index, annotation) in annotations.iter().enumerate() {
            if let Some((number, _)) = annotation.reference {
                on_page.entry(number).or_insert(index);
            }
        }
        // 规则 6：`/AP` 有 `/N`、`/F` 未置 Hidden，却没有被任何 OBJR 声称。
        // 直接嵌入 `/Annots`（无间接引用）的注解不可能被 Table 325 的
        // （间接）`/Obj` 引用声称，可见时同样记入。
        for (index, annotation) in annotations.iter().enumerate() {
            let has_normal = annotation
                .appearance
                .as_ref()
                .is_some_and(|streams| streams.normal.is_some());
            if !has_normal || annotation.flags.hidden() {
                continue;
            }
            let included = annotation
                .reference
                .is_some_and(|(number, _)| claimed.contains(&number));
            if !included {
                violations.push(ArtifactAttributionViolation {
                    page: page_index,
                    section: None,
                    element: None,
                    kid: None,
                    annotation: Some(index),
                    rule: ArtifactAttributionRule::VisibleAnnotationNotInStructure,
                });
            }
        }
        // 规则 7：OBJR 声称本页注解时，注解字典的 `/StructParent` 键必须在
        // 父树中给出指向该声称元素的单元条目（14.7.4.4 的三处 shall）。
        for (element_index, kid_index, object_number) in object_references {
            // `/Obj` 不是本页注解（XObject 或他页注解）：不在本页注解
            // 诊断范围内。
            let Some(&annotation_index) = on_page.get(&object_number) else {
                continue;
            };
            let struct_parent = annotations[annotation_index]
                .dict
                .get("StructParent")
                .and_then(Value::as_integer);
            let linked =
                struct_parent.is_some_and(|key| tree.object_parent(key) == Some(element_index));
            if !linked {
                violations.push(ArtifactAttributionViolation {
                    page: page_index,
                    section: None,
                    element: Some(element_index),
                    kid: Some(kid_index),
                    annotation: Some(annotation_index),
                    rule: ArtifactAttributionRule::AnnotationObjrParentTreeMismatch,
                });
            }
        }
    }
}
