//! 替代文本来源解析（ISO 32000-1 14.9.3–14.9.4，读端）。
//!
//! 本模块把已有的各个替代文本载体（结构元素与 Span 属性表的 `/Alt`、
//! `/ActualText`，注解 `/Contents`，media clip `/Alt`，字段 `/TU`）按
//! 14.9.3 的语义**一起**解析：给定候选集合，返回生效文本、来源出处
//! （provenance）与被降级候选的类型化问题，而不是让调用方自己逐个取值。
//!
//! # 规范依据（raw 抽取，全文行号）
//!
//! - 14.9.3（L32092–L32121）：替代描述可给出的对象——结构元素字典
//!   `/Alt`；（PDF 1.5）以 `Span` 标签挂在标记内容序列上的属性表 `/Alt`；
//!   「尚没有文本表示」的注解经 `/Contents`（来源层级已由
//!   [`AnnotationDescriptionSource`](super::description_source::AnnotationDescriptionSource)
//!   类型化）；以及编码 shall（L32115–L32116）与语言转义 shall 覆盖
//!   prevailing Lang（L32117–L32120，落实见 [`SemanticText`]，本模块直接复用）。
//!   表单字段 `/TU`（Table 220，L23645–L23650）：
//!
//!   > An alternate name … which, if present, **shall** be used in place of
//!   > the actual field name when a conforming reader identifies the field
//!   > in a user-interface.
//!
//! - 14.9.4（L32122–L32160）：`/ActualText` **shall** 是「替换」而非
//!   「描述」（character substitution），NOTE 2 明确它与 `Alt` 的
//!   whole-word/phrase substitution 处理不同——因此本模块把两者放进
//!   [`AlternativesResolution`] 的**两个独立槽位**（replacement 与
//!   description），绝不折叠成一个「alt 文本」。
//!
//! # 优先级：spec 给了什么、库策略补什么
//!
//! 规范给出严格顺序的只有两处：注解 `/Contents` 的来源层级（shall 用于
//! 显示文本的注解类型，may 用于其余，见 14.9.3）与 `/TU` 顶替字段名 `/T`
//! （本库不把 `/T` 当替代文本来源，故无可竞争项）。规范**没有**给出
//! `/Alt` 与 `/ActualText` 之间、也没有给出结构元素 `/Alt` 与 Span 属性表
//! `/Alt` 之间的先后。库策略（非 shall）：
//!
//! 1. 角色隔离：`/ActualText` 候选只进 replacement 槽，`/Alt`、`/Contents`、
//!    `/TU`、media clip `/Alt` 只进 description 槽。
//! 2. 同槽位内按 [`AlternativeTextSource::rank`]：结构元素 > Span 属性表
//!    （与 14.9.2.3「结构内容语言以结构元素为准」的既有取向一致），
//!    其后注解、media clip、字段。
//! 3. 降级：候选去转义后为空串或纯空白、或形态非法（非文本串、残缺
//!    转义）时**不胜出**，记为 [`AlternativeTextProblem`] 并继续考察下一
//!    来源——「空的 `/Alt` 压过真实的 `/Contents`」是本模块要杜绝的情形。
//!    （14.9.3 未明文要求非空；「要给出有意义的替代文本」是本库对
//!    NOTE 1 朗读用途的落实，属库策略。）
//! 4. 同槽位两个候选去转义文本完全相同且都有效时，低秩者记
//!    [`AlternativeTextProblem::Duplicate`]，胜者仍取高秩。
//!
//! # 边界
//!
//! - 只读解析，不写回；载体写端已由 builder 具备。
//! - 一切取值带守卫：越界下标、缺失结构树、非法形态都收敛为
//!   `Ok` 结果里的类型化问题或 `None`，不 panic。
//! - 语言：每个胜出条目的生效语言 = 串内转义（若有）覆盖 prevailing Lang
//!   （14.9.3 shall），由 [`SemanticText::effective_language`] 给出。

use std::fmt;

use super::language::LanguageError;
use super::language::LanguageIdentifier;
use super::language_escape::SemanticText;

