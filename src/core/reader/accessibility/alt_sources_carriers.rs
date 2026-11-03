//! 各载体到 [`AlternativeCandidate`] 的采集（ISO 32000-1 14.9.3 读端）。
//!
//! 薄胶水层：从既有类型化视图（[`SpanProperties`](super::span::SpanProperties)
//! 读的同一条目、[`Annotation::alternate_description`]、
//! [`Field::alternate_name_semantic_text`](crate::core::reader::form::Field::alternate_name_semantic_text)、
//! [`AlternateText::semantic_text_for`]）取候选，交给
//! [`resolve_alternatives`](super::alt_sources::resolve_alternatives)。
//! 条目形态非法（非文本串、残缺转义）在这里一律转成 `Err` 候选，由引擎
//! 记为 [`Malformed`](super::alt_sources::AlternativeTextProblem::Malformed)
//! 问题——调用方拿到的是 `Ok` 结果里的类型化问题，不是 panic。

use crate::core::reader::annotation::Annotation;
use crate::core::reader::content::MarkedSection;
use crate::core::reader::form::Field;
use crate::core::reader::rendition::{MediaClipData, MediaClipSection};
use crate::core::types::{Document, Value};

use super::alt_sources::{
    AlternativeCandidate, AlternativeTextSource, AlternativesResolution, resolve_alternatives,
};
use super::language::{LanguageError, LanguageIdentifier};
use super::language_escape::{SemanticText, SemanticTextEntry};
use super::multi_language::AlternateText;
use super::span::{MarkedContentLanguageError, mcid_value};

/// `/Alt` 家族条目与来源的对应（描述槽）。
const DESCRIPTION_ENTRY: (SemanticTextEntry, AlternativeTextSource) = (
    SemanticTextEntry::AlternateText,
    AlternativeTextSource::MarkedContentAlt,
);

/// Span 属性表的两条语义条目与来源对应。
const SPAN_ENTRIES: [(SemanticTextEntry, AlternativeTextSource); 2] = [
    DESCRIPTION_ENTRY,
    (
        SemanticTextEntry::ActualText,
        AlternativeTextSource::MarkedContentActualText,
    ),
];

/// 结构元素的两条语义条目与来源对应。
const STRUCTURE_ENTRIES: [(SemanticTextEntry, AlternativeTextSource); 2] = [
    (
        SemanticTextEntry::AlternateText,
        AlternativeTextSource::StructureElementAlt,
    ),
    (
        SemanticTextEntry::ActualText,
        AlternativeTextSource::StructureElementActualText,
    ),
];

fn push_candidate(
    candidates: &mut Vec<AlternativeCandidate>,
    source: AlternativeTextSource,
    outcome: Result<Option<SemanticText>, LanguageError>,
    prevailing: Option<LanguageIdentifier>,
) {
    match outcome {
        Ok(Some(text)) => {
            candidates.push(AlternativeCandidate::decoded(source, text, prevailing));
        }
        Ok(None) => {}
        Err(error) => {
            candidates.push(AlternativeCandidate::malformed(source, error, prevailing));
        }
    }
}

impl MarkedSection {
    /// 本区段 `Span` 属性表的 `/Alt` 与 `/ActualText` 候选（14.9.3、14.9.4）。
    ///
    /// 标签不是精确的 `Span`（规范只给 Span 属性表定义这两个替代文本
    /// 条目）或没有属性表时返回空集；条目缺失不产生候选；条目形态非法
    /// 产生 `Err` 候选（引擎记 Malformed 问题，不 panic）。
    pub fn span_alternative_candidates(
        &self,
        prevailing: Option<&LanguageIdentifier>,
    ) -> Vec<AlternativeCandidate> {
        if self.tag != "Span" {
            return Vec::new();
        }
        let Some(properties) = self.properties.as_ref() else {
            return Vec::new();
        };
        let mut candidates = Vec::with_capacity(SPAN_ENTRIES.len());
        for (entry, source) in SPAN_ENTRIES {
            let outcome = SemanticText::from_dict(properties, entry);
            push_candidate(&mut candidates, source, outcome, prevailing.cloned());
        }
        candidates
    }
}

impl Document {
    /// 整页标记内容区段的替代文本解析（14.9.3–14.9.4 读端聚合）。
    ///
    /// 返回与 [`Document::page_marked_content`] 一一对应的
    /// [`AlternativesResolution`] 序列。每个区段同时采集两路候选：
    ///
    /// 1. `Span` 属性表的 `/Alt` `/ActualText`（[`MarkedSection::span_alternative_candidates`]）；
    /// 2. 属性表含整数 `/MCID` 且能经页面 `/StructParents` 与父树定位到
    ///    结构元素时，该元素字典的 `/Alt` `/ActualText`
    ///    （[`StructureTree::element_semantic_text`](crate::core::reader::structure::StructureTree::element_semantic_text)）。
    ///
    /// prevailing Lang 取 [`Document::page_marked_content_languages`] 的
    /// 同位条目（结构内容即结构语言，14.9.2.3）；结构树或父树缺失、
    /// 越界一律静默降级为「无结构候选」。内容流无法解析或语言非法按
    /// [`MarkedContentLanguageError`] 传播；页索引越界是类型化错误，不
    /// panic。
    pub fn page_marked_content_alternatives(
        &self,
        page_index: usize,
    ) -> Result<Vec<AlternativesResolution>, MarkedContentLanguageError> {
        let sections = self.page_marked_content(page_index)?;
        let languages = self.page_marked_content_languages(page_index)?;
        let page_parents = self.page_struct_parents(page_index).ok().flatten();
        let tree = self.structure_tree().ok().flatten();
        let mut resolutions = Vec::with_capacity(sections.len());
        for (index, section) in sections.iter().enumerate() {
            let prevailing = languages.get(index).and_then(|language| language.as_ref());
            let mut candidates = section.span_alternative_candidates(prevailing);
            if let Some(element) = self.section_structure_element(section, page_parents.as_ref())
                && let Some(tree) = tree.as_ref()
            {
                for (entry, source) in STRUCTURE_ENTRIES {
                    let outcome = tree.element_semantic_text(element, entry);
                    push_candidate(&mut candidates, source, outcome, prevailing.cloned());
                }
            }
            resolutions.push(resolve_alternatives(candidates));
        }
        Ok(resolutions)
    }

