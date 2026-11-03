//! Font resource resolution for `Tf` lookups.

use crate::core::types::{Dict, Document, Value};

use super::super::font::TrueTypeFont;
use super::resolve::resolve_value;
use super::state::{FontHandle, TextState};

pub(super) fn resolve_fonts(resources: &Dict, document: &Document, state: &mut TextState) {
    let font_dict = match resources
        .get("Font")
        .and_then(|v| resolve_value(document, v))
    {
        Some(Value::Dict(d)) => d,
        _ => return,
    };
    for (name, font_value) in &font_dict.0 {
        let Some(font_val) = resolve_value(document, font_value) else {
            continue;
        };
        if let Ok(Some(type0)) = document.type0_font(&font_val) {
            state.fonts.insert(name.clone(), FontHandle::Type0(type0));
        } else {
            // Table 111 layout (`/Type1` plus the `/MMType1` instances of
            // 9.6.2.3) is the simple-font width lane; other subtypes report
            // `None` here and keep the encoding-only handle below.
            let font = document.simple_font_widths(&font_val).ok().flatten();
            let encoding = document.simple_font_encoding(&font_val).ok().flatten();
            let builtin = true_type_builtin_encoding(document, &font_val);
            let to_unicode = document.simple_font_to_unicode(&font_val);
            if font.is_some() || encoding.is_some() || font_val.as_dict().is_some() {
                state.fonts.insert(
                    name.clone(),
                    FontHandle::Simple {
                        font,
                        encoding,
                        builtin,
                        to_unicode,
                    },
                );
            }
        }
    }
}

/// Resolve the embedded program's built-in single-byte encoding for a
/// TrueType simple font that has no `/Encoding` entry.
///
/// Returns `None` when `/Encoding` is present, the font is not `/TrueType`,
/// or the embedded `/FontFile2` program cannot be parsed, so callers keep the
/// predefined-encoding fallback.
fn true_type_builtin_encoding(document: &Document, font_val: &Value) -> Option<Vec<Option<char>>> {
    let dict = font_val.as_dict()?;
    if dict.get("Encoding").is_some() {
        return None;
    }
    if dict.get("Subtype").and_then(Value::as_name) != Some("TrueType") {
        return None;
    }
    let font: TrueTypeFont = document.truetype_font(font_val).ok().flatten()?;
    font.builtin_encoding().ok()
}
