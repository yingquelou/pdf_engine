/// Errors that can occur during content stream parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentStreamError {
    /// An operator received an unexpected number of operands.
    InvalidOperandCount {
        operator: String,
        expected: String,
        actual: usize,
    },
    /// An operand has an invalid value (e.g., out of range).
    InvalidOperandValue {
        operator: String,
        operand_index: usize,
        message: String,
    },
    /// Inline image structure is malformed (missing ID or EI).
    MalformedInlineImage,
}

impl std::fmt::Display for ContentStreamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidOperandCount {
                operator,
                expected,
                actual,
            } => write!(
                f,
                "operator '{}' expected {} operands, got {}",
                operator, expected, actual
            ),
            Self::InvalidOperandValue {
                operator,
                operand_index,
                message,
            } => write!(
                f,
                "operator '{}' operand {} is invalid: {}",
                operator, operand_index, message
            ),
            Self::MalformedInlineImage => write!(f, "malformed inline image (missing ID or EI)"),
        }
    }
}

impl std::error::Error for ContentStreamError {}

pub(super) type Result<T> = std::result::Result<T, ContentStreamError>;
