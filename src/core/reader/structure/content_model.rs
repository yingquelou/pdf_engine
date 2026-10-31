//! 标准结构类型的内容模型校验（ISO 32000-1 14.8.4）。
//!
//! 14.8.4.3.5 明确 Tagged PDF 不强制元素顺序与嵌套的严格规则，各导出格式
//! 也各有约定；因此本模块只做**诊断**：[`StructureTree::content_model_violations`]
//! 返回发现列表，由调用方决定是否处理，读取阶段不因此报错，也不改变已有
//! 解析结果。
//!
//! 覆盖的规范强制项（shall）：
//! - 结构树根 `/K` 恰有一个顶层子项（14.8.4.2）；
//! - 分组元素仅用于分组，不得直接关联内容项（14.8.4.2 Table 333）；
//! - `Ruby` 须为 `RB` 后接 `RT` 或 `RP RT RP`（Table 339）；
//! - 无 `PrintField` `/Role` 的 `Form` 须恰有一个 OBJR 子项（Table 340）。
//!
//! 覆盖的表格描述与建议项（should）：
//! - `TOC`（TOCI/TOC）、`TOCI`（Lbl/Reference/NonStruct/P/TOC）、`L`（可选
//!   Caption + 一个以上 LI）、`LI`（Lbl/LBody）、`Table`（TR 或
//!   THead/TBody/TFoot，Caption 限首末）、`TR`（TH/TD）、`THead`/`TBody`/
//!   `TFoot`（TR）的子元素类型与必需子项（Table 333/336/337）；
//! - `Caption` 只允许位于 `L` 首位或 `Table` 首末（Table 336/337）；
//! - `Link` 不得嵌套 `Link` 且应带对象引用，`Annot` 不得嵌套 `Annot`
//!   （Table 338、14.8.4.4.2–14.8.4.4.3）。
//!
//! 未映射到标准名的元素（[`StandardStructureType::Other`]）不参与校验；
//! 元素总数与子项总数已由结构树解析阶段限制，本模块不另设上限。

use super::{StandardStructureType, StructureKid, StructureTree};

/// 一条内容模型发现。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentModelViolation {
    /// 违规结构元素的 arena 下标；顶层子项数量问题为 `None`。
    pub element: Option<usize>,
    /// 违规子项在父元素 `/K` 中的位置；缺少必需子项或序列级问题为 `None`。
    pub kid: Option<usize>,
    /// 违反的规则。
    pub rule: ContentModelRule,
}

/// 违反的内容模型规则。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentModelRule {
    /// 结构树根 `/K` 的顶层子项数不是 1（14.8.4.2，shall）。
    TopLevelCount(usize),
    /// 分组元素直接关联了内容项（14.8.4.2 Table 333，shall）。
    ContentItemInGroupingElement,
    /// 子项类型不在该结构类型的允许集合内（Table 333/336/337/338）。
    UnexpectedChild(ChildModel),
    /// 缺少该结构类型要求的子项（Table 336/337/338/340）。
    MissingChild(ChildModel),
    /// `Caption` 不在 `L` 首位或 `Table` 首末（Table 336/337）。
    CaptionPlacement,
    /// `Ruby` 的元素子项序列不是 `RB` + `RT` 或 `RB` + `RP RT RP`
    /// （Table 339，shall）。
    RubySequence,
    /// 无 `PrintField` `/Role` 的 `Form` 子项不是恰一个 OBJR
    /// （Table 340，shall）。
    FormObjectReference,
}

impl ContentModelRule {
    /// 该规则是否为规范强制（shall）；否则为表格描述或规范建议（should）。
    pub fn is_mandatory(&self) -> bool {
        matches!(
            self,
            Self::TopLevelCount(_)
                | Self::ContentItemInGroupingElement
                | Self::RubySequence
                | Self::FormObjectReference
        )
    }
}

/// 内容模型期望的子项类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChildModel {
    /// `TR`（Table 337）。
    TableRow,
    /// `TH` 或 `TD`（Table 337）。
    TableCell,
    /// `TR` 或 `THead`/`TBody`/`TFoot` 与可选 `Caption`（Table 337）。
    TableChild,
    /// `LI`（Table 336）。
    ListItem,
    /// `LI` 或首位 `Caption`（Table 336）。
    ListChild,
    /// `Lbl` 或 `LBody`（Table 336）。
    ListPart,
    /// `TOCI` 或嵌套 `TOC`（Table 333）。
    TocEntry,
    /// `Lbl`、`Reference`、`NonStruct`、`P` 或嵌套 `TOC`（Table 333）。
    TocItemChild,
    /// 内容项或其他 ILSE（`Link`/`Annot` 的子项约束）。
    ContentOrInline,
    /// OBJR 对象引用（14.8.4.4.2–14.8.4.4.3）。
    ObjectReference,
}

