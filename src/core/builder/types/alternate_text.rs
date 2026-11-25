//! 替代文本 `/Alt`（14.9.2.3、14.9.2.4）写端模型。
//!
//! media clip data（Table 274）与 media clip section（Table 277）的 `/Alt`
//! 可为单一文本串或多语言文本数组；结构元素的 `/Alt`（Table 323）只能是
//! 文本串，不共用本类型。

use std::collections::HashSet;

use crate::core::builder::types::text_string_value;
use crate::core::reader::accessibility::LanguageIdentifier;
use crate::core::types::{Array, Value};
use crate::core::writer::WriteError;

/// 多语言文本数组条目对数上限（与读端 `MultiLanguageTextArray` 一致）。
const MAX_ENTRIES: usize = 4096;

/// `/Alt` 的两种合法形态。
#[derive(Debug, Clone, PartialEq)]
pub enum AlternateText {
    /// 单一文本串，与语言无关。
    Text(String),
    /// 多语言文本数组（14.9.2.4，PDF 1.5）：`[identifier text ...]`。
    MultiLanguage(Vec<(String, String)>),
}

impl AlternateText {
    /// 单一文本串。
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text(text.into())
    }

    /// 多语言文本数组；标识符须合法且唯一（大小写不敏感），空串为默认文本。
    pub fn multilingual(entries: impl IntoIterator<Item = (String, String)>) -> Self {
        Self::MultiLanguage(entries.into_iter().collect())
    }

    /// 写出为 PDF 对象。
    pub fn to_value(&self) -> Value {
        match self {
            Self::Text(text) => text_string_value(text),
            Self::MultiLanguage(entries) => {
                let mut array = Array::new();
                for (identifier, text) in entries {
                    array = array
                        .push(text_string_value(identifier))
                        .push(text_string_value(text));
                }
                Value::Array(array)
            }
        }
    }

    /// 结构校验：标识符合法、不重复（大小写不敏感）、条目数不超限。
    pub(crate) fn validate(&self) -> Result<(), WriteError> {
        let Self::MultiLanguage(entries) = self else {
            return Ok(());
        };
        if entries.len() > MAX_ENTRIES {
            return Err(WriteError::InvalidObject(format!(
                "media clip /Alt has {} entries (limit {MAX_ENTRIES})",
                entries.len()
            )));
        }
        let mut seen = HashSet::with_capacity(entries.len());
        for (index, (identifier, _)) in entries.iter().enumerate() {
            let parsed = LanguageIdentifier::parse(identifier).map_err(|error| {
                WriteError::InvalidObject(format!(
                    "media clip /Alt identifier {index} {identifier:?} is not a valid language tag: {error}"
                ))
            })?;
            if !seen.insert(parsed) {
                return Err(WriteError::InvalidObject(format!(
                    "media clip /Alt identifier {index} {identifier:?} appears more than once"
                )));
            }
        }
        Ok(())
    }
}

impl From<String> for AlternateText {
    fn from(text: String) -> Self {
        Self::Text(text)
    }
}

impl From<&str> for AlternateText {
    fn from(text: &str) -> Self {
        Self::Text(text.to_string())
    }
}

impl From<Vec<(String, String)>> for AlternateText {
    fn from(entries: Vec<(String, String)>) -> Self {
        Self::MultiLanguage(entries)
    }
}
