//! 页面边界读取的类型化错误（14.11.2）。

use std::fmt;

use crate::core::reader::page_content::PageContentError;

/// 页面边界读取的失败原因。
#[derive(Debug, Clone, PartialEq)]
pub enum PageBoxError {
    /// 页面节点或页面树无法解析（含页码越界）。
    Page(PageContentError),
    /// 条目缺失、类型错误或取值非法。
    Malformed(String),
    /// 超出 `/Parent` 链继承深度上限。
    LimitExceeded(&'static str),
}

impl fmt::Display for PageBoxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Page(error) => write!(f, "invalid page box: {error}"),
            Self::Malformed(message) => write!(f, "invalid page box: {message}"),
            Self::LimitExceeded(what) => write!(f, "invalid page box: limit exceeded: {what}"),
        }
    }
}

impl std::error::Error for PageBoxError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Page(error) => Some(error),
            Self::Malformed(_) | Self::LimitExceeded(_) => None,
        }
    }
}

impl From<PageContentError> for PageBoxError {
    fn from(error: PageContentError) -> Self {
        Self::Page(error)
    }
}
