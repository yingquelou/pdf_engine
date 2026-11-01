//! 单个字段/Widget 节点字典的写出。

use crate::core::builder::types::{ActionRefs, FieldValue, FormOption, text_string_value};
use crate::core::types::{Array, Dict, Value};
use crate::core::writer::WriteError;

use super::super::real_array;
use super::FieldSlot;

/// 写出单个字段/Widget 节点字典。
pub(crate) fn field_dict(
    slot: &FieldSlot<'_>,
    slots: &[FieldSlot<'_>],
    page_refs: &[(i64, i64)],
    form_nums: &[i64],
) -> Result<Dict, WriteError> {
    let field = slot.field;
    let name = field.partial_name().unwrap_or("<widget>");
    let mut dict = Dict::new();

    if let Some(rect) = field.rect() {
        dict = dict
            .insert("Type", Value::Name("Annot".into()))
            .insert("Subtype", Value::Name("Widget".into()))
            .insert("Rect", real_array(&rect));
        if field.annotation_flags() != 0 {
            dict = dict.insert("F", Value::Integer(i64::from(field.annotation_flags())));
        }
        if let Some(page) = slot.page {
            let (page_num, generation) = page_refs.get(page).copied().ok_or_else(|| {
                WriteError::InvalidObject(format!(
                    "form field {name} attached to unknown page {page}"
                ))
            })?;
            dict = dict.insert("P", Value::Ref(page_num, generation));
        }
        let mut appearance = Dict::new();
        if field.appearance_states().is_empty() {
            if let Some(handle) = field.appearance_normal() {
                let num = form_nums.get(handle).ok_or_else(|| {
                    WriteError::InvalidObject(format!(
                        "form field {name}: /AP /N references unknown form handle {handle}"
                    ))
                })?;
                appearance = appearance.insert("N", Value::Ref(*num, 0));
            }
        } else {
            let mut states = Dict::new();
            for (state, handle) in field.appearance_states() {
                let num = form_nums.get(*handle).ok_or_else(|| {
                    WriteError::InvalidObject(format!(
                        "form field {name}: /AP /N state {state} references unknown form handle {handle}"
                    ))
                })?;
                states = states.insert(state.clone(), Value::Ref(*num, 0));
            }
            appearance = appearance.insert("N", Value::Dict(states));
        }
        for (key, handle) in [
            ("D", field.appearance_down()),
            ("R", field.appearance_rollover()),
        ] {
            if let Some(handle) = handle {
                let num = form_nums.get(handle).ok_or_else(|| {
                    WriteError::InvalidObject(format!(
                        "form field {name}: /AP /{key} references unknown form handle {handle}"
                    ))
                })?;
                appearance = appearance.insert(key, Value::Ref(*num, 0));
            }
        }
        if !appearance.0.is_empty() {
            dict = dict.insert("AP", Value::Dict(appearance));
        }
        if let Some(state) = field.appearance_state() {
            dict = dict.insert("AS", Value::Name(state.into()));
        }
    }

    if let Some(partial_name) = field.partial_name() {
        dict = dict.insert("T", text_string_value(partial_name));
    }
    if let Some(field_type) = field.field_type() {
        dict = dict.insert("FT", Value::Name(field_type.name().into()));
    }
    if field.flags() != 0 {
        dict = dict.insert("Ff", Value::Integer(i64::from(field.flags())));
    }
    if let Some(value) = field.value() {
        dict = dict.insert("V", field_value(value));
    }
    if let Some(value) = field.default_value() {
        dict = dict.insert("DV", field_value(value));
    }
    if let Some(appearance) = field.default_appearance() {
        dict = dict.insert("DA", text_string_value(appearance));
    }
    if let Some(quadding) = field.quadding() {
        dict = dict.insert("Q", Value::Integer(i64::from(quadding)));
    }
    if let Some(max_len) = field.max_len() {
        dict = dict.insert("MaxLen", Value::Integer(i64::from(max_len)));
    }
    if !field.options().is_empty() {
        dict = dict.insert("Opt", options_value(field.options()));
    }
    if let Some(top_index) = field.top_index() {
        dict = dict.insert("TI", Value::Integer(i64::from(top_index)));
    }
    if !field.alternate_indices().is_empty() {
        dict = dict.insert(
            "I",
            Value::Array(Array(
                field
                    .alternate_indices()
                    .iter()
                    .map(|&index| Value::Integer(i64::from(index)))
                    .collect(),
            )),
        );
    }
    if let Some(name) = field.alternate_name() {
        dict = dict.insert("TU", text_string_value(name));
    }
    if !field.actions().is_empty() {
        let mut additional = Dict::new();
        for (event, action) in field.actions() {
            additional = additional.insert(
                event.name(),
                Value::Dict(action.to_dict(&ActionRefs::default())?),
            );
        }
        dict = dict.insert("AA", Value::Dict(additional));
    }
    if !slot.children.is_empty() {
        let kids: Vec<Value> = slot
            .children
            .iter()
            .map(|&child| Value::Ref(slots[child].num, 0))
            .collect();
        dict = dict.insert("Kids", Value::Array(Array(kids)));
    }
    if let Some(parent) = slot.parent_num {
        dict = dict.insert("Parent", Value::Ref(parent, 0));
    }
    Ok(dict)
}

fn field_value(value: &FieldValue) -> Value {
    match value {
        FieldValue::Text(text) => text_string_value(text),
        FieldValue::Name(name) => Value::Name(name.clone()),
        FieldValue::Multiple(items) => Value::Array(Array(
            items.iter().map(|item| text_string_value(item)).collect(),
        )),
    }
}

fn options_value(options: &[FormOption]) -> Value {
    Value::Array(Array(
        options
            .iter()
            .map(|option| match option.display() {
                Some(display) => Value::Array(Array(vec![
                    text_string_value(option.export()),
                    text_string_value(display),
                ])),
                None => text_string_value(option.export()),
            })
            .collect(),
    ))
}
