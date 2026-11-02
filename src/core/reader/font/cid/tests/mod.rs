//! Tests for the CIDFontType2 reader.

use crate::core::types::{Dict, Value};

pub(super) fn cid_system_info() -> Value {
    Value::Dict(
        Dict::new()
            .insert("Registry", Value::String("Adobe".into()))
            .insert("Ordering", Value::String("Identity".into()))
            .insert("Supplement", Value::Integer(0)),
    )
}

pub(super) fn cid_font(entries: Vec<(&str, Value)>) -> Value {
    cid_font_with_descriptor(entries, Vec::new())
}

pub(super) fn cid_font_with_descriptor(
    entries: Vec<(&str, Value)>,
    descriptor_entries: Vec<(&str, Value)>,
) -> Value {
    let mut descriptor = Dict::new().insert("Type", Value::Name("FontDescriptor".into()));
    for (key, value) in descriptor_entries {
        descriptor = descriptor.insert(key, value);
    }
    let mut dict = Dict::new()
        .insert("Subtype", Value::Name("CIDFontType2".into()))
        .insert("BaseFont", Value::Name("ExampleCIDFont".into()))
        .insert("CIDSystemInfo", cid_system_info())
        .insert("FontDescriptor", Value::Dict(descriptor));
    for (key, value) in entries {
        dict = dict.insert(key, value);
    }
    Value::Dict(dict)
}

mod basic;
mod system_info;
mod vertical_metrics;
