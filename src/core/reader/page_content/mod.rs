//! Page content stream parsing with resource resolution.
//!
//! Provides [`Document::page_content_ops`] to parse a page's content stream
//! into a sequence of [`Op`] values, resolving resource names (fonts, XObjects,
//! color spaces, shadings, patterns, properties) through the page's `/Resources`
//! dictionary with inheritance from parent page tree nodes.
//!
//! Form XObjects are not automatically expanded; callers interpret `Op::PaintXObject`
//! and recursively parse the form's content stream if needed.

use std::collections::HashSet;

use crate::core::types::{Dict, Document, Value};

use super::content::{ContentStreamError, Op, parse_content_stream};

/// Errors that can occur during page content parsing.
#[derive(Debug, Clone, PartialEq)]
pub enum PageContentError {
    /// Page index is out of range or page tree is malformed.
    PageNotFound,
    /// Content stream or resources are malformed.
    MalformedContent,
    /// Content stream parsing failed.
    ContentStream(ContentStreamError),
    /// Content exceeds size or stream count limits.
    ContentTooLarge,
}

impl std::fmt::Display for PageContentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PageNotFound => write!(f, "page not found"),
            Self::MalformedContent => write!(f, "malformed content or resources"),
            Self::ContentStream(e) => write!(f, "content stream error: {}", e),
            Self::ContentTooLarge => write!(f, "content too large"),
        }
    }
}

impl std::error::Error for PageContentError {}

impl From<ContentStreamError> for PageContentError {
    fn from(e: ContentStreamError) -> Self {
        Self::ContentStream(e)
    }
}

type Result<T> = std::result::Result<T, PageContentError>;

/// Maximum depth for page tree traversal and `/Parent` chain walking.
const MAX_PAGE_TREE_DEPTH: usize = 64;
/// Maximum depth for `/Parent` chain walking.
const MAX_PARENT_CHAIN_DEPTH: usize = 64;
/// Maximum total content bytes (page + form XObjects).
const MAX_CONTENT_BYTES: usize = 64 * 1024 * 1024;
/// Maximum number of content streams (page + form XObjects).
const MAX_STREAM_COUNT: usize = 4096;

impl Document {
    /// Parse a page's content stream into a sequence of operators.
    ///
    /// Traverses the page tree to find the `n`th page (0-based), resolves its
    /// `/Contents` (single stream or array of streams) and `/Resources` (with
    /// inheritance from parent page tree nodes), then parses the content stream
    /// into [`Op`] values.
    ///
    /// Form XObjects referenced by `Op::PaintXObject` are not automatically
    /// expanded; callers must recursively parse form content streams if needed.
    ///
    /// Returns an error when the page index is out of range, the page structure
    /// is malformed, or the content exceeds the parsing limits.
    pub fn page_content_ops(&self, page_index: usize) -> Result<Vec<Op>> {
        let page_value = self.locate_page_node(page_index)?;
        let page = page_value.as_dict().ok_or(PageContentError::PageNotFound)?;

        let content_bytes = collect_page_content_bytes(self, page)?;
        let ops = parse_content_stream(&content_bytes)?;
        Ok(ops)
    }

    /// Resolve a resource name from a page's `/Resources` dictionary.
    ///
    /// Looks up `name` in the specified resource category (e.g., "XObject",
    /// "Font", "ColorSpace", "Shading", "Pattern", "Properties") within the
    /// page's `/Resources` dictionary. Returns `None` if the resource is not
    /// found or the category does not exist.
    ///
    /// Resource names are matched after decoding `#xx` escapes (ISO 32000-1
    /// 7.3.5). The returned value may be a direct object or an indirect
    /// reference; use [`Document::resolve_ref`] to dereference if needed.
    pub fn page_resource(
        &self,
        page_index: usize,
        category: &str,
        name: &str,
    ) -> Result<Option<Value>> {
        let page_value = self.locate_page_node(page_index)?;
        let page = page_value.as_dict().ok_or(PageContentError::PageNotFound)?;

        let resources =
            resolve_page_resources(self, page).ok_or(PageContentError::MalformedContent)?;
        let resources = resources
            .as_dict()
            .ok_or(PageContentError::MalformedContent)?;

        let category_dict = match resources.get(category) {
            Some(cat) => match resolve_value(self, cat) {
                Some(Value::Dict(d)) => d,
                _ => return Ok(None),
            },
            None => return Ok(None),
        };

        // Try exact match first, then try with #xx decoding
        if let Some(value) = category_dict.get(name) {
            return Ok(Some(value.clone()));
        }

        // Search for a key that matches after #xx decoding
        for (key, value) in category_dict.0.iter() {
            let decoded = decode_name_escapes(key);
            if decoded == name {
                return Ok(Some(value.clone()));
            }
        }

        Ok(None)
    }

