use crate::pdf::types::{Dict, Document, Value};

use super::{FontError, FontResult};

/// The `/FontFile*` entry that stores an embedded font program.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontProgramSource {
    FontFile,
    FontFile2,
    FontFile3,
}

/// A decoded font program stream from a Type 1 font descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddedFontProgram {
    /// The descriptor key that references this stream.
    pub source: FontProgramSource,
    /// The stream `/Subtype`, when present (notably for `/FontFile3`).
    pub subtype: Option<String>,
    /// Decoded stream bytes.
    pub data: Vec<u8>,
}

impl Document {
    /// Resolve and decode the embedded font program in a Type 1 font descriptor.
    ///
    /// Stream filters and document encryption are handled by the document's
    /// standard stream decoder. This reads bytes only; it does not parse or
    /// validate the embedded font format.
    ///
    /// Returns `Ok(None)` when the font has no descriptor or the descriptor has
    /// no `/FontFile*` entry. Malformed references, non-stream values, multiple
    /// font program entries, and decode failures are reported as errors.
    pub fn type1_font_program(&self, value: &Value) -> FontResult<Option<EmbeddedFontProgram>> {
        let value = self.resolve_value(value)?;
        let dict = value
            .as_dict()
            .ok_or_else(|| FontError::Invalid("font value is not a dictionary".to_string()))?;
        if dict.get("Subtype").and_then(Value::as_name) != Some("Type1") {
            return Ok(None);
        }
        let Some(descriptor_value) = dict.get("FontDescriptor") else {
            return Ok(None);
        };
        let descriptor = self.resolve_value(descriptor_value)?;
        let descriptor = descriptor
            .as_dict()
            .ok_or_else(|| FontError::Invalid("/FontDescriptor is not a dictionary".into()))?;
        if descriptor.get("Type").and_then(Value::as_name) != Some("FontDescriptor") {
            return Err(FontError::Invalid(
                "/FontDescriptor has a missing or invalid /Type".into(),
            ));
        }

        let mut program_entry = None;
        for (key, source) in [
            ("FontFile", FontProgramSource::FontFile),
            ("FontFile2", FontProgramSource::FontFile2),
            ("FontFile3", FontProgramSource::FontFile3),
        ] {
            if let Some(value) = descriptor.get(key) {
                if program_entry.is_some() {
                    return Err(FontError::Invalid(
                        "/FontDescriptor contains multiple /FontFile* entries".into(),
                    ));
                }
                program_entry = Some((value, source, key));
            }
        }
        let Some((program_value, source, key)) = program_entry else {
            return Ok(None);
        };
        let program_value = self.resolve_value(program_value)?;
        let Value::Stream(stream_dict, _) = &program_value else {
            return Err(FontError::Invalid(format!(
                "/FontDescriptor /{key} is not a stream"
            )));
        };
        let subtype = optional_stream_subtype(stream_dict, key)?;
        if source == FontProgramSource::FontFile3 && subtype.is_none() {
            return Err(FontError::Invalid(
                "/FontDescriptor /FontFile3 stream is missing /Subtype".into(),
            ));
        }
        let data = self.decode_stream_bytes(&program_value).map_err(|error| {
            FontError::Invalid(format!(
                "failed to decode /FontDescriptor /{key} stream: {error}"
            ))
        })?;
        Ok(Some(EmbeddedFontProgram {
            source,
            subtype,
            data,
        }))
    }
}

fn optional_stream_subtype(dict: &Dict, key: &str) -> FontResult<Option<String>> {
    match dict.get("Subtype") {
        Some(Value::Name(subtype)) => Ok(Some(subtype.clone())),
        Some(_) => Err(FontError::Invalid(format!(
            "/FontDescriptor /{key} stream has an invalid /Subtype"
        ))),
        None => Ok(None),
    }
}
