//! 表单字段 `/TU` 替代名与 `/RV` 富文本值的 Unicode 语言转义视图
//! （ISO 32000-1 7.9.2.2、12.7.3.2、12.7.3.3 与 14.9.3）。
//!
//! 规范原文（`docs/PDF32000_2008.pdf` 的 `pdftotext -raw` 行号）：
//!
//! - Table 220「Entries common to all field dictionaries」的 `/TU` 行在
//!   L23645–L23650：「An alternate field name that shall be used in place of
//!   the actual field name wherever the field shall be identified in the
//!   user interface... This text is also useful when extracting the
//!   document's contents in support of accessibility to users with
//!   disabilities or for other purposes (see 14.9.3, "Alternate
//!   Descriptions")」（PDF 1.3）。
//! - Table 222「Additional entries common to all fields containing variable
//!   text」的 `/RV` 行在 L23767–L23771：「RV · text string or text stream ·
//!   (Optional; PDF 1.5) A rich text string, as described in 12.7.3.4, "Rich
//!   Text Strings."」（同表的 `/DS` 行 L23720–L23721 与注解侧 FreeText `/DS`
//!   是同一个富文本样式串概念）。Table 222 只把 `/DA`、`/Q` 标为 inheritable，
//!   `/DS` 与 `/RV` **没有** inheritable 标注，故本模块按节点自身字典读取、
//!   不做祖先回落（与 `/TU` 同一口径）。
//! - 14.9.3「Alternate Descriptions」L32111–L32114：字段的替代名「if
//!   present, shall be used in place of the actual field name when a
//!   conforming reader identifies the field in a user-interface. This
//!   alternate name, if provided, shall be specified using the TU entry of
//!   the field dictionary」，NOTE 2（L32114）「The TU entry is useful for
//!   vocalization purposes」。
//! - 14.9.3 L32115–L32116 的 encoding shall 与 L32117–L32120 的转义 shall
//!   「Within alternate descriptions, Unicode escape sequences specifying
//!   language shall override the prevailing Lang entry」同样适用于 `/TU`；
//!   14.9.2.1（L31940–L31941）把该覆盖规则推广到一切 Unicode 文本串。
//! - 7.9.2.2 的转义语法 a)–d) 在 L4242–L4248。
//!
//! 12.7.3.2「Field Names」（L23680–L23697）只要求各全限定名相同表示的
//! `/FT`、`/V`、`/DV` 一致（L23696–L23697），**没有**规定 `/TU` 在各表示间
//! 一致，也未给 `/TU` 定义继承（Table 220 未标注 inheritable）；因此本模块
//! 在节点 [`Field`] 与其归并视图 [`LogicalField`] 上分别按需解析各自暴露的
//! 替代名，不做跨表示的合并或回落。
//!
//! 形态与书签标题（12.3.3 `/Title`）、注解 `/T` 一致：`alternate_name` 保留
//! **不去转义的原串**，语言视图由此处只读访问器按需解析，解析路径与既有
//! 测试零改动。同族第二条目是字段富文本值 `/RV`（Table 222，本库未建模、
//! 经 `Field::dict` 回查），见 [`Field::rich_value_semantic_text`]。字段
//! 映射名 `/TM`（Table 220，raw L23651）仍未接线，属诚实缺口。

use crate::core::reader::accessibility::{
    LANGUAGE_ESCAPE, LanguageError, LanguageIdentifier, SemanticText,
};
use crate::core::reader::content::text_value;
use crate::core::types::Value;

use super::{Field, LogicalField};

/// 解析一个已解码文本串里的语言转义，并把错误归到本条 `/TU` 上。
fn parse_alternate_name(text: Option<&str>) -> Result<Option<SemanticText>, LanguageError> {
    let Some(text) = text else {
        return Ok(None);
    };
    SemanticText::parse(text)
        .map(Some)
        .map_err(|error| LanguageError::Malformed(format!("form field /TU: {error}")))
}

impl Field {
    /// 本节点替代名 `/TU` 里是否出现 7.9.2.2（raw L4242–L4248）的 Unicode
    /// 语言转义序列。
    ///
    /// 只做 U+001B 存在性判断，不校验转义是否合法；合法性由
    /// [`alternate_name_semantic_text`](Self::alternate_name_semantic_text)
    /// 负责。没有 `/TU` 时为 `false`。
    pub fn has_alternate_name_language_escape(&self) -> bool {
        self.alternate_name
            .as_deref()
            .is_some_and(|text| text.contains(LANGUAGE_ESCAPE))
    }

