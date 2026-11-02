//! 注解 `/T` 标题、`/Subj` 主题、`/NM` 注解名与 FreeText `/DS` 默认样式串的
//! Unicode 语言转义视图（ISO 32000-1
//! 7.9.2.2、12.5.2 Table 164、12.5.6.2 Table 170、12.5.6.3 Table 174 与
//! 14.9.2.1）。
//!
//! 规范原文（`docs/PDF32000_2008.pdf` 的 `pdftotext -raw` 行号）：
//!
//! - 7.9.2.2「Text String Type」：散文在 L4218–L4219「An escape sequence may
//!   appear anywhere in a Unicode text string to indicate the language in
//!   which subsequent text shall be written」，语法 a)–d) 在 L4242–L4248
//!   （U+001B + 2 字节 ISO 639 + 可选 2 字节 ISO 3166 + U+001B）。
//! - 14.9.2.1「Natural language specification」：L31940–L31941
//!   「Text strings encoded in Unicode may include an escape sequence or
//!   language tag indicating the language of the text and overriding the
//!   prevailing Lang entry」；L31926–L31928 与 14.9.2.3 的 L31955–L31956
//!   「The Lang entry in the document catalogue shall specify the default
//!   natural language for all text in the document」给出 prevailing Lang
//!   即目录 `/Lang`。
//! - Table 170「Additional entries specific to markup annotations」的 `/T`
//!   行在 L21666–L21668：「The text label that shall be displayed in the
//!   title bar of the annotation's pop-up window when open and active. This
//!   entry shall identify the user who added the annotation」（PDF 1.1）。
//!   Movie（L22388）与 Screen（L22404）注解另各有一条同键 `/T` 标题条目；
//!   本库按通用条目把 `/T` 解析为 [`Annotation::title`]，因此这些子类型
//!   共用同一视图。
//!
//! 形态与书签标题（12.3.3 `/Title`）一致：`Annotation::title`、
//! `Annotation::subject`、`Annotation::name` 与 FreeText 的
//! `Annotation::default_style` 保留**不去转义的原串**，语言视图按需由此处的
//! 访问器解析，解析路径与既有测试零改动。已接线的 Unicode 串共四条：
//!
//! - `/T` 弹出窗口标题（Table 170）与 `/Subj` 主题（Table 170）；
//! - `/NM` 注解名（12.5.2 Table 164，raw L21283–L21284「The annotation name,
//!   a text string uniquely identifying it among all the annotations on its
//!   page」，PDF 1.4）；
//! - `/DS` 默认样式串（12.5.6.3 Table 174，raw L21909「A default style
//!   string, as described in 12.7.3.4, "Rich Text Strings"」，PDF 1.5）。
//!
//! `/DS` 的形态与其余三条不同：解析器按库策略用 `parse_name_or_text` 宽容
//! 接受**名字**与**文本串**两种写法（Table 174 只声明 text string），并统一
//! 存成解码后的 `String`。7.9.2.2 的转义只定义在 Unicode 文本串里，故名字
//! 形态一律判为「无转义」且不报错——见
//! [`default_style_semantic_text`](Self::default_style_semantic_text) 的口径。
//! 同族视图另接了注解 `/Contents` 的语言转义逐段视图与注解字典里**未建模**的
//! `/TM`（Acrobat/ISO 32000-2 的「修改后文本」条目，ISO 32000-1 无此键），
//! 分别在 `language_escape_contents.rs` 与 `language_escape_modified_text.rs`；
//! 字段侧 `/RV` 在 `reader/form/language_escape.rs`。字段映射名 `/TM`
//! （Table 220，raw L23651）尚未接线，属诚实缺口。

use crate::core::reader::accessibility::{
    LANGUAGE_ESCAPE, LanguageError, LanguageIdentifier, SemanticText,
};
use crate::core::reader::content::text_value;
use crate::core::types::Value;

use super::Annotation;

impl Annotation {
    /// 弹出窗口标题 `/T` 里是否出现 7.9.2.2（raw L4242–L4248）的 Unicode
    /// 语言转义序列。
    ///
    /// 只做 U+001B 存在性判断，不校验转义是否合法；合法性由
    /// [`title_semantic_text`](Self::title_semantic_text) 负责。无 `/T`
    /// 时为 `false`。
    pub fn has_title_language_escape(&self) -> bool {
        self.title
            .as_ref()
            .and_then(text_value)
            .is_some_and(|text| text.contains(LANGUAGE_ESCAPE))
    }

