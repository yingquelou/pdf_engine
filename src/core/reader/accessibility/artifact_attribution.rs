//! Artifact 与逻辑结构的归属校验（ISO 32000-1 14.8.2.2.1、14.8.2.2.2、14.8.2.3.1）。
//!
//! 14.8.2.2.1 规定逻辑结构 encompassed 所有真实内容的图形对象，但**不**包含
//! 版面与生产过程残留的 artifact；14.8.2.3.1 进一步规定 artifact 出现在页面
//! 内容序列中、却**不**进入逻辑结构序列。本模块据此做**诊断**：
//! [`Document::artifact_attribution_violations`] 返回发现列表，读取阶段不因
//! 归属问题报错，由调用方决定是否处理（与 14.8.4 的
//! [`content_model`](crate::core::reader::structure::content_model_violations)
//! 诊断同一形态）。
//!
//! 七条规则与 shall/should 划分：
//!
//! - [`ArtifactAttributionRule::ArtifactMarkedAsStructure`]（shall，
//!   14.8.2.3.1）：`Artifact` 区段的属性表（Table 330 之外还混入了结构条目）
//!   携带合法 `/MCID`，等于把 artifact 赋予了结构内容身份；
//! - [`ArtifactAttributionRule::ArtifactInLogicalStructure`]（shall，
//!   14.8.2.2.1）：本页某结构元素的 `/K` 子项（整数 MCID 或 Table 324 的
//!   MCR）指向的 MCID 恰好被页上的 `Artifact` 区段使用；只有**生效页**为本页
//!   的子项参与判定（MCR 的 `/Pg` 覆盖元素的 `/Pg`，整数 MCID 子项沿用元素
//!   的 `/Pg`）；MCR 带 `/Stm` 或 `/StmOwn` 时序列不在页面流中，不做归属；
//! - [`ArtifactAttributionRule::RealContentNotInStructure`]（should，
//!   14.8.2.2.1）：非 `Artifact` 区段沿 [`MarkedSection::parent`] 链向外
//!   （含自身）都找不到合法 `/MCID`，真实内容未进入逻辑结构。
//! - [`ArtifactAttributionRule::TagSuspectMissingProperty`]（shall，
//!   14.8.2.3.1）：标签为精确 `TagSuspect` 的区段没有属性表、属性表缺
//!   `TagSuspect` 条目，或条目值不是名字 `Ordering`。
//! - [`ArtifactAttributionRule::TagSuspectSuspectsFlagNotSet`]（shall，
//!   14.8.2.3.1）：本页含 `TagSuspect` 区段，但目录 `/MarkInfo` 没有
//!   `/Suspects true`。
//! - [`ArtifactAttributionRule::VisibleAnnotationNotInStructure`]（should，
//!   14.8.2.2.1）：注解有含常态（`/N`）外观的外观流、`/F` 未置 Hidden 位，
//!   却没有任何结构元素的 `/K` 以 OBJR 声称它为内容项——「注解属于真实
//!   内容」三个条件里前两条成立、第三条（进入逻辑结构）缺失；
//! - [`ArtifactAttributionRule::AnnotationObjrParentTreeMismatch`]（shall，
//!   14.7.4.3、14.7.4.4 与 Table 325/326）：某结构元素的 `/K` 以 OBJR 引用
//!   本页注解，但注解字典没有整数 `/StructParent` 键，或该键在父树中的
//!   条目不是指向**该元素**的裸（单元）间接引用。
//!
//! 后两条依据 14.8.2.3.1 原文（tag suspects 自 PDF 1.6 引入）：
//!
//! > In some situations, conforming writer may be unable to determine correct
//! > page content order for part of a document's contents. In such cases, tag
//! > suspects (PDF 1.6) can be used. The conforming writer shall identify
//! > suspect content by using marked content (see 14.6, "Marked Content") with
//! > a tag of TagSuspect, as shown in next Example. The marked content shall
//! > have a properties dictionary with an entry whose name is TagSuspect and
//! > whose value is Ordering, which indicates that the ordering of the enclosed
//! > marked content does not meet Tagged PDF specifications.
//! > EXAMPLE  /TagSuspect <</TagSuspect /Ordering>> BDC .... % Problem page contents EMC
//! > Documents containing tag suspects shall contain a Suspects entry with a
//! > value of true in the mark information dictionary (see Table 321).
//!
//! 规则 6 依据 14.8.2.2.1 原文（注解参与真实内容的三个条件）：
//!
//! > A document's real content includes not only the page content stream and
//! > subsidiary form XObjects but also associated annotations that meet all of
//! > the following conditions:
//! > • The annotation has an appearance stream (see 12.5.5, "Appearance
//! > Streams") containing a normal (N) appearance.
//! > • The annotation's Hidden flag (see 12.5.3, "Annotation Flags") is not
//! > set.
//! > • The annotation is included in the document's logical structure (see
//! > 14.7, "Logical Structure").
//!
//! 该句是「真实内容包含……」的定义式表述，第三个条件本身也在条件里，
//! 没有独立的 shall，故规则 6 与同样派生自「逻辑结构 encompassed 所有
//! 真实内容」一句的规则 3 同列 should 级。规则 7 的 shall 形态取自
//! 14.7.4.4（并经 Table 326 与注解公共条目 Table 164 的
//! `/StructParent`）：
//!
//! > The tree shall contain an entry for each object that is a content item
//! > of at least one structure element .... The key for each entry shall be
//! > an integer given as the value of the StructParent or StructParents entry
//! > in the object (see Table 326). .... For an object identified as a content
//! > item by means of an object reference (see 14.7.4.3, "PDF Objects as
//! > Content Items"), the value shall be an indirect reference to the parent
//! > structure element.
//!
//! 注解规则（6/7）的生效范围：仅当目录存在 `/StructTreeRoot`（逻辑结构树
//! 读取成功）时评估。14.8.2 建立在 14.7 的逻辑结构框架之上（14.8.1：
//! "Tagged PDF (PDF 1.4) is a stylized use of PDF that builds on the logical
//! structure framework described in 14.7"），14.8.2.2.1 的第三个条件要求
//! 注解"included in the document's logical structure"；没有结构树的文档
//! 根本不存在可纳入的逻辑结构，把可见注解的存在记为注解归属发现只会
//! 造成噪音，且与规则 2 在结构树缺失时不评估的行为不一致。规则 6 判定
//! 「纳入逻辑结构」看 OBJR 的 `/Obj` 对象号是否等于注解的间接引用
//! （忽略 generation，与结构树其余引用的库内语义一致）；直接嵌入
//! `/Annots` 的非间接注解字典无法被 Table 325 要求的（间接）OBJR 声称，
//! 可见时同样记规则 6。注解读取失败（`/Annots` 损坏、注解字典非法、
//! 数量越限）一律降级为「规则 6/7 不产生任何发现」，诊断不因此报错。
//!
//! `/MCID` 的合法性与 [`span`](super::span) 模块共用同一取值规则
//! （[`span::mcid_value`](super::span)）：整数或整数值的有限浮点数。
//! 页内容归属只比页节点**对象号**（忽略 generation），与结构树其余
//! 引用的库内语义一致；表单 XObject 的内容流不会被递归解析（沿用
//! [`Document::page_marked_content`] 的行为）。
//!
//! 规则 3 的前提是 Tagged PDF（14.8.2）：对没有结构树的文档，它把每个没有
//! `/MCID` 的标记区段都记为 should 级发现，正好构成「内容已标记但未挂结构」
//! 的待补标签清单；调用方可先用 [`Document::mark_info`] 判断文档是否声明为
//! tagged，再决定是否采纳这类发现。
//!
//! 豁免：标签为精确 `TagSuspect` 的区段（14.8.2.3.1 的 PDF 1.6 tag
//! suspect）及其整个子树仍不参与 `RealContentNotInStructure` 判定——
//! 规范承认这段内容的页序本就不合 Tagged PDF，不能据此再判它「未入
//! 结构」。目录 `/MarkInfo /Suspects` 的交叉校验由
//! `TagSuspectSuspectsFlagNotSet` 承担：读 `/MarkInfo` 失败（缺失、条目
//! 非布尔、结构损坏）时一律按「未声明 tag suspects」处理，诊断本身
//! 从不因此报错。
//!
//! 输出顺序确定：先全部 `ArtifactMarkedAsStructure`（区段下标升序），再
//! `ArtifactInLogicalStructure`（元素 arena 下标、子项位置升序；同一 MCID
//! 命中多个 artifact 区段时取最小下标），再 `RealContentNotInStructure`
//! （区段下标升序），再 `TagSuspectMissingProperty`（区段下标升序）与
//! 至多一条 `TagSuspectSuspectsFlagNotSet`；最后追加两条注解归属规则：
//! `VisibleAnnotationNotInStructure`（注解在本页 `/Annots` 数组中的下标
//! 升序）与 `AnnotationObjrParentTreeMismatch`（元素 arena 下标、`/K`
//! 子项位置升序，与 `ArtifactInLogicalStructure` 同一约定）。

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use super::span::mcid_value;
use crate::core::reader::content::{MarkedContentError, MarkedSection};
use crate::core::reader::structure::{
    PageStructureParents, StructureError, StructureKid, StructureTree,
};
use crate::core::types::{Document, Value};

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

