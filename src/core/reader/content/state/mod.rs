//! Content-stream graphics state machine (ROADMAP stage C, item C3).
//!
//! Interprets the operator stream produced by [`super::parse_content_stream`]
//! into the device-independent graphics state defined by ISO 32000-1 §8.4
//! (graphics state, CTM, stroke parameters, `q/Q` stack, `gs` ExtGState
//! application), §9.3 (text state), §9.4 (text object matrices) and §8.6
//! (current colour/pattern values).
//!
//! Model and data only: paths, clipping geometry, marked-content structure and
//! show-text results are not tracked here. The saved state pushed by `q`
//! covers every tracked component (including the text matrices), matching the
//! §8.4.5 statement that `q` saves the entire device-independent graphics
//! state. `BT` resets the text matrix and text line matrix to identity but
//! leaves the other text state intact (§9.4.1).

mod color;
mod document;
mod error;
mod model;
mod runner;

pub use color::ColorValue;
pub use document::run_content_state;
pub use error::ContentStateError;
pub use model::{ContentGraphicsState, IDENTITY_MATRIX, concat_matrices};
pub use runner::ContentStateRunner;

/// Maximum number of simultaneously saved graphics states (`q` without `Q`).
pub const MAX_SAVE_DEPTH: usize = 512;
