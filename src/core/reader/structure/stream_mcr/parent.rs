//! `marked_content_parent` 入口与 `resolve_parent`：按规范顺序定位一个 MCR
//! 的内容流 → 容器 → `/StructParents` 键 → 父树数组。

use crate::core::types::{Document, Value};

use super::super::{
    MarkedContentReference, StructureElement, StructureError, StructureKid, StructureTree,
    malformed,
};
use super::container::{
    MarkedContentContainer, MarkedContentParent, OwnerCheck, StreamMcrDiagnostic, StreamMcrRule,
    check_stream_owner, object_dict, page_index,
};
use super::{Spot, note};

/// 解析 `element` 的 `/K` 子项 `kid`（MCR）的容器、父树键与父元素。
pub(super) fn marked_content_parent(
    document: &Document,
    tree: &StructureTree,
    element_index: usize,
    kid_index: usize,
) -> Result<Option<MarkedContentParent>, StructureError> {
    let element = tree
        .element(element_index)
        .ok_or_else(|| malformed(format!("structure element {element_index} is out of range")))?;
    let reference = match element.kids.get(kid_index) {
        Some(StructureKid::MarkedContent(reference)) => reference.clone(),
        // 整数 MCID、子元素或 OBJR：不是 MCR 形态，交回调用方按其他通道处理。
        Some(_) => return Ok(None),
        None => {
            return Err(malformed(format!(
                "structure element {element_index} has no kid {kid_index}"
            )));
        }
    };
    Ok(Some(resolve_parent(
        document,
        tree,
        element,
        &reference,
        Some(element_index),
        Some(kid_index),
    )))
}

