//! Per-page extraction context: content-stream operator dispatch, form
//! XObject execution, page content collection and the shared byte budget.

use std::collections::HashSet;

use crate::core::reader::optional_content::OptionalContentProperties;
use crate::core::types::{Dict, Document, StreamData, Value};

use super::error::TextExtractionError;
use super::fonts::resolve_fonts;
use super::matrix::Matrix;
use super::operator::{execute_operator, name_operand};
use super::resolve::{form_matrix, resolve_value};
use super::state::{Operand, TextState, token_to_operand};
use super::tokenizer::{CsToken, tokenize_content_stream};
use super::{MAX_CONTENT_BYTES, MAX_CONTENT_STREAMS, MAX_FORM_DEPTH};

/// Per-page extraction state shared by the page content stream and any form
/// XObjects it invokes.
pub(super) struct ExtractionContext<'a> {
    pub(super) document: &'a Document,
    pub(super) state: TextState,
    budget: ContentBudget,
    /// Indirect form XObject references currently being executed, keyed by
    /// `(object number, generation)`. Only the active recursion chain is
    /// tracked so a form may be drawn repeatedly on the same page.
    active_forms: HashSet<(i64, i64)>,
    form_depth: usize,
    /// Catalog `/OCProperties`, loaded once so `BMC`/`BDC` sections can be
    /// filtered against the default optional-content configuration.
    pub(super) oc_properties: Option<OptionalContentProperties>,
}

impl<'a> ExtractionContext<'a> {
    pub(super) fn new(document: &'a Document) -> Self {
        Self {
            document,
            state: TextState::new(),
            budget: ContentBudget::new(),
            active_forms: HashSet::new(),
            form_depth: 0,
            oc_properties: document.optional_content_properties().ok().flatten(),
        }
    }

    /// Parse one content stream (page content or a form XObject body) and
    /// append its text to the shared output.
    pub(super) fn extract_from_content(
        &mut self,
        content_bytes: &[u8],
        resources: &Dict,
    ) -> Result<(), TextExtractionError> {
        resolve_fonts(resources, self.document, &mut self.state);

        let tokens = tokenize_content_stream(content_bytes);
        let mut operands: Vec<Operand> = Vec::new();
        for token in &tokens {
            match token {
                CsToken::ArrayOpen => {
                    operands.push(Operand::Array(Vec::new()));
                }
                CsToken::ArrayClose => {
                    let array = match operands.pop() {
                        Some(Operand::Array(a)) => Operand::Array(a),
                        _ => Operand::Array(Vec::new()),
                    };
                    operands.push(array);
                }
                CsToken::Keyword(kw) => {
                    match kw.as_str() {
                        "Do" => {
                            if let Some(Operand::Name(name)) = operands.last() {
                                self.draw_form(name, resources)?;
                            }
                        }
                        "BMC" => self.open_marked(name_operand(operands.first()), None, resources),
                        "BDC" => self.open_marked(
                            name_operand(operands.first()),
                            operands.get(1),
                            resources,
                        ),
                        "EMC" => self.state.end_marked(),
                        _ => execute_operator(kw, &operands, &mut self.state),
                    }
                    operands.clear();
                }
                _ => {
                    if let Some(op) = token_to_operand(token) {
                        if let Some(Operand::Array(arr)) = operands.last_mut() {
                            arr.push(op);
                        } else {
                            operands.push(op);
                        }
                    }
                }
            }
        }
        self.state.flush_fragment();
        Ok(())
    }