    /// 标题 `/T` 按 7.9.2.2 解析出的 [`SemanticText`] 视图：逐段的转义
    /// 语言与去转义文本（14.9.2.1 raw L31940–L31941）。
    ///
    /// 没有 `/T` 时返回 `Ok(None)`；未闭合、长度非 2/4 或码字节非 ASCII
    /// 字母的残缺转义，以及不是可解码文本串的 `/T` 值返回
    /// [`LanguageError::Malformed`]（库策略：报错而非静默把残缺转义当作
    /// 文本，与 `accessibility::language_escape` 其余入口一致）。读取注解
    /// 本身不因残缺转义失败，只有索取语言视图时才报错。
    pub fn title_semantic_text(&self) -> Result<Option<SemanticText>, LanguageError> {
        let Some(title) = self.title.as_ref() else {
            return Ok(None);
        };
        let text = SemanticText::from_value(title)
            .map_err(|error| LanguageError::Malformed(format!("annotation /T: {error}")))?;
        Ok(Some(text))
    }

    /// 标题 `/T` 去掉全部语言转义后的自然文本；无 `/T` 返回 `Ok(None)`。
    ///
    /// 标题不含转义时与 [`Annotation::title`] 解码后的原串相同。
    pub fn title_text(&self) -> Result<Option<String>, LanguageError> {
        Ok(self.title_semantic_text()?.map(|text| text.plain_text()))
    }

    /// 标题 `/T` 生效的语言（14.9.2.1 raw L31940–L31941 的 shall：串内
    /// Unicode 语言转义**覆盖** prevailing Lang entry）。
    ///
    /// 注解字典没有 `/Lang` 条目（Table 164/Table 170 均未定义），故
    /// prevailing 取目录 `/Lang`（14.9.2.3 raw L31955–L31956「文档目录的
    /// Lang entry 为全文默认语言」），由调用方传入
    /// `Document::document_language` 的结果；`None` 表示文档没有声明。
    /// 有转义时返回**首个转义**声明的语言并忽略 `prevailing`，无转义时
    /// 原样返回 `prevailing`；没有 `/T` 时返回 `Ok(None)`。逐段语言见
    /// [`title_semantic_text`](Self::title_semantic_text)。
    pub fn title_language(
        &self,
        prevailing: Option<&LanguageIdentifier>,
    ) -> Result<Option<LanguageIdentifier>, LanguageError> {
        Ok(self
            .title_semantic_text()?
            .and_then(|text| text.effective_language(prevailing)))
    }

    /// 主题 `/Subj`（Table 170 的 L21706–L21707：「(Optional; PDF 1.5) Text
    /// representing a short description of the subject being addressed by the
    /// annotation」）里是否出现 7.9.2.2 的 Unicode 语言转义序列。
    ///
    /// 与 [`has_title_language_escape`](Self::has_title_language_escape) 同
    /// 一口径：只判断 U+001B 是否存在、不校验合法性；无 `/Subj` 为 `false`。
    pub fn has_subject_language_escape(&self) -> bool {
        self.subject
            .as_ref()
            .and_then(text_value)
            .is_some_and(|text| text.contains(LANGUAGE_ESCAPE))
    }

    /// 主题 `/Subj` 按 7.9.2.2 解析出的 [`SemanticText`] 视图：逐段的转义
    /// 语言与去转义文本（14.9.2.1 raw L31940–L31941 同款 shall）。
    ///
    /// 没有 `/Subj` 时返回 `Ok(None)`；残缺转义与不可解码的值返回
    /// [`LanguageError::Malformed`]，读取注解本身不因残缺转义失败。
    pub fn subject_semantic_text(&self) -> Result<Option<SemanticText>, LanguageError> {
        let Some(subject) = self.subject.as_ref() else {
            return Ok(None);
        };
        let text = SemanticText::from_value(subject)
            .map_err(|error| LanguageError::Malformed(format!("annotation /Subj: {error}")))?;
        Ok(Some(text))
    }