/// 替代文本的角色（14.9.3 描述 vs 14.9.4 替换）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AlternativeTextRole {
    /// 14.9.4 替换文本：与所见等价的字符替换。
    Replacement,
    /// 14.9.3 替代描述：为无法自然转文本的内容给出的人类可读说明。
    Description,
}

/// 一条替代文本的来源出处（14.9.3–14.9.4 的载体条目）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AlternativeTextSource {
    /// 结构元素字典 `/ActualText`（14.9.4，Table 323）。
    StructureElementActualText,
    /// 结构元素字典 `/Alt`（14.9.3）。
    StructureElementAlt,
    /// `Span` 属性表 `/ActualText`（14.9.4，PDF 1.5）。
    MarkedContentActualText,
    /// `Span` 属性表 `/Alt`（14.9.3，PDF 1.5）。
    MarkedContentAlt,
    /// 注解字典 `/Contents`（14.9.3；shall/may 层级见
    /// [`AnnotationDescriptionSource`](super::description_source::AnnotationDescriptionSource)）。
    AnnotationContents,
    /// media clip `/Alt`（14.9.2.4 Table 274/277，文本串或多语言数组）。
    MediaClipAlt,
    /// 字段字典 `/TU` 替代字段名（Table 220 + 14.9.3 NOTE 2）。
    FieldAlternateName,
}

impl AlternativeTextSource {
    /// 该来源填充的角色槽（14.9.4 NOTE 2：替换与描述不混同）。
    pub fn role(self) -> AlternativeTextRole {
        match self {
            Self::StructureElementActualText | Self::MarkedContentActualText => {
                AlternativeTextRole::Replacement
            }
            Self::StructureElementAlt
            | Self::MarkedContentAlt
            | Self::AnnotationContents
            | Self::MediaClipAlt
            | Self::FieldAlternateName => AlternativeTextRole::Description,
        }
    }

    /// 同角色内的库策略优先序（越小越优先；规范未给跨来源顺序，
    /// 见模块文档「优先级」一节）。
    pub fn rank(self) -> u8 {
        match self {
            Self::StructureElementActualText | Self::StructureElementAlt => 0,
            Self::MarkedContentActualText | Self::MarkedContentAlt => 1,
            Self::AnnotationContents => 2,
            Self::MediaClipAlt => 3,
            Self::FieldAlternateName => 4,
        }
    }

    /// 载体字典里的条目名（media clip 与注解固定单名）。
    pub fn dict_entry(self) -> &'static str {
        match self {
            Self::StructureElementActualText | Self::MarkedContentActualText => "ActualText",
            Self::StructureElementAlt | Self::MarkedContentAlt | Self::MediaClipAlt => "Alt",
            Self::AnnotationContents => "Contents",
            Self::FieldAlternateName => "TU",
        }
    }
}

impl fmt::Display for AlternativeTextSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}(/{})", self.dict_entry())
    }
}

/// 一路候选：来源 + 已按 7.9.2.2 解析的语言转义视图（或类型化读取失败）
/// + prevailing Lang。
#[derive(Debug, Clone, PartialEq)]
pub struct AlternativeCandidate {
    /// 候选来源。
    pub source: AlternativeTextSource,
    /// 去转义视图；`Err` 表示载体条目形态非法（非文本串、残缺转义等）。
    pub text: Result<SemanticText, LanguageError>,
    /// 无转义时生效的 prevailing Lang（14.9.2.1）。
    pub prevailing: Option<LanguageIdentifier>,
}

impl AlternativeCandidate {
    /// 一条成功解码的候选。
    pub fn decoded(
        source: AlternativeTextSource,
        text: SemanticText,
        prevailing: Option<LanguageIdentifier>,
    ) -> Self {
        Self {
            source,
            text: Ok(text),
            prevailing,
        }
    }

    /// 一条读取失败（类型化）的候选。
    pub fn malformed(
        source: AlternativeTextSource,
        error: LanguageError,
        prevailing: Option<LanguageIdentifier>,
    ) -> Self {
        Self {
            source,
            text: Err(error),
            prevailing,
        }
    }
}

