//! 自然语言与多语言文本（ISO 32000-1 14.9.2，PDF 1.4/1.5）。
//!
//! 覆盖文档默认语言（14.9.2.1：目录 `/Lang`）、语言标识符语法与比较
//! （14.9.2.2：RFC 3066）、结构元素的 `/Lang` 继承（14.9.2.3）以及多语言
//! 文本数组（14.9.2.4：media clip data/section 的 `/Alt`）。
//!
//! 解析器对既有结构元素只保留 `/Lang` 原串；RFC 3066 校验只发生在
//! [`LanguageIdentifier::parse`] 等类型化入口，非法标签不会让
//! [`crate::core::types::Document::structure_tree`] 整体失败。
//!
//! 层级语义：元素自身 `/Lang` 优先，否则沿父链取最近的声明；空串是
//! 「显式未知」，会终止继承。都没有时调用方回退到
//! [`Document::document_language`]。
//!
//! 另覆盖内容流标记层面的读写模型：Artifact 属性表（14.8.2.2.2 Table 330）、
//! Span 属性表（14.9.3–14.9.5）、反序显示串 [`Document::page_reversed_chars`]
//! （14.8.2.3.3），以及 artifact 与逻辑结构的归属诊断
//! [`Document::artifact_attribution_violations`]（14.8.2.2.1、14.8.2.3.1）。
//!
//! 字体特征推导 [`Document::font_character_properties`]
//! （14.8.2.4.3 Table 331/Table 332，见 `character_properties` 模块文档）。
//!
//! 14.8.2.3.2 的注解排序读端由 [`annotation_ordering`] 提供：
//! [`Document::annotation_content_order`] 从逻辑结构导出注解在页面内容序列
//! 中的位置，返回 [`ContentOrderItem`] 序列，以
//! [`AnnotationOrderBasis`] 区分 `Annot`/`Link`/`Form` 元素的显式关联、
//! 14.8.2.3.2 的邻接（在前/在后）与无从判定三档。
//!
//! 14.9.3–14.9.5 的语义落地由三个模块补齐：
//! [`language_escape`] 按 7.9.2.2 解析文本串里的 Unicode 语言转义，给出
//! 「转义覆盖 prevailing Lang」与去转义文本（[`SemanticText`]）；
//! [`word_break`] 把 `/Alt` 词/短语替换与 `/ActualText` 字符替换、`/E`
//! 展开的词边界三条 shall 做成可查询判定
//! （[`Document::page_marked_content_word_breaks`]）；[`description_source`]
//! 按 14.9.3 与 12.5.6.2 给出注解 `/Contents` 的来源优先级
//! （[`Annotation::alternate_description`]）。

use std::collections::BTreeSet;

use crate::core::types::Document;

mod annotation_ordering;
#[cfg(test)]
mod annotation_ordering_tests;
mod artifact;
mod artifact_attribution;
#[cfg(test)]
mod artifact_attribution_tests;
mod character_properties;
mod character_properties_derive;
#[cfg(test)]
mod character_properties_font_size_tests;
#[cfg(test)]
mod character_properties_tests;
mod description_source;
mod language;
mod language_escape;
#[cfg(test)]
mod language_escape_tests;
mod multi_language;
mod reversed_chars;
#[cfg(test)]
mod reversed_chars_tests;
mod span;
#[cfg(test)]
mod tests;
mod word_break;
#[cfg(test)]
mod word_break_tests;

pub use annotation_ordering::{
    AnnotationOrderBasis, AnnotationOrderingError, AnnotationPosition, ContentOrderItem,
    MarkedContentPosition,
};
pub use artifact::{
    ArtifactAttachment, ArtifactError, ArtifactProperties, ArtifactSubtype, ArtifactType,
};
pub use artifact_attribution::{
    ArtifactAttributionError, ArtifactAttributionRule, ArtifactAttributionViolation,
};
pub use character_properties::{
    CharacterProperties, CharacterPropertiesError, FontStyle, FontVariant, GenericFontFamily,
    NameDerivedAttribute,
};
pub use description_source::{
    AnnotationDescription, AnnotationDescriptionSource, annotation_description_source,
};
pub use language::{LanguageError, LanguageIdentifier};
pub use language_escape::{LANGUAGE_ESCAPE, LanguageTextSegment, SemanticText, SemanticTextEntry};
pub use multi_language::{AlternateText, MultiLanguageTextArray};
pub use reversed_chars::{REVERSED_CHARS_TAG, ReversedCharsDiagnostic, ReversedCharsSequence};
pub use span::{MarkedContentLanguageError, SpanProperties};
pub use word_break::{
    MarkedContentWordBreak, SubstitutionDeclaration, WordBreak, WordBreakDecision, WordBreakRule,
    word_break_between,
};

use language::malformed;

/// 结构元素语言继承的最大祖先层数。
const MAX_LANGUAGE_ANCESTORS: usize = 64;

