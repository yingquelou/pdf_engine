//! Unicode 语言转义序列（ISO 32000-1 7.9.2.2 与 14.9.3–14.9.5）。
//!
//! 规范原文（7.9.2.2「Text String Type」，raw 抽取：散文在 L4218–L4219，
//! 语法在 L4242–L4248）：
//!
//! > An escape sequence may appear anywhere in a Unicode text string to
//! > indicate the language in which subsequent text shall be written.
//! >
//! > The escape sequence shall consist of the following elements, in order:
//! > a) The Unicode value U+001B (that is, the byte sequence 0 followed by 27).
//! > b) A 2-byte ISO 639 language code.
//! > EXAMPLE en for English or ja for Japanese encoded as ASCII characters.
//! > c) (Optional) A 2-byte ISO 3166 country code.
//! > EXAMPLE US for the United States or JP for Japan.
//! > d) The Unicode value U+001B.
//!
//! 14.9.2.1（raw L31940–L31941）把它推广到**所有** Unicode 文本串：
//!
//! > Text strings encoded in Unicode may include an escape sequence or
//! > language tag indicating the language of the text and **overriding** the
//! > prevailing Lang entry (see 7.9.2.2, "Text String Type").
//!
//! 14.9.3（raw L32117–L32120）、14.9.4（raw L32158–L32160）与 14.9.5
//! （raw L32183–L32184）三处同款 shall：
//!
//! > Within alternate descriptions, Unicode escape sequences specifying
//! > language **shall override** the prevailing Lang entry.
//! >
//! > Such a sequence **shall override** the prevailing Lang entry.（ActualText）
//! >
//! > The expansion text, if encoded in Unicode, may include an escape
//! > sequence for indicating the language of the text (see 7.9.2.2, "Text
//! > String Type"). Such a sequence **shall override** the prevailing Lang
//! > entry.（E）
//!
//! # 本库形态
//!
//! [`SemanticText`] 把一个语义文本条目（Span 属性表或结构元素字典的
//! `/Alt`、`/ActualText`、`/E`，以及注解 `/Contents`）切成若干
//! [`LanguageTextSegment`]：每个转义把其后文本划入新的一段，段语言取
//! 转义声明的值；转义之前的段语言为 `None`，表示沿用 prevailing Lang。
//! [`SemanticText::effective_language`] 给出「转义优先于 prevailing
//! Lang」的整串结论，[`SemanticText::plain_text`] 给出去掉转义后的
//! 纯文本。
//!
//! 本切片把该视图接到 14.9.2.1 之外的三处条目与注解 `/Contents`：
//! `Span` 属性表的 `/Alt` `/ActualText` `/E`
//! （[`MarkedSection::span_semantic_text`](crate::core::reader::content::MarkedSection::span_semantic_text)）、
//! 结构元素字典的同名三条目
//! （[`StructureTree::element_semantic_text`](crate::core::reader::structure::StructureTree::element_semantic_text)）、
//! 多语言文本数组选中后的条目文本
//! （[`AlternateText::semantic_text_for`](crate::core::reader::accessibility::AlternateText::semantic_text_for)）
//! 与注解 `/Contents`
//! （[`Annotation::alternate_description`](crate::core::reader::annotation::Annotation::alternate_description)）。
//! 书签标题（12.3.3 `/Title`）也已接线，但走「保留原串 + 访问器」形态：
//! [`OutlineItem::title_semantic_text`](crate::core::reader::navigation::OutlineItem::title_semantic_text)
//! 等四个访问器按需解析，`OutlineItem::title` 字段仍是不去转义的原串。
//! 其余文本串（`/T` 标题、`/TU` 字段别名等）虽然按 14.9.2.1 同样
//! 可带转义，但本切片不接线，属诚实缺口。
//!
//! # 边界与库策略（不是 shall）
//!
//! - 7.9.2.2 把转义定义在 **Unicode 文本串** 中，且 a) 项写明它是
//!   「the byte sequence 0 followed by 27」，即 UTF-16BE 的 `00 1B`，解码后
//!   正是 Rust 字符 U+001B。解析器工作在**解码后的字符串**上，因此无需
//!   再判断源编码；实测 PDFDocEncoding 的第 27 字节按 Annex D 的控制码
//!   映射解码为 U+0007 而不是 U+001B（见
//!   [`crate::core::types::PdfTextString`] 的表），所以转义实际上只在
//!   UTF-16BE 文本串里可达——这与规范一致，不是本库的额外限制。
//! - 14.9.3（raw L32115–L32116）的 shall「Alternate descriptions are text
//!   strings, which shall be encoded in either PDFDocEncoding or Unicode
//!   character encoding」由上游 [`decode_text_string`] 落实：只有字面串与
//!   十六进制串可解码，其余形态返回 [`LanguageError::Malformed`]。本模块
//!   不重复判定编码集合，因此**拒绝非文本串是 shall 的落实，而「只能
//!   是这两种编码」是上游文本串解码既有策略**。
//! - 转义的 2/4 个码字节必须是 ASCII 字母（ISO 639/ISO 3166 的字符编码
//!   形态），拼出的标签再经 [`LanguageIdentifier::parse`] 做 RFC 3066
//!   语法校验；不合规（未闭合、长度非 2/4、含非字母）返回
//!   [`LanguageError::Malformed`]——规范未规定读者如何处置残缺转义，
//!   「报错而非静默当作文本」是**库策略**。
//! - 不做 BiDi、不猜语义：转义只声明语言，不改变字符顺序或朗读形式。