/// 解析过程中被降级/记录的类型化问题。
#[derive(Debug, Clone, PartialEq)]
pub enum AlternativeTextProblem {
    /// 候选去转义后是空串（14.9.3 的朗读用途落空；库策略：不胜出）。
    Empty {
        /// 被降级候选的来源。
        source: AlternativeTextSource,
    },
    /// 候选去转义后只含 Unicode 空白字符。
    WhitespaceOnly {
        /// 被降级候选的来源。
        source: AlternativeTextSource,
    },
    /// 候选条目形态非法（非文本串、残缺语言转义等）。
    Malformed {
        /// 被降级候选的来源。
        source: AlternativeTextSource,
        /// 上游读出的类型化错误。
        error: LanguageError,
    },
    /// 低秩候选与胜出候选的去转义文本逐字符相同（重复给出）。
    Duplicate {
        /// 胜出来源。
        kept: AlternativeTextSource,
        /// 被判定为重复的来源。
        dropped: AlternativeTextSource,
    },
}

/// 一条生效的替代文本：文本 + 出处 + 生效语言。
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedAlternative {
    /// 供出处展示/断言的来源。
    pub source: AlternativeTextSource,
    /// 去掉语言转义后的文本。
    pub text: String,
    /// 生效语言：转义覆盖 prevailing（14.9.3 shall）。
    pub language: Option<LanguageIdentifier>,
}

/// 一个载体的完整解析结果（14.9.3 描述槽 + 14.9.4 替换槽）。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AlternativesResolution {
    /// 14.9.3 替代描述槽位；无有效候选时为 `None`。
    pub description: Option<ResolvedAlternative>,
    /// 14.9.4 替换文本槽位；无有效候选时为 `None`。
    pub replacement: Option<ResolvedAlternative>,
    /// 被降级/记录的候选问题，按候选遍历顺序。
    pub problems: Vec<AlternativeTextProblem>,
}

impl AlternativesResolution {
    /// 「生效替代文本」：描述槽优先，其次替换槽（库策略，两槽始终
    /// 可在结构体里分别取用，不会被折叠）。
    pub fn effective(&self) -> Option<&ResolvedAlternative> {
        self.description.as_ref().or(self.replacement.as_ref())
    }
}

/// 核心引擎：按库策略排序候选、降级空/空白/非法条目、标记重复，给出
/// 两槽结果与问题清单。纯函数，不触碰任何文档对象。
pub fn resolve_alternatives(
    candidates: impl IntoIterator<Item = AlternativeCandidate>,
) -> AlternativesResolution {
    let mut ordered: Vec<AlternativeCandidate> = candidates.into_iter().collect();
    ordered.sort_by_key(|candidate| (candidate.source.role(), candidate.source.rank()));
    let mut resolution = AlternativesResolution::default();
    for candidate in ordered {
        let source = candidate.source;
        let text = match candidate.text {
            Err(error) => {
                resolution
                    .problems
                    .push(AlternativeTextProblem::Malformed { source, error });
                continue;
            }
            Ok(text) => text,
        };
        let plain = text.plain_text();
        if plain.is_empty() {
            resolution
                .problems
                .push(AlternativeTextProblem::Empty { source });
            continue;
        }
        if plain.chars().all(char::is_whitespace) {
            resolution
                .problems
                .push(AlternativeTextProblem::WhitespaceOnly { source });
            continue;
        }
        let resolved = ResolvedAlternative {
            source,
            text: plain,
            language: text.effective_language(candidate.prevailing.as_ref()),
        };
        let slot = match source.role() {
            AlternativeTextRole::Description => &mut resolution.description,
            AlternativeTextRole::Replacement => &mut resolution.replacement,
        };
        match slot {
            Some(kept) => {
                if kept.text == resolved.text {
                    resolution.problems.push(AlternativeTextProblem::Duplicate {
                        kept: kept.source,
                        dropped: resolved.source,
                    });
                }
            }
            winner @ None => *winner = Some(resolved),
        }
    }
    resolution
}
