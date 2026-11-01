//! 流形态 MCR 的父树定位与 Table 326 容器层互斥校验
//! （ISO 32000-1 14.7.4.2、14.7.4.3、14.7.4.4）。
//!
//! 与 [`content_model`](super::content_model) 同一形态：只出**诊断**，读取
//! 阶段不因这些发现报错，也不改变已解析的结构树（解析层已经给出的
//! [`StructureError::Malformed`] 维持既有行为）。
//!
//! # 规范原文要求（shall）
//!
//! Table 324（raw 29460–29481 行）：
//! - `/Pg`「This entry **overrides any Pg entry in the structure element**
//!   containing the marked-content reference; it shall be required if the
//!   structure element has no such entry」（29464–29468 行）——本模块按
//!   `reference.page.or(element.page)` 取生效页，与
//!   [`artifact_attribution`](crate::core::reader::accessibility::artifact_attribution)
//!   已有的覆盖口径一致；
//! - `/Stm`「(Optional; shall be an indirect reference) The content stream
//!   containing the marked-content sequence」，并附 shall「If this entry is
//!   absent, the marked-content sequence **shall be contained in the content
//!   stream of the page identified by Pg**（either in the marked-content
//!   reference dictionary or in the parent structure element）」
//!   （29469–29476 行）——故无 `/Stm` 时容器回落页面；
//! - `/StmOwn`「(Optional; shall be an indirect reference) The PDF object
//!   owning the stream identified by Stm」（29477–29479 行）；
//! - `/MCID`「(Required) The marked-content identifier of the marked-content
//!   sequence **within its content stream**」（29480–29481 行）——索引的是
//!   序列所在流，不是页面上的全局 MCID。
//!
//! Table 326（raw 29690–29699 行）：`/StructParent` 用于「objects that are
//! structural content items」，`/StructParents` 用于「content streams
//! containing marked-content sequences that are structural content items」，
//! 随后是 shall「**At most one of these two entries shall be present in a
//! given object. An object may be either a content item in its entirety or a
//! container for marked-content sequences that are content items, but not
//! both.**」——本模块把这条 shall 的互斥校验从页面层（见
//! [`Document::page_struct_parents`]）扩展到 `/Stm`、`/StmOwn` 与 OBJR
//! `/Obj` 指向的任意对象。
//!
//! 14.7.4.4 的检索法（raw 29721–29727 行）：「Because a marked-content
//! sequence is not an object in its own right, its parent tree key **shall be
//! found in the StructParents entry of the page object or other content stream
//! in which the sequence resides**. The value retrieved from the parent tree
//! shall not be a reference to the parent structure element itself but **to an
//! array of such references**--one for each marked-content sequence contained
//! within that content stream. The parent structure element for the given
//! sequence **shall be found by using the sequence's marked-content identifier
//! as an index into this array**」——对应
//! [`StreamMcrRule::MissingStructParents`]（键无处可取）、
//! [`StreamMcrRule::ParentTreeKeyMissing`]（键在父树中不存在）、
//! [`StreamMcrRule::ParentEntryNotMcidArray`]（条目是单元引用而非数组）、
//! [`StreamMcrRule::McidSlotEmpty`] 与
//! [`StreamMcrRule::McidIndexOutOfBounds`]。
//!
//! # 库自定策略（规范只有 description/should，或本库比 shall 更窄/更宽）
//!
//! - **容器层级取「承载 `/StructParents` 的字典」**：写端
//!   `write_stream_parents`（`src/core/builder/navigation/structure/stream_mcr.rs`）把键
//!   写在**所有者字典**上——有 `/StmOwn` 时是注解字典，无 `/StmOwn` 时是
//!   `/Stm` 流字典自身；注解的外观流上**不写**键（见写端往返测试
//!   `builder_stream_marked_content_roundtrip_links_stm_stmown_and_container_keys`
//!   对 `/AP /N` 对象 `StructParents == None` 的断言）。因此候选顺序是
//!   `/StmOwn` → `/Stm` → 生效页；候选都带键时取第一个（所有者），只读
//!   `/Stm` 才带键的「规范字面读法」也接受（比写端宽）。
//! - `/StmOwn` 的所有者必须**是非流字典**，且其 `/AP`（`/N` 单引用或 `/N`
//!   状态子字典、`/D`、`/R`）**确实引用了 `/Stm`**：Table 324 对 `/StmOwn`
//!   只有「the PDF object owning the stream … an annotation to which an
//!   appearance stream belongs」这句 description，没有 shall 级校验条目；本库
//!   与写端 `validate_stream_mcr_targets` / `appearance_references` 同口径
//!   核对，故 [`StreamMcrRule::OwnerNotDictionary`] 与
//!   [`StreamMcrRule::OwnerAppearanceMismatch`] 归**库策略**而非 shall。
//! - `/StmOwn` 出现但没有 `/Stm` 时「owning the stream identified by Stm」
//!   无所指（[`StreamMcrRule::OwnerWithoutStream`]，库策略）。
//! - `/Stm` 解引用后应是流对象：Table 324 的类型列写 `stream` 但没有 shall
//!   语句，故 [`StreamMcrRule::StmTargetNotStream`] 归**库策略**；本库**不**
//!   要求 `/Subtype /Form`（那是写端「句柄→内容流」通道带来的收窄，页面内容
//!   流与某些外观流都可能没有该条目）。
//! - `/StructParent` 与 `/StructParents` 之外的形态（键为负数、非整数）按
//!   Table 326 的类型列判为 shall 级非法；只读、只报告：定位失败不返回
//!   `Err`，`parent`/`parents_key` 留空并给出结构化发现，不 panic。
//!
//! 输出顺序确定：[`Document::stream_mcr_diagnostic`] 先按「元素 arena 下标、
//! `/K` 子项位置」升序给出流形态 MCR 的定位诊断，再按**对象号**升序给出
//! Table 326 的容器层互斥诊断（`/Stm`、`/StmOwn`、`/Obj` 指向的对象；已由
//! 逐子项定位报出的容器对象不重复报双键）。

