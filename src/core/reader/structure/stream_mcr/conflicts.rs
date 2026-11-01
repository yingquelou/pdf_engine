//! `StreamMcrRule` 枚举、`is_mandatory()`、`stream_mcr_diagnostic()` 全树扫描
//! 与 Table 326 容器层互斥复核。

use std::collections::BTreeMap;

use crate::core::types::Document;

use super::super::{StructureError, StructureKid};
use super::container::{StreamMcrDiagnostic, StreamMcrRule, object_dict};
use super::{Spot, note};

/// Table 326 复核条目的定位信息。
struct SweepEntry {
    /// 对象引用（保留 generation 以便读对象）。
    reference: (i64, i64),
    /// 声称该对象的「元素 arena 下标、`/K` 子项位置」。
    claim: Option<(usize, usize)>,
    /// 该对象是否作流形态 MCR 的容器或 `/Stm` 内容流。
    container: bool,
    /// 该对象是否被 OBJR `/Obj` 声称。
    object_reference: bool,
}

/// 登记一个参与 Table 326 复核的对象。
fn mark(
    objects: &mut BTreeMap<i64, SweepEntry>,
    reference: Option<(i64, i64)>,
    container: bool,
    claim: Option<(usize, usize)>,
) {
    let Some(reference) = reference else {
        return;
    };
    let entry = objects.entry(reference.0).or_insert(SweepEntry {
        reference,
        claim,
        container,
        object_reference: false,
    });
    entry.container |= container;
}

/// 全树扫描：流形态 MCR 的定位诊断 + Table 326 容器层互斥诊断。
pub(super) fn diagnostics(document: &Document) -> Result<Vec<StreamMcrDiagnostic>, StructureError> {
    let Some(tree) = document.structure_tree()? else {
        return Ok(Vec::new());
    };
    let mut found = Vec::new();
    // 参与 Table 326 复核的对象：`/Stm`、`/StmOwn` 与 OBJR `/Obj` 指向的任意对象。
    let mut objects: BTreeMap<i64, SweepEntry> = BTreeMap::new();
    for (element_index, element) in tree.elements().iter().enumerate() {
        for (kid_index, kid) in element.kids.iter().enumerate() {
            let claim = Some((element_index, kid_index));
            match kid {
                StructureKid::MarkedContent(reference)
                    if reference.stream.is_some() || reference.stream_own.is_some() =>
                {
                    let resolved = super::parent::marked_content_parent(
                        document,
                        &tree,
                        element_index,
                        kid_index,
                    )?;
                    if let Some(resolved) = resolved {
                        mark(&mut objects, reference.stream, true, claim);
                        mark(&mut objects, reference.stream_own, true, claim);
                        mark(&mut objects, resolved.container, true, claim);
                        found.extend(resolved.diagnostics);
                    }
                }
                StructureKid::ObjectReference(objr) => {
                    objects
                        .entry(objr.object.0)
                        .or_insert(SweepEntry {
                            reference: objr.object,
                            claim,
                            container: false,
                            object_reference: true,
                        })
                        .object_reference = true;
                }
                _ => {}
            }
        }
    }
    // Table 326 容器层互斥：按对象号升序复核。容器与 `/Stm` 对象的键形态
    // 已由逐子项定位报出，此处只对「仅被 OBJR 声称」的对象补双键检查，
    // 并对「容器 ∩ OBJR 声称」报「不可兼得」。
    for entry in objects.values() {
        let Some(dict) = object_dict(document, entry.reference) else {
            continue;
        };
        let parents = dict.get("StructParents");
        let parent = dict.get("StructParent");
        if parents.is_some() && parent.is_some() && !entry.container {
            found.push(sweep_diagnostic(entry, StreamMcrRule::BothParentEntries));
        }
        if entry.container && entry.object_reference {
            found.push(sweep_diagnostic(
                entry,
                StreamMcrRule::ContainerAlsoObjectReferenced,
            ));
        }
    }
    Ok(found)
}

fn sweep_diagnostic(entry: &SweepEntry, rule: StreamMcrRule) -> StreamMcrDiagnostic {
    note(
        Spot {
            element: entry.claim.map(|(element, _)| element),
            kid: entry.claim.map(|(_, kid)| kid),
            page: None,
            mcid: None,
        },
        Some(entry.reference.0),
        None,
        rule,
    )
}
