//! Type 1 程序解析失败的共享错误构造辅助。

use super::super::FontError;

pub(super) fn invalid_type1(message: impl Into<String>) -> FontError {
    FontError::Invalid(format!("invalid Type 1 font program: {}", message.into()))
}