use std::collections::BTreeMap;
use std::fmt;

use crate::core::types::{Dict, Document, Value};

use super::{
    MarkedContentReference, ParentTreeEntry, StructureElement, StructureError, StructureKid,
    StructureTree, malformed, resolve_value,
};

/// MCR 序列的父树容器层级（Table 324 + 14.7.4.4）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkedContentContainer {
    /// 页字典：`/Stm` 缺省，序列在生效页的内容流里（14.7.4.4「the page
    /// object」）。
    Page,
    /// `/Stm` 所指内容流对象：无 `/StmOwn` 时写端把键写在该流字典上。
    Stream,
    /// `/StmOwn` 所指所有者对象（注解字典）：写端把键写在所有者字典上。
    Owner,
}

/// 一条流形态 MCR / Table 326 容器层的发现。
#[derive(Debug, Clone, PartialEq)]
pub struct StreamMcrDiagnostic {
    /// 声称该序列的结构元素 arena 下标；与被某个对象无关的定位时为 `None`。
    pub element: Option<usize>,
    /// 该元素 `/K` 中的子项位置；与子项无关时为 `None`。
    pub kid: Option<usize>,
    /// 相关对象的对象号（容器、`/Stm`、`/StmOwn` 或 OBJR 的 `/Obj` 目标）。
    pub object: Option<i64>,
    /// 生效页索引（0 基）；MCR 与元素都没有可读 `/Pg`、或页不在页面树中时
    /// 为 `None`。
    pub page: Option<usize>,
    /// 相关 `/MCID`；与序列无关时为 `None`。
    pub mcid: Option<i64>,
    /// 相关父树键（`/StructParents` 的值）；键取不到时为 `None`。
    pub key: Option<i64>,
    /// 命中的规则。
    pub rule: StreamMcrRule,
}