    /// 主题 `/Subj` 去掉全部语言转义后的自然文本；无 `/Subj` 返回 `Ok(None)`。
    ///
    /// 不含转义时与 [`Annotation::subject`] 解码后的原串相同。
    pub fn subject_text(&self) -> Result<Option<String>, LanguageError> {
        Ok(self.subject_semantic_text()?.map(|text| text.plain_text()))
    }

    /// 主题 `/Subj` 生效的语言（14.9.2.1 的 shall：串内 Unicode 语言转义
    /// **覆盖** prevailing Lang entry）。
    ///
    /// 注解字典没有 `/Lang` 条目，故 `prevailing` 由调用方传入
    /// `Document::document_language` 的结果；有转义时返回**首个转义**的语言
    /// 并忽略 `prevailing`，无转义时原样返回 `prevailing`，无 `/Subj` 返回
    /// `Ok(None)`。逐段语言见 [`subject_semantic_text`](Self::subject_semantic_text)。
    pub fn subject_language(
        &self,
        prevailing: Option<&LanguageIdentifier>,
    ) -> Result<Option<LanguageIdentifier>, LanguageError> {
        Ok(self
            .subject_semantic_text()?
            .and_then(|text| text.effective_language(prevailing)))
    }

    /// 注解名 `/NM`（12.5.2 Table 164，raw L21283–L21284：「The annotation
    /// name, a text string uniquely identifying it among all the annotations
    /// on its page」，PDF 1.4）里是否出现 7.9.2.2 的 Unicode 语言转义序列。
    ///
    /// 与 [`has_title_language_escape`](Self::has_title_language_escape)、
    /// [`has_subject_language_escape`](Self::has_subject_language_escape) 同一
    /// 口径：只判断 U+001B 是否存在、不校验转义是否合法；无 `/NM` 为 `false`。
    pub fn has_name_language_escape(&self) -> bool {
        self.name
            .as_ref()
            .and_then(text_value)
            .is_some_and(|text| text.contains(LANGUAGE_ESCAPE))
    }

    /// 注解名 `/NM` 按 7.9.2.2 解析出的 [`SemanticText`] 视图：逐段的转义语言
    /// 与去转义文本（14.9.2.1 raw L31940–L31941 同款 shall）。
    ///
    /// 没有 `/NM` 时返回 `Ok(None)`；未闭合、码长非 2/4、码字节非 ASCII 字母
    /// 的残缺转义返回 [`LanguageError::Malformed`]，错误串带
    /// `annotation /NM` 上下文；读取注解本身不因残缺转义失败，只有索取语言
    /// 视图时才报错。
    pub fn name_semantic_text(&self) -> Result<Option<SemanticText>, LanguageError> {
        let Some(name) = self.name.as_ref() else {
            return Ok(None);
        };
        let text = SemanticText::from_value(name)
            .map_err(|error| LanguageError::Malformed(format!("annotation /NM: {error}")))?;
        Ok(Some(text))
    }

    /// 注解名 `/NM` 去掉全部语言转义后的自然文本；无 `/NM` 返回 `Ok(None)`。
    ///
    /// 不含转义时与 [`Annotation::name`] 解码后的原串相同。注意注解名常被
    /// 用作跨更新的标识（如 `GoToE` 动作的 `/A`、`Markup3D` 的 `/3DA`），
    /// 那些入口按 12.6.4.4 消费**原串**，本访问器只影响语言视图，不改写
    /// `name` 字段。
    pub fn name_text(&self) -> Result<Option<String>, LanguageError> {
        Ok(self.name_semantic_text()?.map(|text| text.plain_text()))
    }

    /// 注解名 `/NM` 生效的语言（14.9.2.1 raw L31940–L31941 的 shall：串内
    /// Unicode 语言转义**覆盖** prevailing Lang entry）。
    ///
    /// 注解字典没有 `/Lang` 条目，故 `prevailing` 由调用方传入
    /// `Document::document_language` 的结果；有转义时返回**首个转义**声明的
    /// 语言并忽略 `prevailing`，无转义时原样返回 `prevailing`，无 `/NM` 返回
    /// `Ok(None)`。逐段语言见
    /// [`name_semantic_text`](Self::name_semantic_text)。
    pub fn name_language(
        &self,
        prevailing: Option<&LanguageIdentifier>,
    ) -> Result<Option<LanguageIdentifier>, LanguageError> {
        Ok(self
            .name_semantic_text()?
            .and_then(|text| text.effective_language(prevailing)))
    }