use std::fmt;

use crate::core::types::{Dict, Value};

use super::language::{LanguageError, LanguageIdentifier, decode_text_string, malformed};
use super::multi_language::AlternateText;

/// 7.9.2.2 a) 与 d) 的定界字符 U+001B。
pub const LANGUAGE_ESCAPE: char = '\u{001B}';

/// 语义文本条目名（14.9.3 `/Alt`、14.9.4 `/ActualText`、14.9.5 `/E`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SemanticTextEntry {
    /// `/Alt`：替代描述（14.9.3）。
    AlternateText,
    /// `/ActualText`：替换文本（14.9.4）。
    ActualText,
    /// `/E`：缩写展开（14.9.5）。
    Expansion,
}

impl SemanticTextEntry {
    /// 条目在字典里的键名。
    pub fn dict_key(self) -> &'static str {
        match self {
            Self::AlternateText => "Alt",
            Self::ActualText => "ActualText",
            Self::Expansion => "E",
        }
    }
}

impl fmt::Display for SemanticTextEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.dict_key())
    }
}

/// 一段共享同一生效语言的文本（转义切分的产物）。
#[derive(Debug, Clone, PartialEq)]
pub struct LanguageTextSegment {
    /// 该段的转义语言；`None` 表示位于任何转义之前，沿用 prevailing Lang。
    language: Option<LanguageIdentifier>,
    /// 去掉转义字符后的文本。
    text: String,
}

impl LanguageTextSegment {
    /// 该段由转义声明的语言；`None` 表示沿用 prevailing Lang。
    pub fn language(&self) -> Option<&LanguageIdentifier> {
        self.language.as_ref()
    }

    /// 该段文本（不含任何转义字符）。
    pub fn text(&self) -> &str {
        &self.text
    }
}

/// 一个语义文本条目按 7.9.2.2 语言转义解析后的视图。
#[derive(Debug, Clone, PartialEq)]
pub struct SemanticText {
    segments: Vec<LanguageTextSegment>,
    escape_count: usize,
}

