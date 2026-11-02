/// 一条 Artifact 归属发现。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtifactAttributionViolation {
    /// 所属页索引（0 基）。
    pub page: usize,
    /// [`Document::page_marked_content`] 返回序列中的区段下标；与页区段
    /// 无关时为 `None`。
    pub section: Option<usize>,
    /// 结构元素的 arena 下标；与元素无关或父树/结构树定位不到时为 `None`。
    pub element: Option<usize>,
    /// 该元素 `/K` 中的子项位置；与子项无关时为 `None`。
    pub kid: Option<usize>,
    /// [`Document::page_annotations`] 返回序列中的注解下标（即本页
    /// `/Annots` 数组下标）；与具体注解无关或定位不到时为 `None`。
    pub annotation: Option<usize>,
    /// 违反的规则。
    pub rule: ArtifactAttributionRule,
}

/// 违反的归属规则。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactAttributionRule {
    /// `Artifact` 区段的属性表携带合法 `/MCID`（14.8.2.3.1，shall）。
    ArtifactMarkedAsStructure,
    /// 结构元素的 MCID 子项归属到本页的 `Artifact` 区段（14.8.2.2.1，shall）。
    ArtifactInLogicalStructure,
    /// 真实内容区段（含祖先链）没有任何合法 `/MCID`（14.8.2.2.1，should）。
    RealContentNotInStructure,
    /// `TagSuspect` 区段没有属性表，或其 `TagSuspect` 条目不是名字
    /// `Ordering`（14.8.2.3.1，shall）。
    TagSuspectMissingProperty,
    /// 本页含 `TagSuspect` 区段但目录 `/MarkInfo` 没有 `/Suspects true`
    /// （14.8.2.3.1 与 Table 321，shall）。
    TagSuspectSuspectsFlagNotSet,
    /// 注解有含常态（`/N`）外观的外观流且 `/F` 未置 Hidden，却未被任何
    /// 结构元素以 OBJR 声称（14.8.2.2.1，should）。
    VisibleAnnotationNotInStructure,
    /// 结构元素以 `/K` 的 OBJR 声称本页注解，但注解字典缺少
    /// `/StructParent` 整数键，或父树该键的条目不是指向该元素的单元
    /// 间接引用（14.7.4.3、14.7.4.4 与 Table 325/326，shall）。
    AnnotationObjrParentTreeMismatch,
}

impl ArtifactAttributionRule {
    /// 该规则是否为规范强制（shall）；否则为规范建议（should）。
    pub fn is_mandatory(&self) -> bool {
        !matches!(
            self,
            Self::RealContentNotInStructure | Self::VisibleAnnotationNotInStructure
        )
    }
}
