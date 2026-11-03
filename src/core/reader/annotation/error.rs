//! 注解读取的类型化错误形态。
//!
//! [`AnnotationError`] 的 `Display`/`Error` 实现与相邻模块错误
//! （`NavigationError`、`ActionError`）的 `From` 转换集中在此；错误构造助手
//! `malformed` 与 `Result` 别名仍留在父模块，供各解析子模块经 `super::` 使用。

use std::fmt;

use crate::core::reader::action::ActionError;
use crate::core::reader::navigation::NavigationError;

/// 注解读取中的类型化错误。
#[derive(Debug, Clone, PartialEq)]
pub enum AnnotationError {
    /// 文档目录缺失或无法解析为字典。
    CatalogMissing,
    /// 注解条目缺失、类型错误或不可解析（阻塞级字段校验失败）。
    Malformed(String),
    /// 页码越界或页面节点无法解析为字典。
    PageNotFound,
    /// 超出遍历深度或注解数量上限。
    LimitExceeded(&'static str),
}

impl fmt::Display for AnnotationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CatalogMissing => write!(f, "document catalog is missing or malformed"),
            Self::Malformed(message) => write!(f, "{message}"),
            Self::PageNotFound => write!(f, "page not found"),
            Self::LimitExceeded(what) => write!(f, "annotation limit exceeded: {what}"),
        }
    }
}

impl std::error::Error for AnnotationError {}

impl From<NavigationError> for AnnotationError {
    fn from(error: NavigationError) -> Self {
        match error {
            NavigationError::CatalogMissing => Self::CatalogMissing,
            NavigationError::Malformed(message) => Self::Malformed(message),
            NavigationError::PageNotFound => Self::PageNotFound,
            NavigationError::LimitExceeded(what) => Self::LimitExceeded(what),
        }
    }
}

impl From<ActionError> for AnnotationError {
    fn from(error: ActionError) -> Self {
        match error {
            ActionError::Malformed(message) => Self::Malformed(message),
            ActionError::PageNotFound => Self::PageNotFound,
        }
    }
}