    /// 替代名 `/TU` 按 7.9.2.2 解析出的 [`SemanticText`] 视图：逐段的转义
    /// 语言与去转义文本（14.9.3 raw L32117–L32120）。
    ///
    /// 本节点没有 `/TU`（含只由祖先声明、不可继承的形态）时返回
    /// `Ok(None)`；未闭合、长度非 2/4 或码字节非 ASCII 字母的残缺转义返回
    /// [`LanguageError::Malformed`]（库策略：报错而非静默把残缺转义当作文本，
    /// 与 `accessibility::language_escape` 其余入口一致）。表单读取本身不因
    /// 残缺转义失败，只有索取语言视图时才报错。
    pub fn alternate_name_semantic_text(&self) -> Result<Option<SemanticText>, LanguageError> {
        parse_alternate_name(self.alternate_name.as_deref())
    }

    /// 替代名 `/TU` 去掉全部语言转义后的自然文本；没有 `/TU` 返回
    /// `Ok(None)`。
    ///
    /// 替代名不含转义时与 [`Field::alternate_name`] 相同。
    pub fn alternate_name_text(&self) -> Result<Option<String>, LanguageError> {
        Ok(self
            .alternate_name_semantic_text()?
            .map(|text| text.plain_text()))
    }

    /// 替代名 `/TU` 生效的语言（14.9.3 raw L32117–L32120 与 14.9.2.1
    /// raw L31940–L31941 的 shall：串内 Unicode 语言转义**覆盖** prevailing
    /// Lang entry）。
    ///
    /// 字段字典没有 `/Lang` 条目（Table 220 未定义），故 prevailing 取目录
    /// `/Lang`（14.9.2.3 raw L31955–L31956），由调用方传入
    /// `Document::document_language` 的结果；`None` 表示文档没有声明。
    /// 有转义时返回**首个转义**声明的语言并忽略 `prevailing`，没有转义时
    /// 原样返回 `prevailing`；没有 `/TU` 时返回 `Ok(None)`。逐段语言见
    /// [`alternate_name_semantic_text`](Self::alternate_name_semantic_text)。
    pub fn alternate_name_language(
        &self,
        prevailing: Option<&LanguageIdentifier>,
    ) -> Result<Option<LanguageIdentifier>, LanguageError> {
        Ok(self
            .alternate_name_semantic_text()?
            .and_then(|text| text.effective_language(prevailing)))
    }
    /// 富文本值 `/RV`（Table 222 的 raw L23767–L23771：「A rich text string,
    /// as described in 12.7.3.4, "Rich Text Strings."」，PDF 1.5）里是否出现
    /// 7.9.2.2（raw L4242–L4248）的 Unicode 语言转义序列。
    ///
    /// 本库不建模 `/RV`（值留在 [`Field::dict`]），故此处只读回查字典值的
    /// 形态。只有**文本串形态**才可能带转义：名字形态按 7.9.2.2 的定义对象
    /// 是 Unicode 文本串，一律为 `false`；文本流形态（内联流或指向流的间接
    /// 引用）本切片不解引用、不取流字节，同为 `false`。没有 `/RV` 也是
    /// `false`。
    pub fn has_rich_value_language_escape(&self) -> bool {
        self.rich_value_text_string()
            .and_then(text_value)
            .is_some_and(|text| text.contains(LANGUAGE_ESCAPE))
    }

    /// `/RV` 的字面串/十六进制串形态值；其余形态（名字、流、引用等）都不是
    /// 文本串通道，由调用方按形态分别处置。
    fn rich_value_text_string(&self) -> Option<&Value> {
        match self.dict.get("RV") {
            Some(value @ (Value::String(_) | Value::HexString(_))) => Some(value),
            _ => None,
        }
    }

    /// 富文本值 `/RV` 按 7.9.2.2 解析出的 [`SemanticText`] 逐段视图
    /// （14.9.3 raw L32117–L32120 与 14.9.2.1 raw L31940–L31941 的 shall）。
    ///
    /// 返回 `Ok(None)` 的三种情形：没有 `/RV`；`/RV` 是**名字**形态（名字
    /// 不是文本串，转义不适用，按注解 `/DS` 的先例不报错）；`/RV` 是**文本流**
    /// 形态（Table 222 允许的另一种写法，需要解引用并取流字节，本切片不扩，
    /// 故不报错也不给视图）。文本串形态下未闭合、码长非 2/4 或码字节非 ASCII
    /// 字母的残缺转义，以及既非文本串也非名字/流的其他形态（数字、数组、
    /// 字典、null 等），返回 [`LanguageError::Malformed`]，错误串带
    /// `form field /RV` 上下文。表单读取本身不因残缺转义失败，只有索取语言
    /// 视图时才报错。
    pub fn rich_value_semantic_text(&self) -> Result<Option<SemanticText>, LanguageError> {
        match self.dict.get("RV") {
            None => Ok(None),
            Some(value @ (Value::String(_) | Value::HexString(_))) => {
                SemanticText::from_value(value)
                    .map(Some)
                    .map_err(|error| LanguageError::Malformed(format!("form field /RV: {error}")))
            }
            Some(Value::Name(_)) | Some(Value::Stream(_, _)) | Some(Value::Ref(_, _)) => Ok(None),
            Some(other) => Err(LanguageError::Malformed(format!(
                "form field /RV shall be a text string or text stream, found {}",
                other.type_name()
            ))),
        }
    }

