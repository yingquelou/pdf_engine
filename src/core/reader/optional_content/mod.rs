//! Optional content properties (PDF 8.11, Tables 99–101).
//!
//! [`Document::optional_content_properties`] parses the catalog's
//! `/OCProperties` entry into typed optional content groups (OCGs) and the
//! default configuration dictionary. Text extraction (`14.6` marked content,
//! see [`crate::core::reader::content::MarkedSection`]) suppresses sections
//! whose group the default configuration turns off; XObjects are not yet
//! filtered by group.

mod error;
mod model;
mod parse;
mod visibility;

pub use error::OptionalContentError;
pub use model::{
    BaseState, OptionalContentConfiguration, OptionalContentGroup, OptionalContentProperties,
};
pub use visibility::membership_visibility;

#[cfg(test)]
mod tests;