impl SemanticText {
    /// 解析已解码文本串里的语言转义。
    ///
    /// 无 U+001B 时得到一个 `language()` 为 `None` 的单段；残缺或非法
    /// 转义返回 [`LanguageError::Malformed`]（见模块文档的库策略说明）。
    pub fn parse(text: &str) -> Result<Self, LanguageError> {
        if !text.contains(LANGUAGE_ESCAPE) {
            return Ok(Self {
                segments: vec![LanguageTextSegment {
                    language: None,
                    text: text.to_owned(),
                }],
                escape_count: 0,
            });
        }
        let chars: Vec<char> = text.chars().collect();
        let mut segments: Vec<LanguageTextSegment> = Vec::new();
        let mut buffer: String = String::new();
        let mut language: Option<LanguageIdentifier> = None;
        let mut escapes = 0usize;
        let mut index = 0usize;
        while index < chars.len() {
            if chars[index] != LANGUAGE_ESCAPE {
                buffer.push(chars[index]);
                index += 1;
                continue;
            }
            let opened_at = index;
            index += 1;
            let mut closed = index;
            while closed < chars.len() && chars[closed] != LANGUAGE_ESCAPE {
                closed += 1;
            }
            if closed >= chars.len() {
                return Err(malformed(format!(
                    "language escape at character offset {opened_at} is not terminated by U+001B"
                )));
            }
            let code: String = chars[index..closed].iter().collect();
            let count = closed - index;
            if count != 2 && count != 4 {
                return Err(malformed(format!(
                    "language escape at character offset {opened_at} must carry a 2-byte ISO 639 code or a 2-byte ISO 639 code with a 2-byte ISO 3166 code, found {count} characters"
                )));
            }
            if !code
                .chars()
                .all(|character| character.is_ascii_alphabetic())
            {
                return Err(malformed(format!(
                    "language escape at character offset {opened_at} carries {code:?}; ISO 639 and ISO 3166 codes shall be ASCII letters"
                )));
            }
            let tag = if count == 2 {
                code
            } else {
                let (primary, country) = code.split_at(2);
                format!("{primary}-{country}")
            };
            if !buffer.is_empty() {
                segments.push(LanguageTextSegment {
                    language: language.clone(),
                    text: std::mem::take(&mut buffer),
                });
            }
            language = Some(LanguageIdentifier::parse(&tag)?);
            escapes += 1;
            index = closed + 1;
        }
        if !buffer.is_empty() {
            segments.push(LanguageTextSegment {
                language: language.clone(),
                text: std::mem::take(&mut buffer),
            });
        }
        if segments.is_empty() {
            segments.push(LanguageTextSegment {
                language,
                text: String::new(),
            });
        }
        Ok(Self {
            segments,
            escape_count: escapes,
        })
    }

    /// 解析一个文本串值（14.9.3–14.9.5 的条目形态门禁在此生效）。
    ///
    /// 必须是字面串或十六进制串且可解码，否则返回
    /// [`LanguageError::Malformed`]；名字、数字、数组等形态一律拒绝。
    pub fn from_value(value: &Value) -> Result<Self, LanguageError> {
        let text = decode_text_string(value).ok_or_else(|| {
            malformed(format!(
                "semantic text shall be a text string, found {}",
                value.type_name()
            ))
        })?;
        Self::parse(&text)
    }

    /// 读取字典条目并给出转义视图；键缺失返回 `Ok(None)`。
    pub fn from_dict(dict: &Dict, entry: SemanticTextEntry) -> Result<Option<Self>, LanguageError> {
        let Some(value) = dict.get(entry.dict_key()) else {
            return Ok(None);
        };
        Self::from_value(value).map(Some)
    }

    /// 逐段视图：每段的转义语言与去转义文本。
    pub fn segments(&self) -> &[LanguageTextSegment] {
        &self.segments
    }

    /// 条目中出现过的转义个数。
    pub fn escape_count(&self) -> usize {
        self.escape_count
    }

    /// 是否至少含一个语言转义（即整串语言由转义决定）。
    pub fn has_language_escape(&self) -> bool {
        self.escape_count > 0
    }

    /// 去掉全部转义后的纯文本（各段拼接）。
    pub fn plain_text(&self) -> String {
        let mut text = String::new();
        for segment in &self.segments {
            text.push_str(segment.text());
        }
        text
    }