/// 违反的规则；[`StreamMcrRule::is_mandatory`] 区分 shall 与库策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamMcrRule {
    /// Table 326 shall：同一对象同时带 `/StructParent` 与 `/StructParents`
    /// （raw 29697–29699 行「At most one of these two entries shall be
    /// present in a given object」）。
    BothParentEntries,
    /// Table 326 shall：容器只有 `/StructParent`，即该对象被当作「整体上的
    /// 内容项」，与「承载标记内容序列的容器」不可兼得（「not both」）。
    ContainerIsContentItem,
    /// Table 326 shall：同一对象既是流形态 MCR 的容器，又被某结构元素的
    /// OBJR `/Obj` 声称。
    ContainerAlsoObjectReferenced,
    /// 14.7.4.4 shall：容器与回落候选都没有 `/StructParents`，父树键无处可
    /// 取（raw 29722–29723 行）。
    MissingStructParents,
    /// Table 326 shall：`/StructParents` 不是非负整数（类型列写 `integer`）。
    StructParentsNotInteger,
    /// 14.7.4.4 shall：父树里没有该键的条目（与「容器缺 `/StructParents`」
    /// 是两种不同的失败）。
    ParentTreeKeyMissing,
    /// 14.7.4.4 shall：MCID 检索命中的父树条目是 OBJR 用的单元引用，而不是
    /// 「an array of such references」（raw 29724–29725 行）。
    ParentEntryNotMcidArray,
    /// 14.7.4.4 shall：父树数组在该 MCID 处是 `null` 空位，序列没有父元素。
    McidSlotEmpty,
    /// 14.7.4.4 shall：`/MCID` 为负或超出该容器父树数组的长度。
    McidIndexOutOfBounds,
    /// 库策略：`/StmOwn` 解引用后不是普通字典（流对象不能作外观流的所有者）。
    OwnerNotDictionary,
    /// 库策略：`/StmOwn` 所有者的 `/AP`（`/N` 单引用、`/N` 状态子字典、`/D`、
    /// `/R`）没有任何引用指向 `/Stm`，即该对象不「拥有」这个流。
    OwnerAppearanceMismatch,
    /// 库策略：`/StmOwn` 出现但没有 `/Stm`，「owning the stream identified
    /// by Stm」无所指。
    OwnerWithoutStream,
    /// 库策略：`/Stm` 解引用后不是流对象（Table 324 的类型列是 `stream`）。
    StmTargetNotStream,
}

impl StreamMcrRule {
    /// 该规则是否为规范强制（shall）；否则为库自定策略。
    pub fn is_mandatory(&self) -> bool {
        !matches!(
            self,
            Self::OwnerNotDictionary
                | Self::OwnerAppearanceMismatch
                | Self::OwnerWithoutStream
                | Self::StmTargetNotStream
        )
    }
}

impl fmt::Display for StreamMcrRule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::BothParentEntries => "both-struct-parent-entries",
            Self::ContainerIsContentItem => "container-is-content-item",
            Self::ContainerAlsoObjectReferenced => "container-also-object-referenced",
            Self::MissingStructParents => "missing-struct-parents",
            Self::StructParentsNotInteger => "struct-parents-not-integer",
            Self::ParentTreeKeyMissing => "parent-tree-key-missing",
            Self::ParentEntryNotMcidArray => "parent-entry-not-mcid-array",
            Self::McidSlotEmpty => "mcid-slot-empty",
            Self::McidIndexOutOfBounds => "mcid-index-out-of-bounds",
            Self::OwnerNotDictionary => "stmown-not-a-dictionary",
            Self::OwnerAppearanceMismatch => "stmown-appearance-does-not-match-stm",
            Self::OwnerWithoutStream => "stmown-without-stm",
            Self::StmTargetNotStream => "stm-target-is-not-a-stream",
        };
        f.write_str(name)
    }
}

/// 一个 MCR 子项（Table 324）的父树定位结果。
#[derive(Debug, Clone, PartialEq)]
pub struct MarkedContentParent {
    /// `/MCID`（Table 324 的 Required 条目）。
    pub mcid: i64,
    /// 生效页引用：MCR 的 `/Pg` 覆盖元素的 `/Pg`。
    pub page: Option<(i64, i64)>,
    /// 序列所在的内容流（`/Stm`）；页面层为 `None`。
    pub stream: Option<(i64, i64)>,
    /// 承载 `/StructParents` 键的容器对象引用；键取不到时为主候选对象。
    pub container: Option<(i64, i64)>,
    /// 容器层级（与 `container` 同步；键取不到时为主候选的层级）。
    pub level: MarkedContentContainer,
    /// 容器字典 `/StructParents` 的父树键；缺键或键形态非法时为 `None`。
    pub parents_key: Option<i64>,
    /// 用 `/MCID` 索引父树数组得到的父元素 arena 下标；定位不到为 `None`。
    pub parent: Option<usize>,
    /// 定位过程中的结构化发现（正路时为空）。
    pub diagnostics: Vec<StreamMcrDiagnostic>,
}