    /// Handle `Do` for a form XObject: resolve it through the current
    /// `/XObject` resource dictionary and append its text in place.
    ///
    /// Missing, malformed, image, and already visited XObjects are skipped so
    /// one bad resource does not fail the whole page. The content budget error
    /// is deliberately propagated.
    fn draw_form(&mut self, name: &str, resources: &Dict) -> Result<(), TextExtractionError> {
        let Some(xobject_map) = resources
            .get("XObject")
            .and_then(|value| resolve_value(self.document, value))
        else {
            return Ok(());
        };
        let Some(xobject_map) = xobject_map.as_dict() else {
            return Ok(());
        };
        let Some(xobject_value) = xobject_map.get(name) else {
            return Ok(());
        };
        let Some(resolved) = resolve_value(self.document, xobject_value) else {
            return Ok(());
        };
        let Some(dict) = resolved.as_dict() else {
            return Ok(());
        };
        let subtype = dict
            .get("Subtype")
            .and_then(|value| resolve_value(self.document, value));
        if subtype.as_ref().and_then(Value::as_name) != Some("Form") {
            return Ok(());
        }

        if self.form_depth >= MAX_FORM_DEPTH {
            return Ok(());
        }
        let active_key = xobject_value.as_ref();
        if let Some(key) = active_key
            && self.active_forms.contains(&key)
        {
            return Ok(());
        }

        // A form without its own `/Resources` inherits the caller's resources
        // (page or enclosing form), per ISO 32000-1 8.10.
        let form_resources = match dict.get("Resources") {
            Some(value) => match resolve_value(self.document, value)
                .and_then(|value| value.as_dict().cloned())
            {
                Some(dict) => dict,
                None => return Ok(()),
            },
            None => resources.clone(),
        };

        let bytes = match self.decode_stream(&resolved) {
            Ok(bytes) => bytes,
            Err(TextExtractionError::ContentTooLarge) => {
                return Err(TextExtractionError::ContentTooLarge);
            }
            Err(_) => return Ok(()),
        };

        // `Do` saves the graphics state (which includes the selected font),
        // runs the form, and restores it afterwards (ISO 32000-1 8.10.1).
        // Font resources are restored as well so a form's same-named font
        // cannot leak into the caller's `Tf` lookups. The form's `/Matrix` is
        // concatenated with the current transformation matrix, and its text
        // matrices start over because text objects cannot span streams.
        let form_matrix = form_matrix(self.document, dict.get("Matrix"));
        let saved_font_name = self.state.font_name.clone();
        let saved_fonts = self.state.fonts.clone();
        // Marked-content sections are lexically scoped to one stream; an
        // unclosed `BMC` in a form must not leak into the caller.
        let saved_marked = std::mem::take(&mut self.state.marked);
        let saved_ctm = self.state.ctm;
        let saved_text_matrix = self.state.text_matrix;
        let saved_line_matrix = self.state.line_matrix;
        self.state.flush_fragment();
        self.state.ctm = form_matrix.concat(saved_ctm);
        self.state.text_matrix = Matrix::IDENTITY;
        self.state.line_matrix = Matrix::IDENTITY;
        if let Some(key) = active_key {
            self.active_forms.insert(key);
        }
        self.form_depth += 1;
        let result = self.extract_from_content(&bytes, &form_resources);
        self.form_depth -= 1;
        if let Some(key) = active_key {
            self.active_forms.remove(&key);
        }
        self.state.flush_fragment();
        self.state.ctm = saved_ctm;
        self.state.text_matrix = saved_text_matrix;
        self.state.line_matrix = saved_line_matrix;
        self.state.font_name = saved_font_name;
        self.state.fonts = saved_fonts;
        self.state.marked = saved_marked;
        result
    }

    /// Collect the page `/Contents` bytes, charging every stream and byte to
    /// the shared budget.
    pub(super) fn collect_content_bytes(
        &mut self,
        page: &Dict,
    ) -> Result<Vec<u8>, TextExtractionError> {
        let contents = page
            .get("Contents")
            .ok_or(TextExtractionError::MalformedContent)?;
        let contents =
            resolve_value(self.document, contents).ok_or(TextExtractionError::MalformedContent)?;
        match contents {
            Value::Stream(_, _) => self.decode_stream(&contents),
            Value::Array(array) => {
                if array.len() > self.budget.streams_remaining {
                    return Err(TextExtractionError::ContentTooLarge);
                }
                let mut out = Vec::new();
                for item in array.iter() {
                    let item = resolve_value(self.document, item)
                        .ok_or(TextExtractionError::MalformedContent)?;
                    let bytes = self.decode_stream(&item)?;
                    if !out.is_empty() {
                        out.push(b'\n');
                    }
                    out.extend_from_slice(&bytes);
                }
                Ok(out)
            }
            _ => Err(TextExtractionError::MalformedContent),
        }
    }

    /// Decode one stream while charging it to the page-wide budget.
    ///
    /// The raw size is checked first when it is already in memory so an
    /// oversized stream is rejected before decoding; the decoded size is
    /// checked as well because filters can expand data substantially.
    fn decode_stream(&mut self, value: &Value) -> Result<Vec<u8>, TextExtractionError> {
        if self.budget.streams_remaining == 0 {
            return Err(TextExtractionError::ContentTooLarge);
        }
        // Reject oversized streams before reading or cloning their raw bytes.
        let raw_len = match value {
            Value::Stream(_, StreamData::Owned(raw)) => Some(raw.len()),
            Value::Stream(
                _,
                StreamData::Ref { len, .. } | StreamData::EncryptedRef { len, .. },
            ) => usize::try_from(*len).ok(),
            _ => None,
        };
        if raw_len.is_some_and(|len| len > self.budget.bytes_remaining) {
            return Err(TextExtractionError::ContentTooLarge);
        }
        self.budget.streams_remaining -= 1;
        let bytes = self
            .document
            .decode_stream_bytes(value)
            .map_err(|_| TextExtractionError::MalformedContent)?;
        if bytes.len() > self.budget.bytes_remaining {
            return Err(TextExtractionError::ContentTooLarge);
        }
        self.budget.bytes_remaining -= bytes.len();
        Ok(bytes)
    }
}

/// Byte/stream budget shared by page content and nested form XObjects.
struct ContentBudget {
    bytes_remaining: usize,
    streams_remaining: usize,
}

impl ContentBudget {
    fn new() -> Self {
        Self {
            bytes_remaining: MAX_CONTENT_BYTES,
            streams_remaining: MAX_CONTENT_STREAMS,
        }
    }
}
