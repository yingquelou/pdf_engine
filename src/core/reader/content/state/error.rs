//! 内容流图形状态机的类型化失败原因与参数校验助手。

use crate::core::reader::page_content::PageContentError;

/// Errors produced while interpreting a content stream into graphics states.
#[derive(Debug, Clone, PartialEq)]
pub enum ContentStateError {
    /// An operator received an out-of-range or otherwise illegal value.
    InvalidValue { operator: String, message: String },
    /// The `q` save stack exceeded [`MAX_SAVE_DEPTH`].
    SaveStackTooDeep { limit: usize },
    /// A `gs` operand names a resource that is absent from `/Resources`.
    UnknownExtGState { name: String },
    /// A referenced ExtGState dictionary failed Table 58 validation.
    InvalidExtGState { name: String, message: String },
    /// The page content stream could not be obtained or parsed.
    PageContent(PageContentError),
}

impl std::fmt::Display for ContentStateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidValue { operator, message } => {
                write!(f, "operator '{operator}' has an invalid value: {message}")
            }
            Self::SaveStackTooDeep { limit } => {
                write!(f, "graphics state save stack exceeded {limit}")
            }
            Self::UnknownExtGState { name } => {
                write!(f, "gs references unknown ExtGState resource '{name}'")
            }
            Self::InvalidExtGState { name, message } => {
                write!(f, "ExtGState resource '{name}' is invalid: {message}")
            }
            Self::PageContent(error) => write!(f, "page content error: {error}"),
        }
    }
}

impl std::error::Error for ContentStateError {}

impl From<PageContentError> for ContentStateError {
    fn from(error: PageContentError) -> Self {
        Self::PageContent(error)
    }
}

pub(super) fn invalid(operator: &str, message: &str) -> ContentStateError {
    ContentStateError::InvalidValue {
        operator: operator.into(),
        message: message.into(),
    }
}

pub(super) fn require_finite(
    operator: &str,
    values: &[f64],
) -> std::result::Result<(), ContentStateError> {
    if values.iter().all(|v| v.is_finite()) {
        Ok(())
    } else {
        Err(invalid(operator, "values must be finite numbers"))
    }
}

pub(super) fn require_enum(
    operator: &str,
    value: i64,
    maximum: i64,
) -> std::result::Result<u8, ContentStateError> {
    if value >= 0 && value <= maximum {
        Ok(value as u8)
    } else {
        Err(invalid(
            operator,
            &format!("value {value} outside 0..={maximum}"),
        ))
    }
}
