//! 类型化解析失败原因与构造辅助，及网格/图案的资源护栏上限。

use crate::core::reader::color_space::ColorSpaceError;

/// Shading/Pattern 解析失败原因。
#[derive(Debug, Clone, PartialEq)]
pub enum ShadingError {
    /// 字典、流数据或位流结构非法（缺键、类型错误、长度或维数不符）。
    Malformed(String),
    /// 暂不支持的着色/图案类型（如 PatternType 6/7 之外的扩展）。
    Unsupported(String),
    /// 颜色函数构造、维数校验或求值失败。
    Function(String),
}

impl std::fmt::Display for ShadingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Malformed(message) => write!(f, "malformed shading: {message}"),
            Self::Unsupported(message) => write!(f, "unsupported shading: {message}"),
            Self::Function(message) => write!(f, "shading function error: {message}"),
        }
    }
}

impl std::error::Error for ShadingError {}

pub(super) type Result<T> = std::result::Result<T, ShadingError>;

pub(super) fn malformed(message: impl Into<String>) -> ShadingError {
    ShadingError::Malformed(message.into())
}

pub(super) fn unsupported(message: impl Into<String>) -> ShadingError {
    ShadingError::Unsupported(message.into())
}

pub(super) fn from_color_space(error: ColorSpaceError) -> ShadingError {
    match error {
        ColorSpaceError::Malformed(message) => malformed(message),
        ColorSpaceError::Unsupported(message) => unsupported(message),
        ColorSpaceError::Function(message) => ShadingError::Function(message),
    }
}

/// 网格位流记录总数上限，防止损坏输入造成内存膨胀。
pub(super) const MAX_MESH_RECORDS: usize = 1_000_000;
/// 平铺图案内容流字节上限（与文本提取/页面内容一致）。
pub(super) const MAX_PATTERN_CONTENT: usize = 64 * 1024 * 1024;
