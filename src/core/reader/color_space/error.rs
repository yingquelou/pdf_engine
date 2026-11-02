//! 颜色空间解析与样本解码的类型化失败原因及其构造助手。

/// 颜色空间解析与样本解码的失败原因。
#[derive(Debug, Clone, PartialEq)]
pub enum ColorSpaceError {
    /// 颜色空间数组、字典或样本数据结构非法（缺键、类型错误、长度不足）。
    Malformed(String),
    /// 暂不支持的颜色空间类型或非法嵌套（如 Indexed 套 Indexed）。
    Unsupported(String),
    /// tint 变换函数构造或求值失败。
    Function(String),
}

impl std::fmt::Display for ColorSpaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Malformed(message) => write!(f, "malformed color space: {message}"),
            Self::Unsupported(message) => write!(f, "unsupported color space: {message}"),
            Self::Function(message) => write!(f, "tint function error: {message}"),
        }
    }
}

impl std::error::Error for ColorSpaceError {}

pub(super) type Result<T> = std::result::Result<T, ColorSpaceError>;

pub(super) fn malformed(message: impl Into<String>) -> ColorSpaceError {
    ColorSpaceError::Malformed(message.into())
}

pub(super) fn unsupported(message: impl Into<String>) -> ColorSpaceError {
    ColorSpaceError::Unsupported(message.into())
}
