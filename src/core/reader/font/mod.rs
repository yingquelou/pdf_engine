//! Semantic access to PDF font dictionaries and embedded font programs.

use std::fmt;

use crate::core::types::{Document, Value};

use super::ParseError;

mod cff;
mod cid;
mod cid_cmap;
mod cid_type0;
mod descriptor;
mod embedded_program;
mod encoding;
mod encoding_data;
mod encoding_unicode;
pub(crate) mod predefined_cmaps;
mod simple;
mod standard14_metrics;
mod truetype;
mod type0;
mod type1_program;
mod type1c;
mod type3;

pub use cff::{
    CffGlyphOutline, CffOutlineParser, CffPathSegment, RawCffBackend, RawCffFont,
    TtfParserCffBackend, UnavailableCffBackend, subset_cff, subset_cff_type1c,
};
// builder 单元测试复用 CFF 合成样本（仅测试构建可见）。
#[cfg(test)]
pub(crate) use cff::fixtures;
pub use cid::{CidFontType2, CidSystemInfo, DefaultVerticalMetrics, VerticalMetrics};
pub use cid_type0::CidFontType0;
pub use descriptor::FontDescriptorMetrics;
pub use embedded_program::{EmbeddedFontProgram, FontProgramSource};
pub use encoding::{EncodingMapping, PredefinedEncoding, SimpleFontEncoding, strip_subset_tag};
pub(crate) use simple::SimpleFontToUnicode;
pub use simple::Type1Font;
pub use truetype::{
    CmapFormat, GlyphPoint, SfntFlavor, TrueTypeFont, TrueTypeGlyph, subset_truetype,
    subset_truetype_with_mapping,
};
pub use type0::{CidDescendant, Type0DecodedCode, Type0Font};
pub(crate) use type1_program::font_file_lengths;

pub use type1_program::{
    Type1BuiltinEncoding, Type1CharString, Type1FontProgram, Type1Header, Type1PathSegment,
    Type1Seac, subset_type1,
};
pub use type3::{Type3Font, Type3GlyphMetrics};

use descriptor::parse_descriptor;
pub(crate) use encoding::{code_for_char, default_encoding, predefined_encoding};

type FontResult<T> = std::result::Result<T, FontError>;

#[derive(Debug)]
pub enum FontError {
    /// An indirect object referenced by the font could not be parsed.
    Parse(ParseError),
    /// A font dictionary or metric does not satisfy the supported structure.
    Invalid(String),
}

impl fmt::Display for FontError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Parse(error) => write!(f, "{error}"),
            Self::Invalid(message) => write!(f, "invalid font: {message}"),
        }
    }
}

impl std::error::Error for FontError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Parse(error) => Some(error),
            Self::Invalid(_) => None,
        }
    }
}

impl From<ParseError> for FontError {
    fn from(error: ParseError) -> Self {
        Self::Parse(error)
    }
}

impl Document {
    fn resolve_value(&self, value: &Value) -> FontResult<Value> {
        match value {
            Value::Ref(num, generation) => {
                let object = self.get_object(*num)?.ok_or_else(|| {
                    FontError::Invalid(format!("unresolved object reference {num} {generation} R"))
                })?;
                Ok(object.value)
            }
            _ => Ok(value.clone()),
        }
    }
}

fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Integer(value) => Some(*value as f64),
        Value::Float(value) if value.is_finite() => Some(*value),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