impl StructureTree {
    /// 按 14.8.4 内容模型检查结构树，返回全部发现。
    ///
    /// 发现按「顶层子项数量 → 元素 arena 顺序 → 子项位置」排列；强制项与
    /// 建议项都返回，调用方可用 [`ContentModelRule::is_mandatory`] 区分。
    /// 未映射到标准名的元素（未命中 `/RoleMap` 的非标准名）跳过。
    pub fn content_model_violations(&self) -> Vec<ContentModelViolation> {
        let mut violations = Vec::new();
        let top_level = self.root().kids.len();
        if top_level != 1 {
            violations.push(ContentModelViolation {
                element: None,
                kid: None,
                rule: ContentModelRule::TopLevelCount(top_level),
            });
        }
        for (index, element) in self.elements().iter().enumerate() {
            let Some(structure_type) = self.standard_type(index) else {
                continue;
            };
            if !structure_type.is_standard() {
                continue;
            }
            if structure_type.category() == Some(super::StructureTypeCategory::Grouping) {
                for (kid, child) in element.kids.iter().enumerate() {
                    if !matches!(child, StructureKid::Element(_)) {
                        violations.push(ContentModelViolation {
                            element: Some(index),
                            kid: Some(kid),
                            rule: ContentModelRule::ContentItemInGroupingElement,
                        });
                    }
                }
            }
            self.check_content_model(index, &structure_type, &mut violations);
        }
        violations
    }

