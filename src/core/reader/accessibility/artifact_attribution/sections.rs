use std::collections::BTreeSet;

use super::super::span::mcid_value;
use super::model::{ArtifactAttributionRule, ArtifactAttributionViolation};
use crate::core::reader::content::MarkedSection;
use crate::core::reader::structure::PageStructureParents;
use crate::core::types::{Document, Value};

impl Document {
    /// 规则 1：`Artifact` 区段携带合法 `/MCID`。
    pub(super) fn collect_artifact_identity_violations(
        &self,
        page_index: usize,
        sections: &[MarkedSection],
        page_parents: &Option<PageStructureParents>,
        violations: &mut Vec<ArtifactAttributionViolation>,
    ) {
        for (index, section) in sections.iter().enumerate() {
            if section.tag != "Artifact" {
                continue;
            }
            let Some(properties) = section.properties.as_ref() else {
                continue;
            };
            let Some(mcid) = mcid_value(properties) else {
                continue;
            };
            let element = usize::try_from(mcid)
                .ok()
                .and_then(|mcid| page_parents.as_ref()?.parents.get(mcid).copied().flatten());
            violations.push(ArtifactAttributionViolation {
                page: page_index,
                section: Some(index),
                element,
                kid: None,
                annotation: None,
                rule: ArtifactAttributionRule::ArtifactMarkedAsStructure,
            });
        }
    }

    /// 规则 3：真实内容区段（含祖先链）没有任何合法 `/MCID`。
    pub(super) fn collect_unstructured_content_violations(
        &self,
        page_index: usize,
        sections: &[MarkedSection],
        violations: &mut Vec<ArtifactAttributionViolation>,
    ) {
        for (index, section) in sections.iter().enumerate() {
            if section.tag == "Artifact" {
                continue;
            }
            let mut current = Some(index);
            let mut visited = BTreeSet::new();
            let mut exempt = false;
            let mut structured = false;
            for _ in 0..sections.len() {
                let Some(step) = current else {
                    break;
                };
                if !visited.insert(step) {
                    break;
                }
                let Some(section) = sections.get(step) else {
                    break;
                };
                if section.tag == "TagSuspect" {
                    exempt = true;
                    break;
                }
                if let Some(properties) = section.properties.as_ref()
                    && mcid_value(properties).is_some()
                {
                    structured = true;
                    break;
                }
                current = section.parent;
            }
            if exempt || structured {
                continue;
            }
            violations.push(ArtifactAttributionViolation {
                page: page_index,
                section: Some(index),
                element: None,
                kid: None,
                annotation: None,
                rule: ArtifactAttributionRule::RealContentNotInStructure,
            });
        }
    }

    /// 规则 4/5：`TagSuspect` 区段的属性表与目录 `/MarkInfo /Suspects`
    /// （14.8.2.3.1）。
    pub(super) fn collect_tag_suspect_violations(
        &self,
        page_index: usize,
        sections: &[MarkedSection],
        violations: &mut Vec<ArtifactAttributionViolation>,
    ) {
        let mut has_suspect = false;
        for (index, section) in sections.iter().enumerate() {
            if section.tag != "TagSuspect" {
                continue;
            }
            has_suspect = true;
            // 属性表缺条目、或条目值不是名字 `Ordering`，都不满足规范的写法。
            let conforming = section.properties.as_ref().is_some_and(|properties| {
                matches!(properties.get("TagSuspect"), Some(Value::Name(name)) if name == "Ordering")
            });
            if !conforming {
                violations.push(ArtifactAttributionViolation {
                    page: page_index,
                    section: Some(index),
                    element: None,
                    kid: None,
                    annotation: None,
                    rule: ArtifactAttributionRule::TagSuspectMissingProperty,
                });
            }
        }
        if !has_suspect {
            return;
        }
        // `/MarkInfo` 读失败按「未声明 tag suspects」处理：诊断不因此报错。
        let declared = self
            .mark_info()
            .ok()
            .flatten()
            .is_some_and(|info| info.suspects);
        if !declared {
            violations.push(ArtifactAttributionViolation {
                page: page_index,
                section: None,
                element: None,
                kid: None,
                annotation: None,
                rule: ArtifactAttributionRule::TagSuspectSuspectsFlagNotSet,
            });
        }
    }
}