    /// 富文本值 `/RV` 去掉全部语言转义后的文本；没有 `/RV` 返回 `Ok(None)`。
    ///
    /// 文本串形态无转义时、以及名字形态下，返回值都等于字典里的原值文本
    /// （名字没有转义可去，同 [`Field::rich_value_semantic_text`] 的口径）；
    /// 文本流形态返回 `Ok(None)`，因为本切片不取流字节。
    pub fn rich_value_text(&self) -> Result<Option<String>, LanguageError> {
        if let Some(Value::Name(name)) = self.dict.get("RV") {
            return Ok(Some(name.clone()));
        }
        Ok(self
            .rich_value_semantic_text()?
            .map(|text| text.plain_text()))
    }

    /// 富文本值 `/RV` 生效的语言（14.9.3 raw L32117–L32120 与 14.9.2.1
    /// raw L31940–L31941 的 shall：串内 Unicode 语言转义**覆盖** prevailing
    /// Lang entry）。
    ///
    /// `prevailing` 由调用方传入 `Document::document_language` 的结果（字段
    /// 字典按 Table 220/Table 222 没有 `/Lang` 条目）。文本串形态有转义时
    /// 返回**首个转义**声明的语言并忽略 `prevailing`；无转义、名字形态与
    /// 文本流形态都原样返回 `prevailing`（前两者不携带转义，后者本切片不
    /// 读取，故都判为「无覆盖」）；没有 `/RV` 返回 `Ok(None)`。逐段语言见
    /// [`rich_value_semantic_text`](Self::rich_value_semantic_text)。
    pub fn rich_value_language(
        &self,
        prevailing: Option<&LanguageIdentifier>,
    ) -> Result<Option<LanguageIdentifier>, LanguageError> {
        match self.dict.get("RV") {
            None => Ok(None),
            Some(Value::Name(_)) | Some(Value::Stream(_, _)) | Some(Value::Ref(_, _)) => {
                Ok(prevailing.cloned())
            }
            Some(_) => Ok(self
                .rich_value_semantic_text()?
                .and_then(|text| text.effective_language(prevailing))),
        }
    }
}

/// 归并视图（12.7.3.2 的同全限定名多重表示）不含 `/RV` 通道：Table 222 只把
/// `/DA`、`/Q` 标为 inheritable，`/RV` 按节点自身字典读取，各表示的富文本值
/// 请用 [`LogicalField::representations`] 逐节点调用
/// [`Field::rich_value_semantic_text`]。
impl LogicalField<'_> {
    /// 归并视图暴露的替代名（前序首个表示的 `/TU`）里是否出现语言转义。
    ///
    /// 各表示自身的 `/TU` 请用
    /// [`representations`](LogicalField::representations) 逐节点调用
    /// [`Field::has_alternate_name_language_escape`]。
    pub fn has_alternate_name_language_escape(&self) -> bool {
        self.alternate_name
            .is_some_and(|text| text.contains(LANGUAGE_ESCAPE))
    }

    /// 归并视图替代名的 [`SemanticText`] 视图（14.9.3 raw L32117–L32120）。
    ///
    /// 语义与错误口径同 [`Field::alternate_name_semantic_text`]，只作用于
    /// [`LogicalField::alternate_name`]（前序首个表示声明的那一条）。
    pub fn alternate_name_semantic_text(&self) -> Result<Option<SemanticText>, LanguageError> {
        parse_alternate_name(self.alternate_name)
    }

    /// 归并视图替代名去掉语言转义后的自然文本；没有 `/TU` 返回 `Ok(None)`。
    pub fn alternate_name_text(&self) -> Result<Option<String>, LanguageError> {
        Ok(self
            .alternate_name_semantic_text()?
            .map(|text| text.plain_text()))
    }

    /// 归并视图替代名生效的语言：转义优先于 prevailing Lang（14.9.3），
    /// prevailing 由调用方传入目录 `/Lang`。
    pub fn alternate_name_language(
        &self,
        prevailing: Option<&LanguageIdentifier>,
    ) -> Result<Option<LanguageIdentifier>, LanguageError> {
        Ok(self
            .alternate_name_semantic_text()?
            .and_then(|text| text.effective_language(prevailing)))
    }
}