    fn check_content_model(
        &self,
        index: usize,
        structure_type: &StandardStructureType,
        violations: &mut Vec<ContentModelViolation>,
    ) {
        let Some(element) = self.element(index) else {
            return;
        };
        let mut push = |kid: Option<usize>, rule: ContentModelRule| {
            violations.push(ContentModelViolation {
                element: Some(index),
                kid,
                rule,
            });
        };
        match structure_type {
            StandardStructureType::TableOfContents => {
                for (kid, child) in element.kids.iter().enumerate() {
                    if let StructureKid::Element(child) = child
                        && !matches!(
                            self.standard_type(*child),
                            Some(
                                StandardStructureType::TableOfContents
                                    | StandardStructureType::TableOfContentsItem
                            )
                        )
                    {
                        push(
                            Some(kid),
                            ContentModelRule::UnexpectedChild(ChildModel::TocEntry),
                        );
                    }
                }
            }
            StandardStructureType::TableOfContentsItem => {
                for (kid, child) in element.kids.iter().enumerate() {
                    if let StructureKid::Element(child) = child
                        && !matches!(
                            self.standard_type(*child),
                            Some(
                                StandardStructureType::Label
                                    | StandardStructureType::Reference
                                    | StandardStructureType::NonStruct
                                    | StandardStructureType::Paragraph
                                    | StandardStructureType::TableOfContents
                            )
                        )
                    {
                        push(
                            Some(kid),
                            ContentModelRule::UnexpectedChild(ChildModel::TocItemChild),
                        );
                    }
                }
            }
            StandardStructureType::List => {
                let mut has_item = false;
                for (kid, child) in element.kids.iter().enumerate() {
                    match child {
                        StructureKid::Element(child) => match self.standard_type(*child) {
                            Some(StandardStructureType::Caption) => {
                                if kid != 0 {
                                    push(Some(kid), ContentModelRule::CaptionPlacement);
                                }
                            }
                            Some(StandardStructureType::ListItem) => has_item = true,
                            _ => push(
                                Some(kid),
                                ContentModelRule::UnexpectedChild(ChildModel::ListChild),
                            ),
                        },
                        _ => push(
                            Some(kid),
                            ContentModelRule::UnexpectedChild(ChildModel::ListChild),
                        ),
                    }
                }
                if !has_item {
                    push(None, ContentModelRule::MissingChild(ChildModel::ListItem));
                }
            }
            StandardStructureType::ListItem => {
                for (kid, child) in element.kids.iter().enumerate() {
                    if let StructureKid::Element(child) = child
                        && !matches!(
                            self.standard_type(*child),
                            Some(StandardStructureType::Label | StandardStructureType::ListBody)
                        )
                    {
                        push(
                            Some(kid),
                            ContentModelRule::UnexpectedChild(ChildModel::ListPart),
                        );
                    }
                }
            }
            StandardStructureType::Table => {
                let mut has_rows = false;
                let last = element.kids.len().saturating_sub(1);
                for (kid, child) in element.kids.iter().enumerate() {
                    match child {
                        StructureKid::Element(child) => match self.standard_type(*child) {
                            Some(StandardStructureType::Caption) => {
                                if kid != 0 && kid != last {
                                    push(Some(kid), ContentModelRule::CaptionPlacement);
                                }
                            }
                            Some(
                                StandardStructureType::TableRow | StandardStructureType::TableBody,
                            ) => {
                                has_rows = true;
                            }
                            Some(
                                StandardStructureType::TableHead
                                | StandardStructureType::TableFooter,
                            ) => {}
                            _ => push(
                                Some(kid),
                                ContentModelRule::UnexpectedChild(ChildModel::TableChild),
                            ),
                        },
                        _ => push(
                            Some(kid),
                            ContentModelRule::UnexpectedChild(ChildModel::TableChild),
                        ),
                    }
                }
                if !has_rows {
                    push(None, ContentModelRule::MissingChild(ChildModel::TableRow));
                }
            }
            StandardStructureType::TableRow => {
                let mut has_cell = false;
                for (kid, child) in element.kids.iter().enumerate() {
                    match child {
                        StructureKid::Element(child)
                            if matches!(
                                self.standard_type(*child),
                                Some(
                                    StandardStructureType::TableHeader
                                        | StandardStructureType::TableData
                                )
                            ) =>
                        {
                            has_cell = true;
                        }
                        _ => push(
                            Some(kid),
                            ContentModelRule::UnexpectedChild(ChildModel::TableCell),
                        ),
                    }
                }
                if !has_cell {
                    push(None, ContentModelRule::MissingChild(ChildModel::TableCell));
                }
            }
            StandardStructureType::TableHead
            | StandardStructureType::TableBody
            | StandardStructureType::TableFooter => {
                let mut has_row = false;
                for (kid, child) in element.kids.iter().enumerate() {
                    match child {
                        StructureKid::Element(child)
                            if matches!(
                                self.standard_type(*child),
                                Some(StandardStructureType::TableRow)
                            ) =>
                        {
                            has_row = true;
                        }
                        _ => push(
                            Some(kid),
                            ContentModelRule::UnexpectedChild(ChildModel::TableRow),
                        ),
                    }
                }
                if !has_row {
                    push(None, ContentModelRule::MissingChild(ChildModel::TableRow));
                }
            }
            StandardStructureType::Link => {
                let mut has_object_reference = false;
                for (kid, child) in element.kids.iter().enumerate() {
                    match child {
                        StructureKid::ObjectReference(_) => has_object_reference = true,
                        StructureKid::Element(child)
                            if matches!(
                                self.standard_type(*child),
                                Some(StandardStructureType::Link)
                            ) =>
                        {
                            push(
                                Some(kid),
                                ContentModelRule::UnexpectedChild(ChildModel::ContentOrInline),
                            );
                        }
                        _ => {}
                    }
                }
                if !has_object_reference {
                    push(
                        None,
                        ContentModelRule::MissingChild(ChildModel::ObjectReference),
                    );
                }
            }
            StandardStructureType::Annot => {
                let mut has_object_reference = false;
                for (kid, child) in element.kids.iter().enumerate() {
                    match child {
                        StructureKid::ObjectReference(_) => has_object_reference = true,
                        StructureKid::Element(child)
                            if matches!(
                                self.standard_type(*child),
                                Some(StandardStructureType::Annot)
                            ) =>
                        {
                            push(
                                Some(kid),
                                ContentModelRule::UnexpectedChild(ChildModel::ContentOrInline),
                            );
                        }
                        _ => {}
                    }
                }
                if !has_object_reference {
                    push(
                        None,
                        ContentModelRule::MissingChild(ChildModel::ObjectReference),
                    );
                }
            }
            StandardStructureType::Ruby => {
                let kids: Vec<Option<StandardStructureType>> = element
                    .kids
                    .iter()
                    .map(|child| match child {
                        StructureKid::Element(child) => self.standard_type(*child),
                        _ => None,
                    })
                    .collect();
                let valid = matches!(
                    kids.as_slice(),
                    [
                        Some(StandardStructureType::RubyBase),
                        Some(StandardStructureType::RubyText)
                    ] | [
                        Some(StandardStructureType::RubyBase),
                        Some(StandardStructureType::RubyPunctuation),
                        Some(StandardStructureType::RubyText),
                        Some(StandardStructureType::RubyPunctuation)
                    ]
                );
                if !valid {
                    push(None, ContentModelRule::RubySequence);
                }
            }
            StandardStructureType::Form => {
                let has_role = self.resolved_attributes(index).iter().any(|attribute| {
                    attribute.owner == "PrintField"
                        && attribute
                            .value
                            .as_dict()
                            .is_some_and(|dict| dict.get("Role").is_some())
                });
                if !has_role
                    && !(element.kids.len() == 1
                        && matches!(element.kids.first(), Some(StructureKid::ObjectReference(_))))
                {
                    push(None, ContentModelRule::FormObjectReference);
                }
            }
            _ => {}
        }
    }
}