    /// Locate the `page_index`th (0-based) page node through the catalog's
    /// `/Pages` tree. Shared by page-scoped reader APIs.
    pub(crate) fn locate_page_node(&self, page_index: usize) -> Result<Value> {
        let root = self
            .trailer
            .dict
            .get("Root")
            .ok_or(PageContentError::PageNotFound)?;
        let root = resolve_value(self, root).ok_or(PageContentError::PageNotFound)?;
        let root = root.as_dict().ok_or(PageContentError::PageNotFound)?;
        let pages_value = root.get("Pages").ok_or(PageContentError::PageNotFound)?;
        let pages_key = pages_value.as_ref();
        let pages = resolve_value(self, pages_value).ok_or(PageContentError::PageNotFound)?;
        let pages = pages.as_dict().ok_or(PageContentError::PageNotFound)?;
        find_page_node(self, pages, pages_key, page_index).ok_or(PageContentError::PageNotFound)
    }

    /// Resolve a page's `/Resources` dictionary by page index.
    pub(crate) fn locate_page_resources(&self, page_index: usize) -> Result<Option<Dict>> {
        let page_value = self.locate_page_node(page_index)?;
        let page = page_value.as_dict().ok_or(PageContentError::PageNotFound)?;
        let Some(resources) = resolve_page_resources(self, page) else {
            return Ok(None);
        };
        Ok(resources.as_dict().cloned())
    }
}

/// Resolve a value, dereferencing indirect references.
fn resolve_value(document: &Document, value: &Value) -> Option<Value> {
    match value {
        Value::Ref(num, generation) => document
            .get_object(*num)
            .ok()
            .flatten()
            .filter(|o| o.generation == *generation)
            .map(|o| o.value),
        _ => Some(value.clone()),
    }
}

/// Find the `index`th page with an iterative depth-first traversal.
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
/// and a depth bound.
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

/// Collect content bytes from a page's `/Contents` (single stream or array).
fn collect_page_content_bytes(document: &Document, page: &Dict) -> Result<Vec<u8>> {
    let contents = page
        .get("Contents")
        .ok_or(PageContentError::MalformedContent)?;
    let contents = resolve_value(document, contents).ok_or(PageContentError::MalformedContent)?;

    match contents {
        Value::Stream(_, _) => {
            let bytes = document
                .decode_stream_bytes(&contents)
                .map_err(|_| PageContentError::MalformedContent)?;
            if bytes.len() > MAX_CONTENT_BYTES {
                return Err(PageContentError::ContentTooLarge);
            }
            Ok(bytes)
        }
        Value::Array(arr) => {
            let mut total_bytes = Vec::new();
            let mut stream_count = 0usize;
            for item in arr.iter() {
                let item =
                    resolve_value(document, item).ok_or(PageContentError::MalformedContent)?;
                if let Value::Stream(_, _) = &item {
                    stream_count += 1;
                    if stream_count > MAX_STREAM_COUNT {
                        return Err(PageContentError::ContentTooLarge);
                    }
                    let bytes = document
                        .decode_stream_bytes(&item)
                        .map_err(|_| PageContentError::MalformedContent)?;
                    if !total_bytes.is_empty() {
                        total_bytes.push(b'\n');
                    }
                    total_bytes.extend_from_slice(&bytes);
                    if total_bytes.len() > MAX_CONTENT_BYTES {
                        return Err(PageContentError::ContentTooLarge);
                    }
                }
            }
            Ok(total_bytes)
        }
        _ => Err(PageContentError::MalformedContent),
    }
}

/// Decode `#xx` escapes in a PDF name (ISO 32000-1 7.3.5).
fn decode_name_escapes(name: &str) -> String {
    let mut result = String::with_capacity(name.len());
    let mut chars = name.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '#' {
            let hex: String = chars.by_ref().take(2).collect();
            if hex.len() == 2 {
                if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                    result.push(byte as char);
                } else {
                    result.push('#');
                    result.push_str(&hex);
                }
            } else {
                result.push('#');
                result.push_str(&hex);
            }
        } else {
            result.push(c);
        }
    }
    result
}

#[cfg(test)]
mod tests;
