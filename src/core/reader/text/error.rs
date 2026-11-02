//! Failure modes of page text extraction.

use std::fmt;

/// Failure modes of [`Document::try_extract_page_text`].
///
/// The infallible [`Document::extract_page_text`] collapses every error to
/// `None`; callers that need to distinguish malformed input from a resource
/// limit can use the fallible entry point instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextExtractionError {
    /// The requested page index is absent, or the page tree cannot be walked
    /// (cycle, excessive depth, or malformed page tree).
    PageNotFound,
    /// `/Contents`, `/Resources`, or a referenced object is missing or cannot
    /// be decoded.
    MalformedContent,
    /// The page's content streams (including nested form XObjects) exceed
    /// the extraction budget (64 MiB or 4096 streams).
    ContentTooLarge,
}

impl fmt::Display for TextExtractionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::PageNotFound => "page not found",
            Self::MalformedContent => "malformed page content",
            Self::ContentTooLarge => "page content exceeds extraction limits",
        };
        f.write_str(message)
    }
}

impl std::error::Error for TextExtractionError {}
