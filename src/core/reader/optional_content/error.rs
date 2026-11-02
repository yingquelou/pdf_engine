//! Typed parse failures for `/OCProperties` and their construction helper.

use std::fmt;

use crate::core::reader::ParseError;

pub(super) type Result<T> = std::result::Result<T, OptionalContentError>;

/// Errors produced while parsing `/OCProperties`.
#[derive(Debug)]
pub enum OptionalContentError {
    /// An indirect object referenced by the structure could not be parsed.
    Parse(ParseError),
    /// The structure does not satisfy Tables 99–101.
    Invalid(String),
}

impl fmt::Display for OptionalContentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(error) => write!(f, "{error}"),
            Self::Invalid(message) => write!(f, "invalid optional content: {message}"),
        }
    }
}

impl std::error::Error for OptionalContentError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Parse(error) => Some(error),
            Self::Invalid(_) => None,
        }
    }
}

impl From<ParseError> for OptionalContentError {
    fn from(error: ParseError) -> Self {
        Self::Parse(error)
    }
}

pub(super) fn invalid(message: impl Into<String>) -> OptionalContentError {
    OptionalContentError::Invalid(message.into())
}
