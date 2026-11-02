//! Content stream parser.
//!
//! Converts a byte stream into a sequence of [`Op`] values, handling inline
//! images specially and preserving unmodeled operators for roundtrip.

mod dict;
mod error;
mod operand;
mod stream;
mod value;

pub(crate) use dict::parse_dictionary_inner;
pub use error::ContentStreamError;
pub use stream::parse_content_stream;
pub(crate) use value::hex_upper;
