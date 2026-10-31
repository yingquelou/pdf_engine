//! 内容流 Span 属性表（ISO 32000-1 14.9.2.1、14.9.3–14.9.5；PDF 1.4/1.5）。
//!
//! `BDC`/`DP` 的属性表以 `Span` 为标签时可携带：
//!
//! - `/Alt`：替代文本（14.9.3，PDF 1.5）；
//! - `/ActualText`：替换文本（14.9.4，PDF 1.5）；
//! - `/E`：展开缩写文本（14.9.5，PDF 1.4）；
//! - `/Lang`：内容语言（14.9.2.1，PDF 1.4）。
//!
//! 这四个条目**只能**是文本串（字面串或十六进制串）；名字、数字等
//! 形态一律返回 [`LanguageError::Malformed`]。
//!
//! 语言层级按 14.9.2.1/14.9.2.3 解析：Span 属性表的 `/Lang` 只作用于
//! 不在结构层级内的标记内容；属性表含整数 `/MCID` 的标记内容属于结构
//! 内容，其语言以结构元素为准（结构元素沿父链继承，最终回退目录
//! `/Lang`）。非 `Span` 标签的命名属性表不在此模块建模；表单 XObject
//! 的内容流不会被递归解析。

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::fmt;

use super::MAX_LANGUAGE_ANCESTORS;
use super::language::{LanguageError, LanguageIdentifier, decode_text_string, malformed};
use crate::core::reader::content::{MarkedContentError, MarkedSection};
use crate::core::types::{Dict, Document, Value};

/// 读取 Span 属性表或标记内容语言时的失败原因。
#[derive(Debug, Clone, PartialEq)]
pub enum MarkedContentLanguageError {
    /// 标记内容区段无法配对或内容流无法解析。
    MarkedContent(MarkedContentError),
    /// 语言标识或文本串形态非法。
    Language(LanguageError),
}

impl fmt::Display for MarkedContentLanguageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MarkedContent(error) => error.fmt(f),
            Self::Language(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for MarkedContentLanguageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::MarkedContent(error) => Some(error),
            Self::Language(error) => Some(error),
        }
    }
}

impl From<MarkedContentError> for MarkedContentLanguageError {
    fn from(error: MarkedContentError) -> Self {
        Self::MarkedContent(error)
    }
}

impl From<LanguageError> for MarkedContentLanguageError {
    fn from(error: LanguageError) -> Self {
        Self::Language(error)
    }
}

/// 一个 `Span` 标记内容属性表的类型化视图（14.9.3–14.9.5）。
///
/// 四个条目均可缺省；缺省时对应字段为 `None`。`/Lang` 为空串时映射为
/// [`LanguageIdentifier::unknown`]，表示「显式未知」并终止继承。
#[derive(Debug, Clone, PartialEq)]
pub struct SpanProperties {
    /// `/Alt`：替代（描述性）文本。
    pub alternate_text: Option<String>,
    /// `/ActualText`：替换实际渲染内容的文本。
    pub actual_text: Option<String>,
    /// `/E`：展开缩写文本。
    pub expansion: Option<String>,
    /// `/Lang`：内容语言。
    pub language: Option<LanguageIdentifier>,
}

impl SpanProperties {
    /// 解析 `tag` 标记内容的属性表。
    ///
    /// 标签不是精确的 `Span`、或属性表缺失时返回 `Ok(None)`；`Span`
    /// 属性表可以不含任何已知条目，此时返回全为 `None` 的 `Some`。位置
    /// 属性、可选内容等其他条目忽略。`/Alt`、`/ActualText`、`/E`、`/Lang`
    /// 不是文本串时返回 [`LanguageError::Malformed`]，`/Lang` 的语法
    /// 校验见 [`LanguageIdentifier::parse`]。
    pub fn parse(tag: &str, properties: Option<&Dict>) -> Result<Option<Self>, LanguageError> {
        if tag != "Span" {
            return Ok(None);
        }
        let Some(properties) = properties else {
            return Ok(None);
        };
        Ok(Some(Self {
            alternate_text: decode_text_entry(properties, "Alt")?,
            actual_text: decode_text_entry(properties, "ActualText")?,
            expansion: decode_text_entry(properties, "E")?,
            language: language_entry(properties)?,
        }))
    }
}

impl MarkedSection {
    /// 按 [`SpanProperties::parse`] 读取本区段的 Span 属性表。
    ///
    /// 非 `Span` 区段或没有属性表时返回 `Ok(None)`；文本串类型错误或
    /// `/Lang` 语法非法返回 [`LanguageError`]。
    pub fn span_properties(&self) -> Result<Option<SpanProperties>, LanguageError> {
        SpanProperties::parse(&self.tag, self.properties.as_ref())
    }
}

/// 解码一个文本串条目；键缺失返回 `Ok(None)`。
fn decode_text_entry(properties: &Dict, key: &str) -> Result<Option<String>, LanguageError> {
    let Some(value) = properties.get(key) else {
        return Ok(None);
    };
    if !matches!(value, Value::String(_) | Value::HexString(_)) {
        return Err(malformed(format!(
            "property list /{key} must be a text string, found {}",
            value.type_name()
        )));
    }
    decode_text_string(value).map(Some).ok_or_else(|| {
        malformed(format!(
            "property list /{key} is not a decodable text string ({})",
            value.type_name()
        ))
    })
}

