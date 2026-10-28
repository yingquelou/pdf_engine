//! AcroForm 字段树对象分配与字典写出（ISO 32000-1 12.7.2–12.7.4）。
//!
//! 字段树在 assemble 主流程中先校验、再按前序分配对象号（根到叶），
//! 随后把每个节点写成独立间接对象：`/Fields`/`/Kids` 引用子节点，
//! 子节点 `/Parent` 回指父节点；带 `/Rect` 的终端节点是字段与 Widget
//! 合并字典，同时登记进页面 `/Annots`。`/AP` 句柄解析为 Form XObject
//! 流引用，与页面绘制及注解外观共享同一对象池。

use crate::core::builder::types::{
    AcroForm, FieldValue, Font, FormField, FormFieldType, FormOption, text_string_value,
};
use crate::core::types::{Array, Dict, Value};
use crate::core::writer::WriteError;

use super::real_array;

/// 字段树节点及其已分配的对象号；`children` 为 `slots` 下标。
pub(super) struct FieldSlot<'a> {
    pub field: &'a FormField,
    pub num: i64,
    pub parent_num: Option<i64>,
    /// 生效页号（自身 `/P` 覆盖或从父节点继承）。
    pub page: Option<usize>,
    pub children: Vec<usize>,
}

/// 校验整棵字段树：页号、矩形、`/FT` 继承、字段类型专属条目与外观句柄。
pub(super) fn validate(
    form: &AcroForm,
    page_count: usize,
    form_count: usize,
) -> Result<(), WriteError> {
    validate_default_fonts(form.default_fonts())?;
    for (page, field) in form.fields() {
        validate_node(field, Some(*page), None, page_count, form_count)?;
    }
    Ok(())
}

/// `/DR /Font` 资源名：非空、不含名称定界符、不重名。
fn validate_default_fonts(fonts: &[(String, Font)]) -> Result<(), WriteError> {
    let mut seen = std::collections::BTreeSet::new();
    let invalid = |message: String| WriteError::InvalidObject(message);
    for (name, _) in fonts {
        if name.is_empty() {
            return Err(invalid(
                "AcroForm /DR font resource name must not be empty".into(),
            ));
        }
        if name.chars().any(|c| {
            c.is_whitespace()
                || matches!(c, '/' | '[' | ']' | '(' | ')' | '<' | '>' | '{' | '}' | '%')
        }) {
            return Err(invalid(format!(
                "AcroForm /DR font resource name {name:?} contains name delimiters"
            )));
        }
        if !seen.insert(name.as_str()) {
            return Err(invalid(format!(
                "AcroForm /DR declares duplicate font resource name {name:?}"
            )));
        }
    }
    Ok(())
}

fn validate_node(
    field: &FormField,
    inherited_page: Option<usize>,
    inherited_type: Option<FormFieldType>,
    page_count: usize,
    form_count: usize,
) -> Result<(), WriteError> {
    let name = field.partial_name().unwrap_or("<widget>");
    let invalid = |message: String| WriteError::InvalidObject(message);
    let page = field.page().or(inherited_page);
    let field_type = field.field_type().or(inherited_type);

    if let Some(rect) = field.rect() {
        if rect.iter().any(|value| !value.is_finite()) {
            return Err(invalid(format!(
                "form field {name}: /Rect must contain four finite numbers"
            )));
        }
        if !field.kids().is_empty() {
            return Err(invalid(format!(
                "form field {name}: a widget must not declare children"
            )));
        }
        let page =
            page.ok_or_else(|| invalid(format!("form field {name}: widget has no page binding")))?;
        if page >= page_count {
            return Err(invalid(format!(
                "form field {name} attached to unknown page {page}"
            )));
        }
    } else {
        if field.kids().is_empty() {
            return Err(invalid(format!(
                "form field {name}: must declare a widget rectangle or children"
            )));
        }
        if field.partial_name().is_none() {
            return Err(invalid(
                "form field with children must have a partial name (/T)".into(),
            ));
        }
        if let Some(page) = page
            && page >= page_count
        {
            return Err(invalid(format!(
                "form field {name} attached to unknown page {page}"
            )));
        }
    }

    if field.kids().is_empty() && field_type.is_none() {
        return Err(invalid(format!("form field {name} is missing /FT")));
    }
    if let Some(quadding) = field.quadding()
        && quadding > 2
    {
        return Err(invalid(format!(
            "form field {name}: /Q must be 0, 1 or 2, got {quadding}"
        )));
    }
    if field.max_len().is_some() && field_type != Some(FormFieldType::Text) {
        return Err(invalid(format!(
            "form field {name}: /MaxLen is only valid for text fields"
        )));
    }
    if !field.options().is_empty() && field_type != Some(FormFieldType::Choice) {
        return Err(invalid(format!(
            "form field {name}: /Opt is only valid for choice fields"
        )));
    }
    for handle in [
        field.appearance_normal(),
        field.appearance_down(),
        field.appearance_rollover(),
    ]
    .into_iter()
    .flatten()
    {
        if handle >= form_count {
            return Err(invalid(format!(
                "form field {name}: /AP references unknown form handle {handle}"
            )));
        }
    }

    for child in field.kids() {
        validate_node(child, page, field_type, page_count, form_count)?;
    }
    Ok(())
}

/// 收集字段树引用的全部外观流句柄（供 assemble 并入 `used_forms`）。
pub(super) fn appearance_handles(form: &AcroForm) -> Vec<usize> {
    let mut handles = Vec::new();
    for (_, field) in form.fields() {
        collect_appearances(field, &mut handles);
    }
    handles
}

fn collect_appearances(field: &FormField, handles: &mut Vec<usize>) {
    handles.extend(
        [
            field.appearance_normal(),
            field.appearance_down(),
            field.appearance_rollover(),
        ]
        .into_iter()
        .flatten(),
    );
    for child in field.kids() {
        collect_appearances(child, handles);
    }
}

/// 按前序为整棵字段树分配对象号；返回扁平节点表与顶层根下标。
pub(super) fn allocate_fields<'a>(
    form: &'a AcroForm,
    num: &mut i64,
) -> (Vec<FieldSlot<'a>>, Vec<usize>) {
    let mut slots = Vec::new();
    let mut roots = Vec::new();
    for (page, field) in form.fields() {
        roots.push(allocate_node(field, Some(*page), None, num, &mut slots));
    }
    (slots, roots)
}

fn allocate_node<'a>(
    field: &'a FormField,
    inherited_page: Option<usize>,
    parent_num: Option<i64>,
    num: &mut i64,
    slots: &mut Vec<FieldSlot<'a>>,
) -> usize {
    let index = slots.len();
    let field_num = *num;
    *num += 1;
    slots.push(FieldSlot {
        field,
        num: field_num,
        parent_num,
        page: field.page().or(inherited_page),
        children: Vec::new(),
    });
    let page = slots[index].page;
    let mut children = Vec::with_capacity(field.kids().len());
    for child in field.kids() {
        children.push(allocate_node(child, page, Some(field_num), num, slots));
    }
    slots[index].children = children;
    index
}

/// 写出单个字段/Widget 节点字典。
pub(super) fn field_dict(
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
        for (key, handle) in [
            ("N", field.appearance_normal()),
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
    if let Some(tooltip) = field.tooltip() {
        dict = dict.insert("TU", text_string_value(tooltip));
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
