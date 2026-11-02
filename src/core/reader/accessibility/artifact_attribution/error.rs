use std::fmt;

use crate::core::reader::content::MarkedContentError;
use crate::core::reader::structure::StructureError;

/// 读取归属诊断所需输入时的失败原因。
#[derive(Debug, Clone, PartialEq)]
pub enum ArtifactAttributionError {
    /// 页面标记内容无法配对或内容流无法解析。
    MarkedContent(MarkedContentError),
    /// 逻辑结构树损坏，无法执行规则 2。
    Structure(StructureError),
}

impl fmt::Display for ArtifactAttributionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MarkedContent(error) => error.fmt(f),
            Self::Structure(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for ArtifactAttributionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::MarkedContent(error) => Some(error),
            Self::Structure(error) => Some(error),
        }
    }
}

impl From<MarkedContentError> for ArtifactAttributionError {
    fn from(error: MarkedContentError) -> Self {
        Self::MarkedContent(error)
    }
}

impl From<StructureError> for ArtifactAttributionError {
    fn from(error: StructureError) -> Self {
        Self::Structure(error)
    }
}
