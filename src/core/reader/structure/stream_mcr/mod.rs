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
//! - `/Stm` 解引用后应是流对象：Table 324 的类型类型列写 `stream` 但没有 shall
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

mod conflicts;
mod container;
mod parent;
#[cfg(test)]
mod tests;

pub use self::container::MarkedContentParent;
pub use self::container::{MarkedContentContainer, StreamMcrDiagnostic, StreamMcrRule};

use crate::core::types::Document;

use super::{StructureError, StructureTree};

/// 一条发现的定位坐标。
#[derive(Debug, Clone, Copy)]
pub(super) struct Spot {
    pub element: Option<usize>,
    pub kid: Option<usize>,
    pub page: Option<usize>,
    pub mcid: Option<i64>,
}

pub(super) fn note(
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
    parent::marked_content_parent(document, tree, element_index, kid_index)
}

/// 全树扫描：流形态 MCR 的定位诊断 + Table 326 容器层互斥诊断。
pub(super) fn diagnostics(document: &Document) -> Result<Vec<StreamMcrDiagnostic>, StructureError> {
    conflicts::diagnostics(document)
}