    /// 首个转义声明的语言；没有转义时返回 `None`。
    pub fn escape_language(&self) -> Option<&LanguageIdentifier> {
        self.segments
            .iter()
            .find_map(|segment| segment.language.as_ref())
    }

    /// 条目开头的语言，用于朗读整串（14.9.3/14.9.4/14.9.5 的 shall）。
    ///
    /// 存在转义时返回**首个转义**的语言并忽略 `prevailing`（转义 shall
    /// 覆盖 prevailing Lang）；没有转义时返回 `prevailing`。逐段语言请用
    /// [`segments`](Self::segments)。
    pub fn effective_language(
        &self,
        prevailing: Option<&LanguageIdentifier>,
    ) -> Option<LanguageIdentifier> {
        if self.escape_count > 0 {
            self.escape_language().cloned()
        } else {
            prevailing.cloned()
        }
    }
}

impl AlternateText {
    /// 多语言数组与语言转义同时存在时的次序（14.9.2.4 + 14.9.3）。
    ///
    /// 转义**不参与**数组条目选择：先用 prevailing Lang 按
    /// [`MultiLanguageTextArray::text_for`](super::multi_language::MultiLanguageTextArray::text_for)
    /// 的「精确→数组序前缀→默认文本」顺序选中一条文本，再对该条文本
    /// 应用语言转义，转义优先于 prevailing Lang。数组里找不到任何条目
    /// （也没有默认文本）时返回 `Ok(None)`。
    pub fn semantic_text_for(
        &self,
        prevailing: Option<&LanguageIdentifier>,
    ) -> Result<Option<SemanticText>, LanguageError> {
        let unknown = LanguageIdentifier::unknown();
        self.text_for(prevailing.unwrap_or(&unknown))
            .map(SemanticText::parse)
            .transpose()
    }
}

impl crate::core::reader::content::MarkedSection {
    /// 本区段 `Span` 属性表里某个语义文本条目的语言转义视图
    /// （14.9.3–14.9.5 + 7.9.2.2）。
    ///
    /// 标签不是精确的 `Span`、没有属性表或没有该键时返回 `Ok(None)`；
    /// 键在但不是可解码文本串返回 [`LanguageError::Malformed`]。与
    /// [`span_properties`](crate::core::reader::accessibility::SpanProperties::parse)
    /// 读同一个键，差别只在本 API 额外解析串内的语言转义。
    pub fn span_semantic_text(
        &self,
        entry: SemanticTextEntry,
    ) -> Result<Option<SemanticText>, LanguageError> {
        if self.tag != "Span" {
            return Ok(None);
        }
        let Some(properties) = self.properties.as_ref() else {
            return Ok(None);
        };
        SemanticText::from_dict(properties, entry)
    }
}

impl crate::core::reader::structure::StructureTree {
    /// 结构元素字典里某个语义文本条目的语言转义视图（14.9.3–14.9.5）。
    ///
    /// Table 323 把结构元素的 `/Alt`、`/ActualText`、`/E` 都定义为
    /// **文本串**（多语言文本数组只属于 media clip 的 `/Alt`，
    /// 14.9.2.4），读取复用结构解析器已解码的类型化字段，因此间接引用
    /// 形态的条目同样生效。下标越界、元素没有该条目时返回 `Ok(None)`；
    /// 本 API 只读元素自身字典，不做 14.8.5.3 的属性继承。
    pub fn element_semantic_text(
        &self,
        index: usize,
        entry: SemanticTextEntry,
    ) -> Result<Option<SemanticText>, LanguageError> {
        let Some(element) = self.element(index) else {
            return Ok(None);
        };
        let text = match entry {
            SemanticTextEntry::AlternateText => &element.alt,
            SemanticTextEntry::ActualText => &element.actual_text,
            SemanticTextEntry::Expansion => &element.expanded,
        };
        text.as_deref().map(SemanticText::parse).transpose()
    }
}
