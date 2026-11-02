//! 注解 `/TM`（modified text，修改后文本）的 Unicode 语言转义只读视图
//! （ISO 32000-1 7.9.2.2 与 14.9.2.1；形态与 `language_escape.rs` 的
//! `/T`、`/Subj`、`/NM` 四条完全一致）。
//!
//! # 条目出处（已核）
//!
//! - `pdftotext -raw docs/PDF32000_2008.pdf` 全文检索确认：**ISO
//!   32000-1:2008 的注解字典没有 `/TM` 条目**。Table 164「Entries common to
//!   all annotation dictionaries」（raw L21262–L21349）只有 `Type`、
//!   `Subtype`、`Rect`、`Contents`、`P`、`NM`、`M`、`F`、`AP`、`AS`、
//!   `Border`、`C`、`StructParent`、`OC`；Table 170「Additional entries
//!   specific to markup annotations」（raw L21664–L21751，含两页续表）另给
//!   `T`、`Popup`、`CA`、`RC`、`CreationDate`、`IRT`、`Subj`、`RT`、`IT`、
//!   `ExData`，**没有** `TM`，也没有同源的 `LT`。全文里 `TM` 只出现在
//!   Table 220 的字段映射名（raw L23651）与 3D 动画时间倍率（raw L27537）。
//! - `/TM`（连同 `/LT`）属 Acrobat 扩展、并在 ISO 32000-2 里补入标记类注解
//!   附加条目：`TM` 为 text string（PDF 1.4），语义是「注解内容被修改后应当
//!   显示的新文本」，与 `/Contents`（Table 164 raw L21272「Text that shall be
//!   displayed for the annotation」）配对使用。本库以 ISO 32000-1 为准，故
//!   解析器**不建模** `/TM`（`Annotation::dict` 的未建模条目通道，见
//!   `reader/annotation/mod.rs` 模块文档末段），本视图只将已接线 14.9.2.1
//!   的转义规则按需施加在该未建模文本串上。
//! - 转义适用性：14.9.2.1（raw L31940–L31941）把「串内 Unicode 语言转义
//!   覆盖 prevailing Lang entry」推广到**一切** Unicode 文本串，7.9.2.2
//!   （raw L4242–L4248）给出语法；故 `/TM` 与 `/Subj` 一样可带转义，本视图
//!   的判别与错误口径同 [`Annotation::name_semantic_text`]。
//!
//! # 只读边界
//!
//! 不改解析路径、不改 [`Annotation`] 的字段、不加写端：条目形态直接回查
//! [`Annotation::dict`] 里的 `TM` 值（沿用 `/DS` 那次「回查字典判别形态」的
//! 手法，见 `language_escape.rs` 的
//! [`Annotation::default_style_semantic_text`]）。`/TM` 没有 `/DS` 那样的
//! 宽容解析分支，故值形态判者与其余四条一致：非可解码文本串（名字、数字、
//! 数组、间接引用等）返回 [`LanguageError::Malformed`]，缺键返回
//! `Ok(None)`；读取注解本身不因残缺转义失败。

use crate::core::reader::accessibility::{
    LANGUAGE_ESCAPE, LanguageError, LanguageIdentifier, SemanticText,
};
use crate::core::reader::content::text_value;
use crate::core::types::Value;

use super::Annotation;

impl Annotation {
    /// 注解字典里未建模的 `TM` 值；只读，不改写解析结果。
    fn modified_text_entry(&self) -> Option<&Value> {
        self.dict.get("TM")
    }

    /// 修改后文本 `/TM` 里是否出现 7.9.2.2（raw L4242–L4248）的 Unicode
    /// 语言转义序列。
    ///
    /// 与 [`has_name_language_escape`](Annotation::has_name_language_escape)
    /// 同一口径：只做 U+001B 存在性判断、不校验转义是否合法；没有 `/TM` 时
    /// 为 `false`。
    pub fn has_modified_text_language_escape(&self) -> bool {
        self.modified_text_entry()
            .and_then(text_value)
            .is_some_and(|text| text.contains(LANGUAGE_ESCAPE))
    }

    /// 修改后文本 `/TM` 按 7.9.2.2 解析出的 [`SemanticText`] 视图：逐段的
    /// 转义语言与去转义文本（14.9.2.1 raw L31940–L31941 同款 shall）。
    ///
    /// 没有 `/TM` 时返回 `Ok(None)`；未闭合、长度非 2/4 或码字节非 ASCII
    /// 字母的残缺转义，以及不是可解码文本串的 `/TM` 值返回
    /// [`LanguageError::Malformed`]，错误串带 `annotation /TM` 上下文。
    pub fn modified_text_semantic_text(&self) -> Result<Option<SemanticText>, LanguageError> {
        let Some(value) = self.modified_text_entry() else {
            return Ok(None);
        };
        let text = SemanticText::from_value(value)
            .map_err(|error| LanguageError::Malformed(format!("annotation /TM: {error}")))?;
        Ok(Some(text))
    }

    /// 修改后文本 `/TM` 去掉全部语言转义后的自然文本；无 `/TM` 返回
    /// `Ok(None)`。
    ///
    /// 不含转义时与字典里解码后的原串相同——`Annotation` 不存 `/TM` 字段，
    /// 故原串本身仍只能经 [`Annotation::dict`] 取用。
    pub fn modified_text_text(&self) -> Result<Option<String>, LanguageError> {
        Ok(self
            .modified_text_semantic_text()?
            .map(|text| text.plain_text()))
    }

    /// 修改后文本 `/TM` 生效的语言（14.9.2.1 raw L31940–L31941 的 shall：
    /// 串内 Unicode 语言转义**覆盖** prevailing Lang entry）。
    ///
    /// 注解字典没有 `/Lang` 条目（Table 164/Table 170 均未定义），故
    /// prevailing 取目录 `/Lang`（14.9.2.3 raw L31955–L31956），由调用方传入
    /// `Document::document_language` 的结果；`None` 表示文档没有声明。
    /// 有转义时返回**首个转义**声明的语言并忽略 `prevailing`，无转义时原样
    /// 返回 `prevailing`，没有 `/TM` 时返回 `Ok(None)`。逐段语言见
    /// [`modified_text_semantic_text`](Self::modified_text_semantic_text)。
    pub fn modified_text_language(
        &self,
        prevailing: Option<&LanguageIdentifier>,
    ) -> Result<Option<LanguageIdentifier>, LanguageError> {
        Ok(self
            .modified_text_semantic_text()?
            .and_then(|text| text.effective_language(prevailing)))
    }
}