/// 一条发现的定位坐标。
#[derive(Debug, Clone, Copy)]
struct Spot {
    element: Option<usize>,
    kid: Option<usize>,
    page: Option<usize>,
    mcid: Option<i64>,
}

fn note(
    spot: Spot,
    object: Option<i64>,
    key: Option<i64>,
    rule: StreamMcrRule,
) -> StreamMcrDiagnostic {
    StreamMcrDiagnostic {
        element: spot.element,
        kid: spot.kid,
        object,
        page: spot.page,
        mcid: spot.mcid,
        key,
        rule,
    }
}

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
        && !matches!(read_object(document, stream_ref), Some(Value::Stream(..)))
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
            Some(ParentTreeEntry::Object(_)) => diagnostics.push(note(
                spot,
                container_number,
                Some(key),
                StreamMcrRule::ParentEntryNotMcidArray,
            )),
            Some(ParentTreeEntry::Page(parents)) => match usize::try_from(reference.mcid) {
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
            },
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

/// `/StmOwn` 的所有者核对结果（库策略，见模块文档）。
enum OwnerCheck {
    /// 非流字典且 `/AP` 引用了 `/Stm`。
    Conforming,
    /// 解引用后不是普通字典（流对象不能作所有者）。
    NotDictionary,
    /// `/AP` 的 `/N`（单引用或状态子字典）、`/D`、`/R` 都不引用 `/Stm`。
    AppearanceMismatch,
}

/// 与写端 `appearance_references`（builder/navigation/structure.rs）同口径。
fn check_stream_owner(document: &Document, owner: (i64, i64), stream: (i64, i64)) -> OwnerCheck {
    let Some(Value::Dict(dict)) = read_object(document, owner) else {
        return OwnerCheck::NotDictionary;
    };
    if appearance_references(document, &dict, stream.0) {
        OwnerCheck::Conforming
    } else {
        OwnerCheck::AppearanceMismatch
    }
}

/// 所有者字典的 `/AP` 是否引用对象号 `stream_num` 的外观流：`/N` 可以是单条
/// 流引用，也可以是「状态名 → 流引用」子字典，`/D`、`/R` 为单引用；`/AP`
/// 本身允许间接引用（读端容错，写端只内联字典）。
fn appearance_references(document: &Document, dict: &Dict, stream_num: i64) -> bool {
    let Some(value) = dict.get("AP") else {
        return false;
    };
    let Ok(resolved) = resolve_value(document, value, "MCR /StmOwn /AP") else {
        return false;
    };
    let Some(appearance) = resolved.as_dict() else {
        return false;
    };
    ["N", "D", "R"].iter().any(|key| match appearance.get(key) {
        Some(Value::Ref(number, _)) => *number == stream_num,
        Some(Value::Dict(states)) => states
            .0
            .values()
            .any(|value| matches!(value, Value::Ref(number, _) if *number == stream_num)),
        _ => false,
    })
}

/// 解引用对象；读不到（悬空引用）返回 `None`。
fn read_object(document: &Document, reference: (i64, i64)) -> Option<Value> {
    resolve_value(
        document,
        &Value::Ref(reference.0, reference.1),
        &format!("object {}", reference.0),
    )
    .ok()
}

/// 对象（字典或流）的字典；对象缺失或既非字典也非流返回 `None`。
fn object_dict(document: &Document, reference: (i64, i64)) -> Option<Dict> {
    match read_object(document, reference)? {
        Value::Dict(dict) => Some(dict),
        Value::Stream(dict, _) => Some(dict),
        _ => None,
    }
}

/// 页对象号 → 页索引（0 基）；忽略 generation，页不在页面树中返回 `None`。
fn page_index(document: &Document, page: Option<(i64, i64)>) -> Option<usize> {
    let (number, _) = page?;
    let count = document.page_count().ok()?;
    (0..count).find(|index| {
        document
            .locate_page_node_with_ref(*index)
            .ok()
            .and_then(|(_, reference)| reference)
            .is_some_and(|(found, _)| found == number)
    })
}

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
                    let resolved =
                        marked_content_parent(document, &tree, element_index, kid_index)?;
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
