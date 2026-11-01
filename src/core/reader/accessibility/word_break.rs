//! 词/短语替换与字符替换的词边界语义（ISO 32000-1 14.9.3、14.9.4、14.9.5）。
//!
//! 规范原文（raw 抽取）：
//!
//! - 14.9.3（L32121–L32124）：
//!   > When applied to structure elements, the alternate description text
//!   > **shall** be considered to be a complete (or whole) word or phrase
//!   > substitution for the current element. If each of two (or more) elements
//!   > in a sequence have an Alt entry in their dictionaries, they **shall** be
//!   > treated as if a word break is present between them. The same applies to
//!   > consecutive marked-content sequences.
//! - 14.9.4（L32139–L32143）：
//!   > The ActualText value **shall** be used as a replacement, not a
//!   > description, for the content … The value of ActualText **shall** be
//!   > considered to be a character substitution for the structure element or
//!   > marked-content sequence. If each of two (or more) consecutive structure
//!   > or marked-content sequences has an ActualText entry, they **shall** be
//!   > treated as if **no** word break is present between them.
//! - 14.9.5（L32181–L32183）：
//!   > The E value (a text string) **shall** be considered to be a word or
//!   > phrase substitution for the tagged text and therefore **shall** be
//!   > treated as if a word break separates it from any surrounding text.
//!
//! # 本库形态
//!
//! 这三条 shall 约束的是「相邻内容之间是否插入词边界」，属于**读端消费
//! 规则**：它不改变 PDF 字节，也无法由语法检查证伪，只影响辅助技术如何
//! 拼接提取出的文本。本库因此把它落成可查询的类型化判定，而不是修改文本
//! 提取的输出（既有提取结果保持不变，避免动摇既有断言）：
//!
//! - [`SubstitutionDeclaration`]：一个序列（`Span` 属性表或其所用结构元素）
//!   是否声明 `/Alt`、`/ActualText`、`/E`；
//! - [`word_break_between`]：纯函数，按上面三条 shall 判定两个相邻声明之间
//!   的词边界；
//! - [`Document::page_marked_content_word_breaks`]：把二者套到一页的标记内容
//!   序列上，返回逐边界的判定。
//!
//! # 判定次序与冲突（库策略）
//!
//! 规范没有规定 `/Alt` 成对与 `/ActualText` 成对同时成立时如何取舍
//! （14.9.4 NOTE 2 只说明二者语义不同）。本库按条目在规范中的先后顺序取
//! 词边界规则（14.9.3 先于 14.9.4），并在 [`WordBreakDecision::conflicting`]
//! 如实标记冲突，调用方可据此另行处置。`/E` 是单边规则：任一邻侧带 `/E`
//! 即构成词边界（14.9.5 的「与周围任何文本之间」）。
//!
//! # 声明来源与边界
//!
//! 每个区段的声明优先取 `Span` 属性表（14.9.3 第二条）；属性表没有该键时
//! 以属性表 `/MCID` 定位到的结构元素字典兜底（14.9.3 第一条）。嵌套区段
//! 不做特殊处理，边界按 [`Document::page_marked_content`] 的返回序（关闭
//! 顺序）逐对相邻计算。语言转义（[`SemanticText`](super::language_escape::SemanticText)）
//! 不参与本模块的判定：转义只换语言，不换词边界。

use crate::core::reader::content::MarkedSection;
use crate::core::reader::structure::StructureTree;
use crate::core::types::Document;

use super::language::LanguageError;
use super::span::{MarkedContentLanguageError, mcid_value};

/// 一个序列声明的替换语义条目集合（14.9.3、14.9.4、14.9.5）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SubstitutionDeclaration {
    /// 是否声明 `/Alt`（词/短语替换）。
    pub alternate_text: bool,
    /// 是否声明 `/ActualText`（字符替换）。
    pub actual_text: bool,
    /// 是否声明 `/E`（缩写展开）。
    pub expansion: bool,
}

impl SubstitutionDeclaration {
    /// 是否没有任何替换声明。
    pub fn is_empty(&self) -> bool {
        !self.alternate_text && !self.actual_text && !self.expansion
    }
}

/// 词边界判定的规范依据。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordBreakRule {
    /// 14.9.3：两侧相邻序列都带 `/Alt`，视为其间存在词边界。
    AlternateTextPair,
    /// 14.9.5：任一序列带 `/E`，其展开与周围文本之间存在词边界。
    ExpansionAdjacent,
    /// 14.9.4：两侧相邻连续序列都带 `/ActualText`，视为其间不存在词边界。
    ActualTextPair,
}

/// 相邻两个序列之间是否存在词边界。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordBreak {
    /// 应视为存在词边界。
    Present,
    /// 应视为不存在词边界（字符替换相连）。
    Absent,
    /// 三条 shall 都不命中：规范没有给出边界声明。
    NotDeclared,
}

/// 一对相邻声明的词边界判定结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WordBreakDecision {
    /// 是否应插入词边界；三条 shall 都不命中时为 [`WordBreak::NotDeclared`]。
    pub word_break: WordBreak,
    /// 命中的规则；`NotDeclared` 时为 `None`。
    pub rule: Option<WordBreakRule>,
    /// 词边界规则与字符替换规则同时命中（冲突，见模块文档的取舍说明）。
    pub conflicting: bool,
}