    /// 经 `/MCID` → 页面父树数组定位区段所属结构元素的下标。
    fn section_structure_element(
        &self,
        section: &MarkedSection,
        page_parents: Option<&crate::core::reader::structure::PageStructureParents>,
    ) -> Option<usize> {
        let mcid = mcid_value(section.properties.as_ref()?)?;
        let parents = page_parents?;
        parents
            .parents
            .get(usize::try_from(mcid).ok()?)
            .copied()
            .flatten()
    }
}

impl Annotation {
    /// 本注解的替代文本解析：唯一来源是 `/Contents`（14.9.3；显示文本
    /// 与否的 shall/may 层级见
    /// [`Annotation::alternate_description`] 与
    /// [`AnnotationDescriptionSource`](super::description_source::AnnotationDescriptionSource)）。
    ///
    /// 无 `/Contents`（含带父 `Popup`）→ 两槽皆 `None`；条目形态非法 →
    /// `Malformed` 问题。`/T`、`/Subj` 等不是 14.9.3 的替代描述来源，
    /// 不采集。
    pub fn resolved_alternative_text(
        &self,
        prevailing: Option<&LanguageIdentifier>,
    ) -> AlternativesResolution {
        let candidate = match self.alternate_description(prevailing) {
            Ok(Some(description)) => Some(AlternativeCandidate::decoded(
                AlternativeTextSource::AnnotationContents,
                description.text,
                prevailing.cloned(),
            )),
            Ok(None) => None,
            Err(error) => Some(AlternativeCandidate::malformed(
                AlternativeTextSource::AnnotationContents,
                error,
                prevailing.cloned(),
            )),
        };
        resolve_alternatives(candidate)
    }
}

impl Field {
    /// 本字段的替代文本解析：唯一来源是 `/TU` 替代字段名（Table 220：
    /// 「if present, **shall** be used in place of the actual field name」；
    /// 14.9.3 NOTE 2 的朗读用途）。
    ///
    /// 只读本节点（`/TU` 不可继承）；无 `/TU` → `None`；残缺语言转义 →
    /// `Malformed` 问题。`/T` 字段名按 shall 是被顶替的对象，不是来源。
    pub fn resolved_alternative_text(
        &self,
        prevailing: Option<&LanguageIdentifier>,
    ) -> AlternativesResolution {
        let candidate = match self.alternate_name_semantic_text() {
            Ok(Some(text)) => Some(AlternativeCandidate::decoded(
                AlternativeTextSource::FieldAlternateName,
                text,
                prevailing.cloned(),
            )),
            Ok(None) => None,
            Err(error) => Some(AlternativeCandidate::malformed(
                AlternativeTextSource::FieldAlternateName,
                error,
                prevailing.cloned(),
            )),
        };
        resolve_alternatives(candidate)
    }
}

/// 把 media clip `/Alt`（文本串或多语言数组，14.9.2.4）解析成候选集。
///
/// 数组按 prevailing Lang 选中后再解析串内转义
/// （[`AlternateText::semantic_text_for`]）；选不中条目 → 无候选；
/// 数组形态非法 → `Malformed` 候选。
pub fn media_clip_alternatives(
    alt: &AlternateText,
    prevailing: Option<&LanguageIdentifier>,
) -> AlternativesResolution {
    let candidate = match alt.semantic_text_for(prevailing) {
        Ok(Some(text)) => Some(AlternativeCandidate::decoded(
            AlternativeTextSource::MediaClipAlt,
            text,
            prevailing.cloned(),
        )),
        Ok(None) => None,
        Err(error) => Some(AlternativeCandidate::malformed(
            AlternativeTextSource::MediaClipAlt,
            error,
            prevailing.cloned(),
        )),
    };
    resolve_alternatives(candidate)
}

/// 从原始 `/Alt` 值（可为文本串或多语言数组）解析 media clip 描述槽。
fn clip_alt_resolution(
    alt: Option<&Value>,
    prevailing: Option<&LanguageIdentifier>,
) -> AlternativesResolution {
    match alt.map(AlternateText::parse) {
        Some(Ok(parsed)) => media_clip_alternatives(&parsed, prevailing),
        Some(Err(error)) => resolve_alternatives(Some(AlternativeCandidate::malformed(
            AlternativeTextSource::MediaClipAlt,
            error,
            prevailing.cloned(),
        ))),
        None => resolve_alternatives(None::<AlternativeCandidate>),
    }
}

impl MediaClipData {
    /// 剪辑数据 `/Alt` 的替代文本解析（14.9.2.4 Table 274 + 14.9.3）。
    pub fn resolved_alternative_text(
        &self,
        prevailing: Option<&LanguageIdentifier>,
    ) -> AlternativesResolution {
        clip_alt_resolution(self.alt.as_ref(), prevailing)
    }
}

impl MediaClipSection {
    /// 剪辑片段 `/Alt` 的替代文本解析（14.9.2.4 Table 277 + 14.9.3）。
    pub fn resolved_alternative_text(
        &self,
        prevailing: Option<&LanguageIdentifier>,
    ) -> AlternativesResolution {
        clip_alt_resolution(self.alt.as_ref(), prevailing)
    }
}
