//! 注解 `/Contents` 作为替代描述的来源优先级（ISO 32000-1 14.9.3、12.5.6.2）。
//!
//! 规范原文（raw 抽取）：
//!
//! - 14.9.3 可指定替代描述的对象（L32097–L32103）里，注解占一条：
//!   > Any type of annotation (see 12.5.3, "Annotations") that does not already
//!   > have a text representation, through a Contents entry in the annotation
//!   > dictionary
//! - 14.9.3（L32104–L32106，来源优先级）：
//!   > For annotation types that normally display text, the Contents entry of
//!   > the annotation dictionary **shall** be used as the source for an
//!   > alternate description. For annotation types that do not display text, a
//!   > Contents entry (PDF 1.4) may be included to specify an alternate
//!   > description. Sound annotations, which need no alternate description for
//!   > the purpose of vocalization, may include a Contents entry specifying a
//!   > description that may be displayed for the benefit of users with hearing
//!   > impairments.
//! - 14.9.3（L32115–L32116，编码）：
//!   > Alternate descriptions are text strings, which **shall** be encoded in
//!   > either PDFDocEncoding or Unicode character encoding.
//!
//! # 「正常显示文本」的判定依据（复用既有权威来源，不另立子类型表）
//!
//! 12.5.6.2「Markup Annotations」（raw L21633–L21660）把 `/Contents` 的语义
//! 按注解分组说清楚，且分组与 Table 169 的 Markup 列一致：
//!
//! > Typically, it is the text that shall be displayed for the annotation or,
//! > if the annotation does not display text, an alternate description of the
//! > annotation's contents in human-readable form.
//! > • Free text annotations display text directly on the page. The
//! > annotation's Contents entry specifies the displayed text.
//! > • Most other markup annotations have an associated pop-up window that may
//! > contain text. The annotation's Contents entry specifies the text that
//! > shall be displayed when the pop-up window is opened. …
//! > • Sound annotations do not have a pop-up window but may also have
//! > associated text specified by the Contents entry.
//! > • For all other annotation types (Link, Movie, Widget, PrinterMark, and
//! > TrapNet), the Contents entry shall provide an alternate representation of
//! > the annotation's contents in human-readable form …
//!
//! 因此「正常显示文本」= [`AnnotationSubtype::is_markup`] 为真且不是 `Sound`；
//! 「不显示文本」= 非 markup 子类型。子类型名、引入版本与 markup 归属仍只有
//! [`AnnotationSubtype`]（Table 169）一个权威来源，本模块只做映射，不复制
//! 任何一列。
//!
//! # 边界（库策略与诚实缺口）
//!
//! - `Popup` 在 Table 169 的 Markup 列是 No，故归入
//!   [`AnnotationDescriptionSource::OptionalContents`]；12.5.6.2 NOTE 2 补充
//!   「注解有 `/Parent` 时其 `/Contents` 应被忽略」，本模块按 NOTE 2 在
//!   [`Annotation::alternate_description`] 里对带父的 `Popup` 返回 `None`。
//! - `Widget` 等表单字段自带文本表示，14.9.3 的替代**名称**通道是字段
//!   `/TU`（已由表单模块建模），本模块不把它当替代描述来源。
//! - 「shall be used as the source」约束的是读屏软件取哪一路文本朗读，PDF
//!   层无法验证其结果；本库给出的是**来源判定**，即把优先级以类型暴露出来。
//! - 编码 shall（PDFDocEncoding 或 Unicode）由 [`SemanticText::from_value`]
//!   上游的文本串解码保证；串内的语言转义按 14.9.3 的 shall 覆盖 prevailing
//!   Lang，见 [`SemanticText::effective_language`]。

use crate::core::reader::annotation::{Annotation, AnnotationSubtype};
use crate::core::types::Value;

use super::language::{LanguageError, LanguageIdentifier};
use super::language_escape::SemanticText;

/// 14.9.3 规定的注解替代描述来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnotationDescriptionSource {
    /// 子类型正常显示文本（12.5.6.2 的 markup 组，`Sound` 除外）：
    /// `/Contents` **shall** 是替代描述的来源。
    ContentsRequired,
    /// 子类型不显示文本：`/Contents`（PDF 1.4）**may** 给出替代描述。
    ContentsOptional,
    /// `Sound`：不需要用于朗读的替代描述；`/Contents` **may** 是给听障
    /// 用户的说明文本。
    SoundNote,
}

impl AnnotationDescriptionSource {
    /// 该来源下 `/Contents` 是否构成强制的替代描述通道（14.9.3 shall）。
    pub fn is_required(&self) -> bool {
        matches!(self, Self::ContentsRequired)
    }
}

/// 按 12.5.6.2 的分组把子类型映射为 14.9.3 的来源优先级。
///
/// 判定只用 [`AnnotationSubtype::is_markup`]（Table 169 的 Markup 列）与
/// `Sound` 一个显式例外，不复制子类型清单；未识别子类型
/// （[`AnnotationSubtype::Unknown`]）按「不显示文本」处理，因为规范只把
/// 显示文本的属性赋予标准 markup 类型。
pub fn annotation_description_source(subtype: &AnnotationSubtype) -> AnnotationDescriptionSource {
    if matches!(subtype, AnnotationSubtype::Sound) {
        return AnnotationDescriptionSource::SoundNote;
    }
    if subtype.is_markup() {
        AnnotationDescriptionSource::ContentsRequired
    } else {
        AnnotationDescriptionSource::ContentsOptional
    }
}

impl Annotation {
    /// 本注解的替代描述：来源优先级 + `/Contents` 的语言转义视图
    /// （14.9.3）。
    ///
    /// `/Contents` 缺失、不是可解码文本串之外的形态时返回
    /// [`LanguageError::Malformed`]（编码 shall 的落实点）；`Popup` 带
    /// `/Parent` 时按 12.5.6.2 NOTE 2 返回 `Ok(None)`（其文本由父注解提供）。
    /// 返回值同时给出 [`AnnotationDescriptionSource`]，调用方据此知道该条
    /// `/Contents` 是 shall 的来源还是可选的描述。
    pub fn alternate_description(
        &self,
        prevailing: Option<&LanguageIdentifier>,
    ) -> Result<Option<AnnotationDescription>, LanguageError> {
        let source = annotation_description_source(&self.subtype);
        if self.subtype == AnnotationSubtype::Popup && self.parent.is_some() {
            return Ok(None);
        }
        let Some(contents) = &self.contents else {
            return Ok(None);
        };
        if !matches!(contents, Value::String(_) | Value::HexString(_)) {
            return Err(LanguageError::Malformed(format!(
                "annotation /Contents must be a text string, found {}",
                contents.type_name()
            )));
        }
        let text = SemanticText::from_value(contents)?;
        Ok(Some(AnnotationDescription {
            source,
            language: text.effective_language(prevailing),
            text,
        }))
    }
}

/// 一条已解析的注解替代描述（14.9.3）。
#[derive(Debug, Clone, PartialEq)]
pub struct AnnotationDescription {
    /// `/Contents` 在 14.9.3 里的来源地位。
    pub source: AnnotationDescriptionSource,
    /// 串生效语言：转义优先于 prevailing Lang（14.9.3 NOTE 3 的 shall）。
    pub language: Option<LanguageIdentifier>,
    /// 去转义文本与逐段语言的完整视图。
    pub text: SemanticText,
}

impl AnnotationDescription {
    /// 去掉语言转义后的替代描述文本。
    pub fn plain_text(&self) -> String {
        self.text.plain_text()
    }
}
