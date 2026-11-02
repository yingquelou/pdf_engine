//! Page-level extraction entry points on [`Document`].

use std::collections::HashSet;

use crate::core::types::{Dict, Document, Value};

use super::context::ExtractionContext;
use super::error::TextExtractionError;
use super::extract_text;
use super::resolve::resolve_value;
use super::state::TextState;
use super::{MAX_PAGE_TREE_DEPTH, MAX_PARENT_CHAIN_DEPTH, TextFragment};

impl Document {
    /// Extract Unicode text from a single content stream.
    ///
    /// `content_bytes` are the decoded stream bytes; `resources` is the
    /// page's (or parent's) `/Resources` dictionary. Font references in
    /// the content stream are resolved through `self`.
    pub fn extract_text(&self, content_bytes: &[u8], resources: &Dict) -> String {
        extract_text(content_bytes, resources, self)
    }

    /// Extract positioned text runs from a single content stream.
    ///
    /// The same decoding rules as [`Document::extract_text`] apply, but the
    /// output is split into runs carrying the text-space origin in default
    /// user space (see [`TextFragment`]).
    pub fn extract_text_fragments(
        &self,
        content_bytes: &[u8],
        resources: &Dict,
    ) -> Vec<TextFragment> {
        let mut context = ExtractionContext::new(self);
        let _ = context.extract_from_content(content_bytes, resources);
        let mut state = context.state;
        state.flush_fragment();
        state.fragments
    }

    /// Extract Unicode text from a page by its 0-based index.
    ///
    /// Traverses the page tree to find the `n`th page, resolves its
    /// `/Contents` (single stream or array of streams) and `/Resources`
    /// (with inheritance from parent page tree nodes), then extracts text.
    ///
    /// Returns `None` when the page index is out of range, the page structure
    /// is malformed, or the content exceeds the extraction limits. Use
    /// [`Document::try_extract_page_text`] to distinguish those cases.
    pub fn extract_page_text(&self, page_index: usize) -> Option<String> {
        self.try_extract_page_text(page_index).ok()
    }

    /// Fallible variant of [`Document::extract_page_text`].
    ///
    /// Page tree traversal and `/Parent` resource inheritance are bounded (64
    /// levels) and cycle-safe. Page content plus nested form XObjects share a
    /// 64 MiB / 4096-stream budget; exceeding it returns
    /// [`TextExtractionError::ContentTooLarge`] instead of truncating.
    pub fn try_extract_page_text(&self, page_index: usize) -> Result<String, TextExtractionError> {
        self.extract_page_state(page_index)
            .map(|state| state.output)
    }

    /// Positioned variant of [`Document::try_extract_page_text`].
    ///
    /// Returns one [`TextFragment`] per contiguous text run; see
    /// [`Document::extract_text_fragments`] for the position model.
    pub fn try_extract_page_text_fragments(
        &self,
        page_index: usize,
    ) -> Result<Vec<TextFragment>, TextExtractionError> {
        self.extract_page_state(page_index)
            .map(|state| state.fragments)
    }

    /// Infallible variant of [`Document::try_extract_page_text_fragments`].
    pub fn extract_page_text_fragments(&self, page_index: usize) -> Option<Vec<TextFragment>> {
        self.try_extract_page_text_fragments(page_index).ok()
    }

    /// Page-tree walk plus content extraction shared by the text and fragment
    /// entry points.
    fn extract_page_state(&self, page_index: usize) -> Result<TextState, TextExtractionError> {
        let root = self
            .trailer
            .dict
            .get("Root")
            .ok_or(TextExtractionError::PageNotFound)?;
        let root = resolve_value(self, root).ok_or(TextExtractionError::PageNotFound)?;
        let root = root.as_dict().ok_or(TextExtractionError::PageNotFound)?;
        let pages_value = root.get("Pages").ok_or(TextExtractionError::PageNotFound)?;
        let pages_key = pages_value.as_ref();
        let pages = resolve_value(self, pages_value).ok_or(TextExtractionError::PageNotFound)?;
        let pages = pages.as_dict().ok_or(TextExtractionError::PageNotFound)?;

        let page_value = find_page_node(self, pages, pages_key, page_index)
            .ok_or(TextExtractionError::PageNotFound)?;
        let page = page_value
            .as_dict()
            .ok_or(TextExtractionError::PageNotFound)?;

        let resources =
            resolve_page_resources(self, page).ok_or(TextExtractionError::MalformedContent)?;
        let resources = resources
            .as_dict()
            .ok_or(TextExtractionError::MalformedContent)?;

        let mut context = ExtractionContext::new(self);
        let content_bytes = context.collect_content_bytes(page)?;
        context.extract_from_content(&content_bytes, resources)?;
        let mut state = context.state;
        state.flush_fragment();
        Ok(state)
    }
}

/// Find the `index`th page with an iterative depth-first traversal.
///
/// `pages_ref` is the indirect reference the root `/Pages` node was resolved
/// from (if any) and seeds the visited set so a `/Kids` entry pointing back at
/// the root is rejected. Only indirect references are tracked; directly
/// embedded dictionaries cannot form a cycle because a value cannot contain
/// itself.
fn find_page_node(
    document: &Document,
    pages: &Dict,
    pages_ref: Option<(i64, i64)>,
    index: usize,
) -> Option<Value> {
    let mut visited: HashSet<(i64, i64)> = HashSet::new();
    if let Some(key) = pages_ref {
        visited.insert(key);
    }

    let mut stack = vec![(Value::Dict(pages.clone()), 0usize)];
    let mut page_number = 0usize;
    while let Some((node_value, depth)) = stack.pop() {
        if depth > MAX_PAGE_TREE_DEPTH {
            continue;
        }
        // Resolve refs lazily, but never enqueue the same ref twice. This is
        // what makes a cyclic `/Kids` graph terminate.
        let Some(node) = resolve_value(document, &node_value) else {
            continue;
        };
        let Some(node) = node.as_dict() else {
            continue;
        };
        match node.get("Type").and_then(Value::as_name).unwrap_or("") {
            "Page" => {
                if page_number == index {
                    return Some(Value::Dict(node.clone()));
                }
                page_number += 1;
            }
            "Pages" => {
                let Some(kids) = node.get("Kids") else {
                    continue;
                };
                let Some(kids) = resolve_value(document, kids) else {
                    continue;
                };
                let Some(kids) = kids.as_array() else {
                    continue;
                };
                for kid in kids.iter().rev() {
                    if let Some(key) = kid.as_ref()
                        && !visited.insert(key)
                    {
                        continue;
                    }
                    stack.push((kid.clone(), depth + 1));
                }
            }
            _ => {}
        }
    }
    None
}

/// Resolve a page's `/Resources`, walking `/Parent` links with a visited set
/// and a depth bound. A cycle stops the walk; returning the nearest ancestor
/// resources found so far.
fn resolve_page_resources(document: &Document, page: &Dict) -> Option<Value> {
    let mut current = page.clone();
    let mut visited: HashSet<(i64, i64)> = HashSet::new();
    for _ in 0..=MAX_PARENT_CHAIN_DEPTH {
        if let Some(resources) = current.get("Resources") {
            return resolve_value(document, resources);
        }
        let parent = current.get("Parent")?;
        if let Some(key) = parent.as_ref()
            && !visited.insert(key)
        {
            return None;
        }
        let parent = resolve_value(document, parent)?;
        current = parent.as_dict()?.clone();
    }
    None
}
