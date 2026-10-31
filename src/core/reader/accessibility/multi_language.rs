//! 多语言文本数组与替代文本（ISO 32000-1 14.9.2.4，PDF 1.5）。

use std::collections::HashSet;

use crate::core::types::Value;

use super::language::{LanguageError, LanguageIdentifier, decode_text_string, malformed};

/// 多语言文本数组的条目对数上限。
const MAX_MULTI_LANGUAGE_ENTRIES: usize = 4096;

/// 多语言文本数组：`[identifier text identifier text ...]`。
///
/// 条目保持数组顺序；同一标识符（大小写不敏感，含空串）不得重复出现。
/// 空标识符对应的文本是默认文本。
#[derive(Debug, Clone, PartialEq)]
pub struct MultiLanguageTextArray {
    entries: Vec<(LanguageIdentifier, String)>,
}

impl MultiLanguageTextArray {
    /// 解析多语言文本数组。
    ///
    /// 必须是偶数长度数组，每项都是可解码的文本串；标识符须通过
    /// [`LanguageIdentifier::parse`]。重复标识符或形态非法返回
    /// [`LanguageError::Malformed`]，条目对数超过 4096 返回
    /// [`LanguageError::LimitExceeded`]。
    pub fn parse(value: &Value) -> Result<Self, LanguageError> {
        let Value::Array(items) = value else {
            return Err(malformed("multi-language text is not an array"));
        };
        if items.0.len() % 2 != 0 {
            return Err(malformed(format!(
                "multi-language text array has an odd number of items ({})",
                items.0.len()
            )));
        }
        let pairs = items.0.len() / 2;
        if pairs > MAX_MULTI_LANGUAGE_ENTRIES {
            return Err(LanguageError::LimitExceeded(
                "multi-language text array entries",
            ));
        }
        let mut entries = Vec::with_capacity(pairs);
        let mut seen = HashSet::with_capacity(pairs);
        for (index, pair) in items.0.as_chunks::<2>().0.iter().enumerate() {
            let identifier_text = decode_text_string(&pair[0]).ok_or_else(|| {
                malformed(format!(
                    "multi-language identifier {index} is not a text string ({})",
                    pair[0].type_name()
                ))
            })?;
            let identifier = LanguageIdentifier::parse(&identifier_text)?;
            if !seen.insert(identifier.clone()) {
                return Err(malformed(format!(
                    "multi-language identifier {identifier:?} appears more than once"
                )));
            }
            let text = decode_text_string(&pair[1]).ok_or_else(|| {
                malformed(format!(
                    "multi-language text {index} is not a text string ({})",
                    pair[1].type_name()
                ))
            })?;
            entries.push((identifier, text));
        }
        Ok(Self { entries })
    }

    /// 全部条目，保持数组声明顺序。
    pub fn entries(&self) -> &[(LanguageIdentifier, String)] {
        &self.entries
    }

    /// 空标识符对应的默认文本。
    pub fn default_text(&self) -> Option<&str> {
        self.entries
            .iter()
            .find(|(identifier, _)| identifier.is_unknown())
            .map(|(_, text)| text.as_str())
    }

    /// 按 14.9.2.4 的查找顺序取文本。
    ///
    /// 先做大小写不敏感的精确匹配；否则按数组顺序做前缀匹配（给定
    /// 标识符是数组标识符的前导子串，且其后紧跟 `-`）；都没有则回退
    /// 默认文本。给定标识符未知时只返回默认文本。
    pub fn text_for(&self, language: &LanguageIdentifier) -> Option<&str> {
        if language.is_unknown() {
            return self.default_text();
        }
        if let Some((_, text)) = self
            .entries
            .iter()
            .find(|(identifier, _)| identifier == language)
        {
            return Some(text);
        }
        for (identifier, text) in &self.entries {
            if is_prefix_language(language.as_str(), identifier.as_str()) {
                return Some(text);
            }
        }
        self.default_text()
    }
}

/// `given` 是否为 `candidate` 的大小写不敏感前导子串，且 `candidate`
/// 在该位置以一个 `-` 继续（如 `en` 匹配 `en-US`；`en-US` 不匹配 `en-GB`）。
fn is_prefix_language(given: &str, candidate: &str) -> bool {
    let given = given.as_bytes();
    let candidate = candidate.as_bytes();
    if given.is_empty() || candidate.len() <= given.len() {
        return false;
    }
    candidate[..given.len()].eq_ignore_ascii_case(given) && candidate[given.len()] == b'-'
}

/// 替代文本：普通文本串或多语言文本数组（14.9.2.4）。
#[derive(Debug, Clone, PartialEq)]
pub enum AlternateText {
    /// 普通文本串，与语言无关。
    Text(String),
    /// 多语言文本数组。
    MultiLanguage(MultiLanguageTextArray),
}

impl AlternateText {
    /// 解析 `/Alt`：文本串或多语言文本数组，其余形态非法。
    pub fn parse(value: &Value) -> Result<Self, LanguageError> {
        match value {
            Value::Array(_) => MultiLanguageTextArray::parse(value).map(Self::MultiLanguage),
            _ => decode_text_string(value).map(Self::Text).ok_or_else(|| {
                malformed(format!(
                    "alternate text is neither a text string nor an array ({})",
                    value.type_name()
                ))
            }),
        }
    }

    /// 取给定语言的文本；普通文本串恒返回自身。
    pub fn text_for(&self, language: &LanguageIdentifier) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text),
            Self::MultiLanguage(array) => array.text_for(language),
        }
    }
}