/// 按 14.9.3、14.9.4、14.9.5 判定 `before`/`after` 之间的词边界。
///
/// 次序：`/Alt` 成对 → 词边界；否则任一侧 `/E` → 词边界；否则
/// `/ActualText` 成对 → 无词边界；否则 [`WordBreak::NotDeclared`]。
/// 前两条与第三条同时命中时按前者给出词边界并置
/// [`WordBreakDecision::conflicting`]。
pub fn word_break_between(
    before: &SubstitutionDeclaration,
    after: &SubstitutionDeclaration,
) -> WordBreakDecision {
    let phrase_rule = if before.alternate_text && after.alternate_text {
        Some(WordBreakRule::AlternateTextPair)
    } else if before.expansion || after.expansion {
        Some(WordBreakRule::ExpansionAdjacent)
    } else {
        None
    };
    let character_rule = before.actual_text && after.actual_text;
    match phrase_rule {
        Some(rule) => WordBreakDecision {
            word_break: WordBreak::Present,
            rule: Some(rule),
            conflicting: character_rule,
        },
        None if character_rule => WordBreakDecision {
            word_break: WordBreak::Absent,
            rule: Some(WordBreakRule::ActualTextPair),
            conflicting: false,
        },
        None => WordBreakDecision {
            word_break: WordBreak::NotDeclared,
            rule: None,
            conflicting: false,
        },
    }
}

/// 页上一个标记内容序列的词边界判定（含规范来源）。
#[derive(Debug, Clone, PartialEq)]
pub struct MarkedContentWordBreak {
    /// 边界前区段在 [`Document::page_marked_content`] 返回序中的下标。
    pub before: usize,
    /// 边界后区段的下标（恒为 `before + 1`）。
    pub after: usize,
    /// 前一个区段的替换声明。
    pub before_declaration: SubstitutionDeclaration,
    /// 后一个区段的替换声明。
    pub after_declaration: SubstitutionDeclaration,
    /// [`word_break_between`] 的结论。
    pub decision: WordBreakDecision,
}

/// 读取一个区段的替换声明：`Span` 属性表与 `/MCID` 对应的结构元素字典
/// 任一声明了某条目即算声明（词边界只看「是否成对声明」，两个来源是
/// 并集关系；属性表形态非法仍以错误传播）。
fn declaration_of(
    section: &MarkedSection,
    tree: Option<&StructureTree>,
    parents: Option<&[Option<usize>]>,
) -> Result<SubstitutionDeclaration, LanguageError> {
    let mut declaration = section
        .span_properties()?
        .map(|properties| SubstitutionDeclaration {
            alternate_text: properties.alternate_text.is_some(),
            actual_text: properties.actual_text.is_some(),
            expansion: properties.expansion.is_some(),
        })
        .unwrap_or_default();
    if let (Some(source), Some(tree), Some(parents)) = (section.properties.as_ref(), tree, parents)
        && let Some(mcid) = mcid_value(source)
        && let Ok(mcid) = usize::try_from(mcid)
        && let Some(Some(element)) = parents.get(mcid)
        && let Some(element) = tree.element(*element)
    {
        declaration.alternate_text |= element.alt.is_some();
        declaration.actual_text |= element.actual_text.is_some();
        declaration.expansion |= element.expanded.is_some();
    }
    Ok(declaration)
}

impl Document {
    /// 本页相邻标记内容序列之间的词边界判定（14.9.3、14.9.4、14.9.5）。
    ///
    /// 返回的第 `i` 个条目对应 [`Document::page_marked_content`] 返回序里
    /// 第 `i` 与第 `i + 1` 个区段之间的边界，因此长度恒为
    /// `sections.len().saturating_sub(1)`；首个区段之前没有边界可判定。
    /// 每个区段的声明按 `Span` 属性表优先、其 `/MCID` 结构元素字典兜底。
    ///
    /// 结构树或父树缺失、损坏时静默退化为「只看属性表」（与
    /// [`Document::page_marked_content_languages`] 同一容错策略）；文本串
    /// 形态非法仍以 [`MarkedContentLanguageError`] 传播。
    pub fn page_marked_content_word_breaks(
        &self,
        page_index: usize,
    ) -> Result<Vec<MarkedContentWordBreak>, MarkedContentLanguageError> {
        let sections = self.page_marked_content(page_index)?;
        let page_parents = self.page_struct_parents(page_index).ok().flatten();
        let tree = self.structure_tree().ok().flatten();
        let parents: Option<&[Option<usize>]> = page_parents
            .as_ref()
            .map(|parents| parents.parents.as_slice());
        let mut declarations = Vec::with_capacity(sections.len());
        for section in &sections {
            declarations.push(declaration_of(section, tree.as_ref(), parents)?);
        }
        let mut boundaries = Vec::with_capacity(declarations.len().saturating_sub(1));
        for index in 0..declarations.len().saturating_sub(1) {
            let before = declarations[index];
            let after = declarations[index + 1];
            boundaries.push(MarkedContentWordBreak {
                before: index,
                after: index + 1,
                before_declaration: before,
                after_declaration: after,
                decision: word_break_between(&before, &after),
            });
        }
        Ok(boundaries)
    }
}