    /// `/DS` 是否为名字形态：解析器用 `parse_name_or_text` 同时接受名字与
    /// 文本串（见模块文档），此处回查原字典值判别形态，只读不改写。
    fn default_style_is_name(&self) -> bool {
        self.default_style.is_some() && self.dict.get("DS").and_then(Value::as_name).is_some()
    }

    /// FreeText 默认样式串 `/DS`（12.5.6.3 Table 174，raw L21909：「A default
    /// style string, as described in 12.7.3.4, "Rich Text Strings"」，
    /// PDF 1.5）里是否出现 7.9.2.2 的 Unicode 语言转义序列。
    ///
    /// 与 [`has_name_language_escape`](Self::has_name_language_escape) 的差别
    /// 只在形态：7.9.2.2 把转义定义在**Unicode 文本串**里，故名字形态的
    /// `/DS` 一律为 `false`（即使解码出的名字里真有 U+001B 也不按转义处理），
    /// 无 `/DS` 同为 `false`。
    pub fn has_default_style_language_escape(&self) -> bool {
        !self.default_style_is_name()
            && self
                .default_style
                .as_deref()
                .is_some_and(|text| text.contains(LANGUAGE_ESCAPE))
    }

    /// 默认样式串 `/DS` 按 7.9.2.2 解析出的 [`SemanticText`] 视图。
    ///
    /// 返回 `Ok(None)` 的三种情形：没有 `/DS`；`/DS` 是名字形态（名字不是
    /// 文本串，转义不适用，按库策略不报错，见模块文档）。文本串形态下残缺
    /// 转义返回 [`LanguageError::Malformed`]，错误串带 `annotation /DS`
    /// 上下文；与 `/T`、`/Subj`、`/NM` 一样，读取注解本身不因残缺转义失败。
    pub fn default_style_semantic_text(&self) -> Result<Option<SemanticText>, LanguageError> {
        if self.default_style_is_name() {
            return Ok(None);
        }
        let Some(text) = self.default_style.as_deref() else {
            return Ok(None);
        };
        let view = SemanticText::parse(text)
            .map_err(|error| LanguageError::Malformed(format!("annotation /DS: {error}")))?;
        Ok(Some(view))
    }

    /// 默认样式串 `/DS` 去掉全部语言转义后的文本；无 `/DS` 返回 `Ok(None)`。
    ///
    /// 文本串形态无转义时、以及名字形态下，返回值都等于
    /// [`Annotation::default_style`] 原串（名字没有转义可去）。
    pub fn default_style_text(&self) -> Result<Option<String>, LanguageError> {
        if self.default_style_is_name() {
            return Ok(self.default_style.clone());
        }
        Ok(self
            .default_style_semantic_text()?
            .map(|text| text.plain_text()))
    }

    /// 默认样式串 `/DS` 生效的语言（14.9.2.1 的 shall：串内 Unicode 语言转义
    /// **覆盖** prevailing Lang entry）。
    ///
    /// `prevailing` 由调用方传入 `Document::document_language` 的结果。文本串
    /// 形态有转义时返回**首个转义**的语言并忽略 `prevailing`；无转义与名字
    /// 形态都原样返回 `prevailing`（名字不携带转义，故永不覆盖）；没有 `/DS`
    /// 返回 `Ok(None)`。逐段语言见
    /// [`default_style_semantic_text`](Self::default_style_semantic_text)。
    pub fn default_style_language(
        &self,
        prevailing: Option<&LanguageIdentifier>,
    ) -> Result<Option<LanguageIdentifier>, LanguageError> {
        if self.default_style.is_none() {
            return Ok(None);
        }
        if self.default_style_is_name() {
            return Ok(prevailing.cloned());
        }
        Ok(self
            .default_style_semantic_text()?
            .and_then(|text| text.effective_language(prevailing)))
    }
}