/// 按规范顺序定位一个 MCR：内容流 → 容器 → `/StructParents` 键 → 父树数组。
fn resolve_parent(
    document: &Document,
    tree: &StructureTree,
    element: &StructureElement,
    reference: &MarkedContentReference,
    element_index: Option<usize>,
    kid_index: Option<usize>,
) -> MarkedContentParent {
    // Table 324：MCR 的 `/Pg` 覆盖元素的 `/Pg`（解析层已保证两者至少其一）。
    let page = reference.page.or(element.page);
    let spot = Spot {
        element: element_index,
        kid: kid_index,
        page: page_index(document, page),
        mcid: Some(reference.mcid),
    };
    let mut diagnostics: Vec<StreamMcrDiagnostic> = Vec::new();
    let stream = reference.stream;
    let owner = reference.stream_own;

    if owner.is_some() && stream.is_none() {
        diagnostics.push(note(
            spot,
            owner.map(|(number, _)| number),
            None,
            StreamMcrRule::OwnerWithoutStream,
        ));
    }
    if let (Some(owner_ref), Some(stream_ref)) = (owner, stream) {
        match check_stream_owner(document, owner_ref, stream_ref) {
            OwnerCheck::Conforming => {}
            OwnerCheck::NotDictionary => diagnostics.push(note(
                spot,
                Some(owner_ref.0),
                None,
                StreamMcrRule::OwnerNotDictionary,
            )),
            OwnerCheck::AppearanceMismatch => diagnostics.push(note(
                spot,
                Some(owner_ref.0),
                None,
                StreamMcrRule::OwnerAppearanceMismatch,
            )),
        }
    }
    if let Some(stream_ref) = stream
        && !matches!(
            super::container::read_object(document, stream_ref),
            Some(Value::Stream(..))
        )
    {
        diagnostics.push(note(
            spot,
            Some(stream_ref.0),
            None,
            StreamMcrRule::StmTargetNotStream,
        ));
    }

    // 容器候选（模块文档的库策略顺序）：`/StmOwn` 所有者 → `/Stm` 流 → 生效页。
    let candidates: Vec<(MarkedContentContainer, (i64, i64))> = match (owner, stream, page) {
        (Some(owner), _, _) => {
            let mut candidates = vec![(MarkedContentContainer::Owner, owner)];
            if let Some(stream) = stream {
                candidates.push((MarkedContentContainer::Stream, stream));
            }
            candidates
        }
        (None, Some(stream), _) => vec![(MarkedContentContainer::Stream, stream)],
        (None, None, Some(page)) => vec![(MarkedContentContainer::Page, page)],
        (None, None, None) => Vec::new(),
    };
    let primary = candidates.first().copied();
    let mut key_of: Option<(i64, MarkedContentContainer, (i64, i64))> = None;
    // 任一候选带 `/StructParent` 或 `/StructParents` 即置真：缺键诊断只用于
    // 「两个条目都没有」的容器，形态非法与「内容项兼容器」已各有规则。
    let mut carries_entry = false;
    for (level, candidate) in &candidates {
        let Some(dict) = object_dict(document, *candidate) else {
            continue;
        };
        let parents = dict.get("StructParents");
        let parent = dict.get("StructParent");
        carries_entry |= parents.is_some() || parent.is_some();
        // 双键违反 Table 326 的「at most one」shall，无论键最终取自哪个候选。
        if parents.is_some() && parent.is_some() {
            diagnostics.push(note(
                spot,
                Some(candidate.0),
                None,
                StreamMcrRule::BothParentEntries,
            ));
        }
        match parents {
            Some(Value::Integer(key)) if *key >= 0 && key_of.is_none() => {
                key_of = Some((*key, *level, *candidate));
            }
            Some(_) => {
                // 主候选的键形态非法即无处可取；回落候选只在尚未取到键时报。
                if key_of.is_none() {
                    diagnostics.push(note(
                        spot,
                        Some(candidate.0),
                        None,
                        StreamMcrRule::StructParentsNotInteger,
                    ));
                }
            }
            None => {
                // 只有「应当承载键的主候选」才按内容项/缺键定性（库策略顺序）。
                if primary.is_some_and(|(primary_level, primary_ref)| {
                    primary_level == *level && primary_ref == *candidate
                }) && parent.is_some()
                {
                    diagnostics.push(note(
                        spot,
                        Some(candidate.0),
                        None,
                        StreamMcrRule::ContainerIsContentItem,
                    ));
                }
            }
        }
    }

    let mut container = primary.map(|(_, reference)| reference);
    let mut level = primary
        .map(|(level, _)| level)
        .unwrap_or(MarkedContentContainer::Page);
    let mut parents_key = None;
    match key_of {
        Some((key, key_level, key_reference)) => {
            container = Some(key_reference);
            level = key_level;
            parents_key = Some(key);
        }
        None => {
            if !carries_entry {
                diagnostics.push(note(
                    spot,
                    container.map(|(number, _)| number),
                    None,
                    StreamMcrRule::MissingStructParents,
                ));
            }
        }
    }

    let mut parent = None;
    let container_number = container.map(|(number, _)| number);
    if let Some(key) = parents_key {
        match tree.parent_tree_entry(key) {
            None => diagnostics.push(note(
                spot,
                container_number,
                Some(key),
                StreamMcrRule::ParentTreeKeyMissing,
            )),
            Some(super::super::ParentTreeEntry::Object(_)) => diagnostics.push(note(
                spot,
                container_number,
                Some(key),
                StreamMcrRule::ParentEntryNotMcidArray,
            )),
            Some(super::super::ParentTreeEntry::Page(parents)) => {
                match usize::try_from(reference.mcid) {
                    Ok(index) => match parents.get(index) {
                        Some(Some(index)) => parent = Some(*index),
                        Some(None) => diagnostics.push(note(
                            spot,
                            container_number,
                            Some(key),
                            StreamMcrRule::McidSlotEmpty,
                        )),
                        None => diagnostics.push(note(
                            spot,
                            container_number,
                            Some(key),
                            StreamMcrRule::McidIndexOutOfBounds,
                        )),
                    },
                    Err(_) => diagnostics.push(note(
                        spot,
                        container_number,
                        Some(key),
                        StreamMcrRule::McidIndexOutOfBounds,
                    )),
                }
            }
        }
    }

    MarkedContentParent {
        mcid: reference.mcid,
        page,
        stream,
        container,
        level,
        parents_key,
        parent,
        diagnostics,
    }
}