/// 读取并校验 `/Lang`；键缺失返回 `Ok(None)`，空串返回未知语言。
fn language_entry(properties: &Dict) -> Result<Option<LanguageIdentifier>, LanguageError> {
    let Some(text) = decode_text_entry(properties, "Lang")? else {
        return Ok(None);
    };
    LanguageIdentifier::parse(&text).map(Some)
}

/// 读取 `/MCID` 的整数值。
///
/// 内容流的加载器把内联字典中的数字统一解析为 [`Value::Float`]，因此
/// 除 [`Value::Integer`] 外也接受值为整数的浮点数；`1.5` 之类的非整数
/// 不算合法的 `/MCID`。
fn mcid_value(properties: &Dict) -> Option<i64> {
    match properties.get("MCID") {
        Some(Value::Integer(mcid)) => Some(*mcid),
        Some(Value::Float(value))
            if value.is_finite()
                && value.fract() == 0.0
                && *value >= 0.0
                && *value <= i64::MAX as f64 =>
        {
            Some(*value as i64)
        }
        _ => None,
    }
}

/// 解析一个标记内容区段实际生效的语言（14.9.2.1、14.9.2.3）。
///
/// 从 `sections[index]` 开始沿 [`MarkedSection::parent`] 链向外查找，
/// 最多检查 [`MAX_LANGUAGE_ANCESTORS`] 个节点并防环：
///
/// 1. 当前区段的属性表含整数 `/MCID` 时，该区段属于结构内容：立即返回
///    `structured_language(properties)`；结构语言取不到时回退
///    `document_language`，**不会**继续使用外层 Span 的 `/Lang`。
/// 2. 否则标签为精确的 `Span` 且属性表含 `/Lang` 时，返回类型化语言；
///    空串映射为未知语言并同样终止查找。
/// 3. 否则继续检查外层区段。
///
/// 整条链都没有声明时返回 `document_language`。`/Lang` 或目录语言的
/// 语法/类型错误以 [`LanguageError`] 传播。
pub(crate) fn resolve_section_language(
    sections: &[MarkedSection],
    index: usize,
    structured_language: &dyn Fn(&Dict) -> Option<LanguageIdentifier>,
    document_language: Option<&LanguageIdentifier>,
) -> Result<Option<LanguageIdentifier>, LanguageError> {
    let mut current = Some(index);
    let mut visited = BTreeSet::new();
    for _ in 0..MAX_LANGUAGE_ANCESTORS {
        let Some(section_index) = current else {
            break;
        };
        if !visited.insert(section_index) {
            break;
        }
        let Some(section) = sections.get(section_index) else {
            break;
        };
        if let Some(properties) = section.properties.as_ref()
            && mcid_value(properties).is_some()
        {
            return Ok(structured_language(properties).or_else(|| document_language.cloned()));
        }
        if section.tag == "Span"
            && let Some(properties) = section.properties.as_ref()
            && properties.get("Lang").is_some()
        {
            return language_entry(properties);
        }
        current = section.parent;
    }
    Ok(document_language.cloned())
}

impl Document {
    /// 读取页面上每个标记内容区段实际生效的语言（14.9.2）。
    ///
    /// 返回的 [`Vec`] 与 [`Document::page_marked_content`] 一一对应（同一
    /// 关闭顺序、同样的 points）。每个条目按本模块的层级规则解析：
    ///
    /// - `Span` 属性表的 `/Lang` 覆盖其序列内（含嵌套标记内容）的文本；
    ///   空串表示显式未知并终止继承。
    /// - 属性表含整数 `/MCID` 的区段属于结构层级，语言改由结构元素决定：
    ///   通过页面 `/StructParents` 与父树数组定位元素，再沿结构元素父链
    ///   继承；结构元素没有声明时回退目录 `/Lang`。
    /// - 嵌套的另一个带 `/Lang` 的 `Span` 覆盖外层。
    ///
    /// 非 `Span` 标签的命名属性表（例如 `P` + `/Properties` 资源名）不在
    /// 此 API 的支持范围内；表单 XObject 的内容流不会被递归解析。结构
    /// 树或父树缺失、以及 [`StructureError`](crate::core::reader::structure::StructureError)
    /// 一律被容忍并回退目录语言；[`LanguageError`] 正常传播。
    pub fn page_marked_content_languages(
        &self,
        page_index: usize,
    ) -> Result<Vec<Option<LanguageIdentifier>>, MarkedContentLanguageError> {
        let sections = self.page_marked_content(page_index)?;
        let document_language = self.document_language()?;
        // 结构信息是可选增强：缺失或损坏时按目录语言处理。
        let page_parents = self.page_struct_parents(page_index).ok().flatten();
        let tree = self.structure_tree().ok().flatten();
        let structure_error = RefCell::new(None::<LanguageError>);
        let structured_language = |properties: &Dict| -> Option<LanguageIdentifier> {
            let mcid = usize::try_from(mcid_value(properties)?).ok()?;
            let parents = page_parents.as_ref()?;
            let element = parents.parents.get(mcid).copied().flatten()?;
            let tree = tree.as_ref()?;
            match tree.effective_language(element) {
                Ok(language) => language,
                Err(error) => {
                    let mut slot = structure_error.borrow_mut();
                    if slot.is_none() {
                        *slot = Some(error);
                    }
                    None
                }
            }
        };
        let mut languages = Vec::with_capacity(sections.len());
        for index in 0..sections.len() {
            languages.push(resolve_section_language(
                &sections,
                index,
                &structured_language,
                document_language.as_ref(),
            )?);
        }
        if let Some(error) = structure_error.borrow_mut().take() {
            return Err(error.into());
        }
        Ok(languages)
    }
}
