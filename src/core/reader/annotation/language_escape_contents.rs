//! 注解 `/Contents` 内容文本的 Unicode 语言转义**逐段**只读视图
//! （ISO 32000-1 7.9.2.2、12.5.2 Table 164、12.5.6.2 与 14.9.2.1/14.9.3）。
//!
//! 规范原文（`pdftotext -raw docs/PDF32000_2008.pdf` 的行号）：
//!
//! - Table 164（raw L21272–L21277）把 `/Contents` 定为 text string：「Text
//!   that shall be displayed for the annotation or, if this type of
//!   annotation does not display text, an alternate description of the
//!   annotation's contents in human-readable form … (see 14.9.3, "Alternate
//!   Descriptions")」。
//! - 12.5.6.2「Markup Annotations」（raw L21633–L21660）按子类型分组说明它的
//!   语义，L21651–L21652 另有一条分段规则：「When separating text into
//!   paragraphs, a CARRIAGE RETURN (0Dh) shall be used and not, for example,
//!   a LINE FEED character (0Ah)」，NOTE 2（L21657–L21658）：「If an
//!   annotation has no parent, the Contents entry shall represent the text of
//!   the annotation, otherwise it shall be ignored」。
//! - 7.9.2.2（raw L4242–L4248）给出转义语法，14.9.2.1（raw L31940–L31941）
//!   把「串内转义覆盖 prevailing Lang」推广到一切 Unicode 文本串，14.9.3
//!   NOTE 3（raw L32117–L32120）对替代描述重申同一条 shall。
//!
//! # 「逐段」的确切所指
//!
//! 本模块的段是 [`SemanticText::segments`] 给出的**语言转义段**：每个
//! U+001B 序列把其后的文本划入新段，段语言取转义声明的值，首个转义之前的段
//! 语言为 `None`（沿用 prevailing Lang）。12.5.6.2 L21651–L21652 的
//! 回车分段（段落）是另一回事，本视图**不**做段落切分，也不改动原串。
//!
//! # 与 14.9.3 来源判定的分工
//!
//! [`Annotation::alternate_description`] 已经按 12.5.6.2 的分组给出「这条
//! `/Contents` 在 14.9.3 里是 shall 来源还是可选描述」，并据 NOTE 2 对带
//! `/Parent` 的 `Popup` 返回 `None`。本模块只补
//! 与 `/T`、`/Subj`、`/NM`、`/DS` 同族的**条目级**视图：读 `Annotation::contents`
//! 保留的未去转义原串，不做子类型来源判定（带父 `Popup` 的 `/Contents` 在这里
//! 仍按其字典原值展开），两条通道互不覆盖。
//!
//! 形态与 [`Annotation::title_semantic_text`] 一致：纯读端，解析路径与
//! [`Annotation::contents`] 字段零改动；残缺转义与不可解码的值返回
//! [`LanguageError::Malformed`]，读取注解本身不失败。

use crate::core::reader::accessibility::{
    LANGUAGE_ESCAPE, LanguageError, LanguageIdentifier, SemanticText,
};
use crate::core::reader::content::text_value;

use super::Annotation;

impl Annotation {
    /// 内容文本 `/Contents`（Table 164，raw L21272）里是否出现 7.9.2.2
    /// （raw L4242–L4248）的 Unicode 语言转义序列。
    ///
    /// 只做 U+001B 存在性判断，不校验转义是否合法；合法性由
    /// [`contents_semantic_text`](Self::contents_semantic_text) 负责。
    /// 没有 `/Contents` 时为 `false`。
    pub fn has_contents_language_escape(&self) -> bool {
        self.contents
            .as_ref()
            .and_then(text_value)
            .is_some_and(|text| text.contains(LANGUAGE_ESCAPE))
    }

    /// 内容文本 `/Contents` 按 7.9.2.2 解析出的 [`SemanticText`] 逐段视图：
    /// 每段的转义语言与去转义文本（14.9.2.1 raw L31940–L31941 与 14.9.3
    /// NOTE 3 raw L32117–L32120 的 shall）。
    ///
    /// 没有 `/Contents` 时返回 `Ok(None)`；未闭合、长度非 2/4 或码字节非
    /// ASCII 字母的残缺转义，以及不是可解码文本串的值返回
    /// [`LanguageError::Malformed`]，错误串带 `annotation /Contents` 上下文
    /// （库策略：报错而非静默把残缺转义当作文本，与既有四条视图一致）。
    pub fn contents_semantic_text(&self) -> Result<Option<SemanticText>, LanguageError> {
        let Some(contents) = self.contents.as_ref() else {
            return Ok(None);
        };
        let text = SemanticText::from_value(contents)
            .map_err(|error| LanguageError::Malformed(format!("annotation /Contents: {error}")))?;
        Ok(Some(text))
    }

    /// 内容文本 `/Contents` 去掉全部语言转义后的自然文本；无 `/Contents`
    /// 返回 `Ok(None)`。
    ///
    /// 不含转义时与 [`Annotation::contents`] 解码后的原串相同。返回文本里的
    /// 回车（12.5.6.2 L21651 的段落分隔符）按原样保留，本视图不做段落切分。
    pub fn contents_text(&self) -> Result<Option<String>, LanguageError> {
        Ok(self.contents_semantic_text()?.map(|text| text.plain_text()))
    }

    /// 内容文本 `/Contents` 生效的语言（14.9.2.1 raw L31940–L31941 与 14.9.3
    /// NOTE 3 的 shall：串内 Unicode 语言转义**覆盖** prevailing Lang entry）。
    ///
    /// 注解字典没有 `/Lang` 条目（Table 164/Table 170 均未定义），故
    /// prevailing 取目录 `/Lang`（14.9.2.3 raw L31955–L31956），由调用方传入
    /// `Document::document_language` 的结果；`None` 表示文档没有声明。
    /// 有转义时返回**首个转义**声明的语言并忽略 `prevailing`，无转义时原样
    /// 返回 `prevailing`，没有 `/Contents` 时返回 `Ok(None)`。逐段语言见
    /// [`contents_semantic_text`](Self::contents_semantic_text)。
    pub fn contents_language(
        &self,
        prevailing: Option<&LanguageIdentifier>,
    ) -> Result<Option<LanguageIdentifier>, LanguageError> {
        Ok(self
            .contents_semantic_text()?
            .and_then(|text| text.effective_language(prevailing)))
    }
}
