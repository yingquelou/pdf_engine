use super::{ParseError, fmt};

/// Errors produced while parsing a graphics state parameter dictionary.
#[derive(Debug)]
pub enum GraphicsStateError {
    /// An indirect object referenced by the dictionary could not be parsed.
    Parse(ParseError),
    /// The dictionary or one of its entries does not satisfy Table 58.
    Invalid(String),
}

impl fmt::Display for GraphicsStateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(error) => write!(f, "{error}"),
            Self::Invalid(message) => write!(f, "invalid graphics state: {message}"),
        }
    }
}

impl std::error::Error for GraphicsStateError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Parse(error) => Some(error),
            Self::Invalid(_) => None,
        }
    }
}

impl From<ParseError> for GraphicsStateError {
    fn from(error: ParseError) -> Self {
        Self::Parse(error)
    }
}
