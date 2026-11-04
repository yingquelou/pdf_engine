//! Graphics state parameter dictionaries (PDF 8.4.5, Table 58).
//!
//! [`ExtGState`] exposes every standard entry with type and range checks.
//! Entries that carry function, halftone or soft-mask objects (`/SMask`,
//! `/TR`, `/TR2`, `/HT`, `/BG`, `/BG2`, `/UCR`, `/UCR2`) are preserved as
//! resolved raw values; [`Document::soft_mask`] additionally parses an
//! `/SMask` value into a typed [`SoftMask`] dictionary (11.6.5.2, Table 144)
//! and
//! `Document::halftone` (see the `halftone` submodule) parses an `/HT`
//! value into a [`HalftoneModel`]. Interpreting transfer functions remains
//! out of scope.
//! Unknown entries are kept verbatim in [`ExtGState::other`] without
//! resolving them.
//!
//! The two hybrid-colour-space keys `/CS` and `/CSS` (11.6.3 transparency
//! clause; PDF/VT-2 and later standardised as PDF 1.4 extensions) are
//! modelled through [`HybridColorSpace`] and [`HybridColorSpaceStyle`].
//! ISO 32000-1:2008 Table 58 does not list either key, so the reader uses
//! a *preserve-and-report* contract for these two entries: a value whose
//! shape falls outside the accepted model stays in [`ExtGState::other`]
//! under the same key and a matching [`GraphicsStateIssue`] is appended to
//! [`ExtGState::issues`]. No other entry follows this contract — malformed
//! Table 58 values still return [`GraphicsStateError::Invalid`], matching
//! the hard contract the other entries already have.

mod halftone;

use std::collections::BTreeMap;
use std::fmt;

use crate::core::types::{Dict, Document, Value};

pub use halftone::{CompositeHalftone, HalftoneModel, Type1Halftone, Type2Halftone};

use super::ParseError;

mod document;
mod error;
mod helpers;
mod model;
mod soft_mask;

pub use document::parse_ext_g_state;
pub use error::GraphicsStateError;
use helpers::invalid;
pub use model::{
    BlendMode, DashPattern, ExtGState, ExtGStateFont, GraphicsStateIssue, HybridColorSpace,
    HybridColorSpaceElement, HybridColorSpaceStyle, SoftMask, SoftMaskSubtype,
};

type Result<T> = std::result::Result<T, GraphicsStateError>;

#[cfg(test)]
mod tests;