/// 读取归属诊断所需输入时的失败原因。
#[derive(Debug, Clone, PartialEq)]
pub enum ArtifactAttributionError {
    /// 页面标记内容无法配对或内容流无法解析。
    MarkedContent(MarkedContentError),
    /// 逻辑结构树损坏，无法执行规则 2。
    Structure(StructureError),
}

impl fmt::Display for ArtifactAttributionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MarkedContent(error) => error.fmt(f),
            Self::Structure(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for ArtifactAttributionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::MarkedContent(error) => Some(error),
            Self::Structure(error) => Some(error),
        }
    }
}

impl From<MarkedContentError> for ArtifactAttributionError {
    fn from(error: MarkedContentError) -> Self {
        Self::MarkedContent(error)
    }
}

impl From<StructureError> for ArtifactAttributionError {
    fn from(error: StructureError) -> Self {
        Self::Structure(error)
    }
}

impl Document {
    /// 按 14.8.2 校验一页上 artifact、真实内容与注解同逻辑结构的归属，
    /// 返回全部发现。
    ///
    /// 七条规则的判定见模块文档；`section`、`element`、`kid`、`annotation`
    /// 按规则填充，定位不到的层级为 `None`。页码越界或页面内容流损坏返回
    /// [`ArtifactAttributionError::MarkedContent`]；结构树损坏返回
    /// [`ArtifactAttributionError::Structure`]。父树或结构树**缺失**
    /// （而非损坏）不报错：规则 1 的 `element` 降级为 `None`，规则 2 与
    /// 注解规则 6/7 自然没有可评估的结构（结构树缺失时注解规则整组跳过，
    /// 见模块文档的生效范围）；`/MarkInfo` 读取失败只按「未声明 tag
    /// suspects」处理；注解读取失败只让规则 6/7 不产出发现。
    pub fn artifact_attribution_violations(
        &self,
        page_index: usize,
    ) -> Result<Vec<ArtifactAttributionViolation>, ArtifactAttributionError> {
        let sections = self.page_marked_content(page_index)?;
        let mut violations = Vec::new();
        // 父树与结构树是可选增强：缺失或损坏只影响 element 定位，不报错。
        let page_parents = self.page_struct_parents(page_index).ok().flatten();
        self.collect_artifact_identity_violations(
            page_index,
            &sections,
            &page_parents,
            &mut violations,
        );
        // 结构树整次诊断只取一次：损坏仍按原规则 2 报 Structure 错误，
        // 缺失（`Ok(None)`）时规则 2 与注解规则 6/7 不评估。
        let tree = self.structure_tree()?;
        self.collect_structure_attribution_violations(
            page_index,
            &sections,
            tree.as_ref(),
            &mut violations,
        );
        self.collect_unstructured_content_violations(page_index, &sections, &mut violations);
        self.collect_tag_suspect_violations(page_index, &sections, &mut violations);
        self.collect_annotation_attribution_violations(page_index, tree.as_ref(), &mut violations);
        Ok(violations)
    }

    /// 规则 1：`Artifact` 区段携带合法 `/MCID`。
    fn collect_artifact_identity_violations(
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

    /// 规则 2：本页结构元素的 MCID 子项归属到 `Artifact` 区段。
    fn collect_structure_attribution_violations(
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

    /// 规则 3：真实内容区段（含祖先链）没有任何合法 `/MCID`。
    fn collect_unstructured_content_violations(
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
    fn collect_tag_suspect_violations(
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

    /// 规则 6/7：注解与逻辑结构的归属（14.8.2.2.1 与 14.7.4.3–14.7.4.4）。
    ///
    /// 仅当目录存在可读的逻辑结构树时评估；注解读取失败一律降级为
    /// 「不产生这两条规则的发现」，与模块文档的生效范围一致。
    fn collect_annotation_attribution_violations(
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
