//! Semantic representation of a PDF rectangle.

use std::fmt;

use super::{Array, Value};

/// A PDF rectangle represented by its four coordinates in array order.
///
/// PDF rectangle arrays contain `[llx, lly, urx, ury]`. Coordinates are
/// preserved as supplied; this type does not reorder reversed bounds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PdfRect {
    llx: f64,
    lly: f64,
    urx: f64,
    ury: f64,
}

/// An error encountered while parsing a PDF rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfRectError {
    ExpectedArray,
    InvalidArrayLength,
    NonNumericCoordinate { index: usize },
}

impl PdfRect {
    /// Create a rectangle from coordinates in PDF array order.
    pub fn new(llx: f64, lly: f64, urx: f64, ury: f64) -> Self {
        Self { llx, lly, urx, ury }
    }

    /// Parse a rectangle from a PDF array.
    pub fn from_array(array: &Array) -> Result<Self, PdfRectError> {
        if array.len() != 4 {
            return Err(PdfRectError::InvalidArrayLength);
        }

        let mut coordinates = [0.0; 4];
        for (index, (coordinate, value)) in coordinates.iter_mut().zip(array.iter()).enumerate() {
            *coordinate = match value {
                Value::Integer(number) => *number as f64,
                Value::Float(number) => *number,
                _ => return Err(PdfRectError::NonNumericCoordinate { index }),
            };
        }

        Ok(Self::new(
            coordinates[0],
            coordinates[1],
            coordinates[2],
            coordinates[3],
        ))
    }

    /// Parse a rectangle from a PDF array value.
    pub fn from_value(value: &Value) -> Result<Self, PdfRectError> {
        match value {
            Value::Array(array) => Self::from_array(array),
            _ => Err(PdfRectError::ExpectedArray),
        }
    }

    /// Serialize this rectangle to a PDF array value.
    pub fn to_value(self) -> Value {
        Value::Array(Array(vec![
            Value::Float(self.llx),
            Value::Float(self.lly),
            Value::Float(self.urx),
            Value::Float(self.ury),
        ]))
    }

    pub fn llx(&self) -> f64 {
        self.llx
    }

    pub fn lly(&self) -> f64 {
        self.lly
    }

    pub fn urx(&self) -> f64 {
        self.urx
    }

    pub fn ury(&self) -> f64 {
        self.ury
    }
}

impl fmt::Display for PdfRectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ExpectedArray => f.write_str("expected a PDF array for rectangle"),
            Self::InvalidArrayLength => {
                f.write_str("PDF rectangle array must contain exactly four coordinates")
            }
            Self::NonNumericCoordinate { index } => {
                write!(
                    f,
                    "PDF rectangle coordinate at index {index} is not numeric"
                )
            }
        }
    }
}

impl std::error::Error for PdfRectError {}
