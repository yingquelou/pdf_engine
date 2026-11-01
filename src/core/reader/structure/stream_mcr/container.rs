//! MCR 容器候选顺序（`/StmOwn` → `/Stm` → 生效页）、`/StmOwn` 所有者核对、
//! 公共类型（[`MarkedContentContainer`]、[`StreamMcrDiagnostic`]、
//! [`StreamMcrRule`]、[`MarkedContentParent`]）与对象/页面辅助函数。

use std::fmt;

use crate::core::types::{Dict, Document, Value};

use super::super::resolve_value;

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

/// `/StmOwn` 的所有者核对结果（库策略，见模块文档）。
pub(super) enum OwnerCheck {
    /// 非流字典且 `/AP` 引用了 `/Stm`。
    Conforming,
    /// 解引用后不是普通字典（流对象不能作所有者）。
    NotDictionary,
    /// `/AP` 的 `/N`（单引用或状态子字典）、`/D`、`/R` 都不引用 `/Stm`。
    AppearanceMismatch,
}

/// 与写端 `appearance_references`（builder/navigation/structure.rs）同口径。
pub(super) fn check_stream_owner(
    document: &Document,
    owner: (i64, i64),
    stream: (i64, i64),
) -> OwnerCheck {
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
pub(super) fn read_object(document: &Document, reference: (i64, i64)) -> Option<Value> {
    resolve_value(
        document,
        &Value::Ref(reference.0, reference.1),
        &format!("object {}", reference.0),
    )
    .ok()
}

/// 对象（字典或流）的字典；对象缺失或既非字典也非流返回 `None`。
pub(super) fn object_dict(document: &Document, reference: (i64, i64)) -> Option<Dict> {
    match read_object(document, reference)? {
        Value::Dict(dict) => Some(dict),
        Value::Stream(dict, _) => Some(dict),
        _ => None,
    }
}

/// 页对象号 → 页索引（0 基）；忽略 generation，页不在页面树中返回 `None`。
pub(super) fn page_index(document: &Document, page: Option<(i64, i64)>) -> Option<usize> {
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
