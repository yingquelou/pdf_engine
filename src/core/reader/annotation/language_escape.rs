//! 注解 `/T` 标题的 Unicode 语言转义视图（ISO 32000-1 7.9.2.2 与 14.9.2.1）。
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
//! 形态与书签标题（12.3.3 `/Title`）一致：`Annotation::title` 保留**不去
//! 转义的原串**，语言视图按需由此处的访问器解析，解析路径与既有测试零改动。
//! `/Subj`、`/NM`、`/DS` 等其余 Unicode 串尚未接线，属诚实缺口。

use crate::core::reader::accessibility::{
    LANGUAGE_ESCAPE, LanguageError, LanguageIdentifier, SemanticText,
};
use crate::core::reader::content::text_value;

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
}
