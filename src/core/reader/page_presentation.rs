//! 页面演示条目（12.4.4 Presentations、7.7.3.4 Table 30）：`/Dur` 停留
//! 时长与 `/Trans` 页面过渡。
//!
//! 两个条目均为 PDF 1.1 引入的页面字典条目，且不参与页面属性继承。
//! `/Trans` 复用过渡字典解析器 [`parse_transition`]（Table 162）；`/Dur`
//! 为秒数，缺省表示演示时不自动换页。

use std::fmt;

use crate::core::reader::navigation::resolve;
use crate::core::reader::page_content::PageContentError;
use crate::core::reader::transition::{Transition, TransitionError, parse_transition};
use crate::core::types::{Document, Value};

#[cfg(test)]
mod tests;

/// 页面演示条目解析的失败原因。
#[derive(Debug)]
pub enum PagePresentationError {
    /// 页面节点或页面树无法解析。
    Page(PageContentError),
    /// `/Trans` 过渡字典解析失败。
    Transition(TransitionError),
    /// 条目本身取值非法。
    Invalid(String),
}

impl fmt::Display for PagePresentationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Page(error) => write!(f, "{error}"),
            Self::Transition(error) => write!(f, "page /Trans: {error}"),
            Self::Invalid(message) => write!(f, "invalid page presentation: {message}"),
        }
    }
}

impl std::error::Error for PagePresentationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Page(error) => Some(error),
            Self::Transition(error) => Some(error),
            Self::Invalid(_) => None,
        }
    }
}

impl From<PageContentError> for PagePresentationError {
    fn from(error: PageContentError) -> Self {
        Self::Page(error)
    }
}

fn invalid(message: impl Into<String>) -> PagePresentationError {
    PagePresentationError::Invalid(message.into())
}

impl Document {
    /// 解析 `page_index`（0 基）页面字典的 `/Dur` 停留时长（秒）。
    ///
    /// 页面无 `/Dur` 时返回 `None`（演示时不自动换页）；负值视为非法。
    pub fn page_duration(&self, page_index: usize) -> Result<Option<f64>, PagePresentationError> {
        let Some(value) = page_entry(self, page_index, "Dur")? else {
            return Ok(None);
        };
        let seconds = match value {
            Value::Integer(number) => number as f64,
            Value::Float(number) if number.is_finite() => number,
            Value::Float(_) => return Err(invalid("page /Dur is not a finite number")),
            _ => return Err(invalid("page /Dur is not a number")),
        };
        if seconds < 0.0 {
            return Err(invalid("page /Dur is negative"));
        }
        Ok(Some(seconds))
    }

    /// 解析 `page_index`（0 基）页面字典的 `/Trans` 过渡字典（12.4.4）。
    ///
    /// 页面无 `/Trans` 时返回 `None`；条目可为直接字典或间接引用。
    pub fn page_transition(
        &self,
        page_index: usize,
    ) -> Result<Option<Transition>, PagePresentationError> {
        let Some(value) = page_entry(self, page_index, "Trans")? else {
            return Ok(None);
        };
        parse_transition(self, &value)
            .map(Some)
            .map_err(PagePresentationError::Transition)
    }
}

/// 读取页面字典条目并解引用；页面节点缺失或非字典时报错。
fn page_entry(
    document: &Document,
    page_index: usize,
    key: &str,
) -> Result<Option<Value>, PagePresentationError> {
    let page = document.locate_page_node(page_index)?;
    let dict = page
        .as_dict()
        .ok_or_else(|| invalid("page node is not a dictionary"))?;
    let Some(value) = dict.get(key) else {
        return Ok(None);
    };
    resolve(document, value)
        .map_err(|error| invalid(error.to_string()))
        .map(Some)
}