impl Document {
    /// 读取文档默认语言（14.9.2.1：目录 `/Lang`，PDF 1.4）。
    ///
    /// 目录没有 `/Lang` 时返回 `Ok(None)`；值为间接引用时只解一层
    /// （与 14.9.2 的条目形态一致）；必须是文本串，空串合法并映射为
    /// [`LanguageIdentifier::unknown`]。
    pub fn document_language(&self) -> Result<Option<LanguageIdentifier>, LanguageError> {
        let catalog = crate::core::reader::navigation::catalog(self)
            .map_err(|error| malformed(format!("catalog: {error}")))?;
        let Some(value) = catalog.get("Lang") else {
            return Ok(None);
        };
        let value = match value {
            crate::core::types::Value::Ref(number, generation) => self
                .get_object(*number)
                .map_err(|error| {
                    malformed(format!(
                        "/Lang reference {number} {generation} cannot be read: {error}"
                    ))
                })?
                .map(|object| object.value)
                .ok_or_else(|| {
                    malformed(format!(
                        "/Lang reference {number} {generation} does not exist"
                    ))
                })?,
            other => other.clone(),
        };
        let text = language::decode_text_string(&value).ok_or_else(|| {
            malformed(format!(
                "catalog /Lang is not a text string ({})",
                value.type_name()
            ))
        })?;
        LanguageIdentifier::parse(&text).map(Some)
    }
}

/// 沿 `parent_at` 链查找第一个声明了 `/Lang` 的节点并解析。
///
/// `visited` 防环，最多检查 [`MAX_LANGUAGE_ANCESTORS`] 个节点；超出上限
/// 或遇到环时静默停止。空串由 [`LanguageIdentifier::parse`] 映射为
/// 未知语言，调用方据此终止继承。
fn first_declared_language(
    start: usize,
    mut parent_at: impl FnMut(usize) -> Option<usize>,
    mut language_at: impl FnMut(usize) -> Option<String>,
) -> Result<Option<LanguageIdentifier>, LanguageError> {
    let mut current = Some(start);
    let mut visited = BTreeSet::new();
    for _ in 0..MAX_LANGUAGE_ANCESTORS {
        let Some(index) = current else {
            return Ok(None);
        };
        if !visited.insert(index) {
            return Ok(None);
        }
        if let Some(language) = language_at(index) {
            return LanguageIdentifier::parse(&language).map(Some);
        }
        current = parent_at(index);
    }
    Ok(None)
}

impl crate::core::reader::structure::StructureTree {
    /// 结构元素自身的 `/Lang`（14.9.2.3）。
    ///
    /// 下标越界或元素没有 `/Lang` 时返回 `Ok(None)`；空串映射为
    /// [`LanguageIdentifier::unknown`]，语法非法返回
    /// [`LanguageError::Malformed`]。
    pub fn element_language(
        &self,
        index: usize,
    ) -> Result<Option<LanguageIdentifier>, LanguageError> {
        let Some(element) = self.element(index) else {
            return Ok(None);
        };
        let Some(language) = &element.lang else {
            return Ok(None);
        };
        LanguageIdentifier::parse(language).map(Some)
    }

    /// 结构元素实际生效的语言（14.9.2.3）。
    ///
    /// 从自身沿
    /// [`StructureElement::parent`](crate::core::reader::structure::StructureElement::parent)
    /// 链向上，第一个带 `/Lang`
    /// 的元素即结果；空串同样返回未知语言并终止继承。链上没有声明时
    /// 返回 `Ok(None)`，调用方回退到 [`Document::document_language`]。
    /// 越界下标返回 `Ok(None)`；环与超过 64 层深度静默停止。
    pub fn effective_language(
        &self,
        index: usize,
    ) -> Result<Option<LanguageIdentifier>, LanguageError> {
        if self.element(index).is_none() {
            return Ok(None);
        }
        first_declared_language(
            index,
            |index| self.element(index).and_then(|element| element.parent),
            |index| self.element(index).and_then(|element| element.lang.clone()),
        )
    }
}

impl crate::core::reader::rendition::MediaClipData {
    /// `/Alt` 的替代文本（14.9.2.4 Table 274）：普通文本串或多语言数组。
    ///
    /// 条目缺失返回 `Ok(None)`；形态非法返回
    /// [`LanguageError::Malformed`]。
    pub fn alternate_text(&self) -> Result<Option<AlternateText>, LanguageError> {
        self.alt.as_ref().map(AlternateText::parse).transpose()
    }
}

impl crate::core::reader::rendition::MediaClipSection {
    /// `/Alt` 的替代文本（14.9.2.4 Table 277）：普通文本串或多语言数组。
    ///
    /// 条目缺失返回 `Ok(None)`；形态非法返回
    /// [`LanguageError::Malformed`]。
    pub fn alternate_text(&self) -> Result<Option<AlternateText>, LanguageError> {
        self.alt.as_ref().map(AlternateText::parse).transpose()
    }
}
