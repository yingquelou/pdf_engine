//! Graphics state parameter dictionaries (PDF 8.4.4, Table 58).
//!
//! [`ExtGState`] exposes every standard entry with type and range checks.
//! Entries that carry function, halftone or soft-mask objects (`/SMask`,
//! `/TR`, `/TR2`, `/HT`, `/BG`, `/BG2`, `/UCR`, `/UCR2`) are preserved as
//! resolved raw values; [`Document::soft_mask`] additionally parses an
//! `/SMask` value into a typed [`SoftMask`] dictionary and
//! `Document::halftone` (see the `halftone` submodule) parses an `/HT`
//! value into a [`HalftoneModel`]. Interpreting transfer functions remains
//! out of scope.
//! Unknown entries are kept verbatim in [`ExtGState::other`] without
//! resolving them.

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
pub use model::{BlendMode, DashPattern, ExtGState, ExtGStateFont, SoftMask, SoftMaskSubtype};

type Result<T> = std::result::Result<T, GraphicsStateError>;

#[cfg(test)]
mod tests;
