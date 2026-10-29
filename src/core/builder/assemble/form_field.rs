//! AcroForm 字段树对象分配与字典写出（ISO 32000-1 12.7.2–12.7.4）。
//!
//! 字段树在 assemble 主流程中先校验、再按前序分配对象号（根到叶），
//! 随后把每个节点写成独立间接对象：`/Fields`/`/Kids` 引用子节点，
//! 子节点 `/Parent` 回指父节点；带 `/Rect` 的终端节点是字段与 Widget
//! 合并字典，同时登记进页面 `/Annots`。`/AP` 句柄解析为 Form XObject
//! 流引用，与页面绘制及注解外观共享同一对象池。

use crate::core::builder::types::{
    AcroForm, Action, FieldActionEvent, FieldValue, FormField, FormFieldType, FormOption,
    text_string_value, validate_name_value,
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
    /// 全限定名（`/T` 链以 `.` 连接；无名 Widget 继承父名）。
    pub fqn: String,
    pub children: Vec<usize>,
}

/// 校验整棵字段树：页号、矩形、`/FT` 继承、字段类型专属条目、外观句柄、
/// `/AA` 触发事件与 `/CO` 计算顺序。
pub(super) fn validate(
    form: &AcroForm,
    page_count: usize,
    form_count: usize,
) -> Result<(), WriteError> {
    super::acro_form::validate_default_resources(form)?;
    let mut calculate = Vec::new();
    for (page, field) in form.fields() {
        validate_node(
            field,
            Some(*page),
            None,
            page_count,
            form_count,
            "",
            &mut calculate,
        )?;
    }
    validate_calculation_order(form, &calculate)
}

fn validate_node(
    field: &FormField,
    inherited_page: Option<usize>,
    inherited_type: Option<FormFieldType>,
    page_count: usize,
    form_count: usize,
    parent_fqn: &str,
    calculate: &mut Vec<String>,
) -> Result<(), WriteError> {
    let name = field.partial_name().unwrap_or("<widget>");
    let invalid = |message: String| WriteError::InvalidObject(message);
    let page = field.page().or(inherited_page);
    let field_type = field.field_type().or(inherited_type);
    let fqn = match field.partial_name() {
        Some(part) if !part.is_empty() => {
            if parent_fqn.is_empty() {
                part.to_string()
            } else {
                format!("{parent_fqn}.{part}")
            }
        }
        _ => parent_fqn.to_string(),
    };

    validate_actions(field)?;
    if field
        .actions()
        .iter()
        .any(|(event, _)| *event == FieldActionEvent::Calculate)
    {
        if field.partial_name().is_none_or(str::is_empty) {
            return Err(invalid(format!(
                "form field {name}: a /C calculation action requires a non-empty partial name (/T)"
            )));
        }
        calculate.push(fqn.clone());
    }

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
    validate_appearance_states(field)?;
    let handles = [
        field.appearance_normal(),
        field.appearance_down(),
        field.appearance_rollover(),
    ]
    .into_iter()
    .flatten()
    .chain(field.appearance_states().iter().map(|(_, handle)| *handle));
    for handle in handles {
        if handle >= form_count {
            return Err(invalid(format!(
                "form field {name}: /AP references unknown form handle {handle}"
            )));
        }
    }

    for child in field.kids() {
        validate_node(
            child, page, field_type, page_count, form_count, &fqn, calculate,
        )?;
    }
    Ok(())
}

/// `/AP /N` 状态子字典（Table 189）：仅 Widget、与单一流互斥、状态名合法
/// 且唯一；`/AS` 在有状态字典时必须指向已登记状态。
fn validate_appearance_states(field: &FormField) -> Result<(), WriteError> {
    let invalid = |message: String| WriteError::InvalidObject(message);
    let name = field.partial_name().unwrap_or("<widget>");
    if field.appearance_states().is_empty() {
        return Ok(());
    }
    if field.rect().is_none() {
        return Err(invalid(format!(
            "form field {name}: appearance states require a widget rectangle (/Rect)"
        )));
    }
    if field.appearance_normal().is_some() {
        return Err(invalid(format!(
            "form field {name}: /AP /N cannot be both a stream and a state sub-dictionary"
        )));
    }
    for (index, (state, _)) in field.appearance_states().iter().enumerate() {
        validate_name_value(state, "form field appearance state")
            .map_err(|message| invalid(format!("form field {name}: {message}")))?;
        if field.appearance_states()[..index]
            .iter()
            .any(|(existing, _)| existing == state)
        {
            return Err(invalid(format!(
                "form field {name}: appearance state {state} is registered twice"
            )));
        }
    }
    if let Some(state) = field.appearance_state()
        && !field
            .appearance_states()
            .iter()
            .any(|(name, _)| name == state)
    {
        return Err(invalid(format!(
            "form field {name}: /AS names unregistered appearance state {state}"
        )));
    }
    Ok(())
}

/// `/AA` 触发事件：每个事件至多一次，且 Table 196 限定为 JavaScript 动作。
fn validate_actions(field: &FormField) -> Result<(), WriteError> {
    let name = field.partial_name().unwrap_or("<widget>");
    let invalid = |message: String| WriteError::InvalidObject(message);
    let mut seen = std::collections::BTreeSet::new();
    for (event, action) in field.actions() {
        if !seen.insert(*event) {
            return Err(invalid(format!(
                "form field {name}: /AA /{} is declared twice",
                event.name()
            )));
        }
        if !matches!(action, Action::JavaScript(_)) {
            return Err(invalid(format!(
                "form field {name}: /AA /{} must be a JavaScript action",
                event.name()
            )));
        }
    }
    Ok(())
}

/// `/CO` 名录必须恰好覆盖全部带 `/C` 计算动作的字段（缺省时自动生成）。
fn validate_calculation_order(form: &AcroForm, calculate: &[String]) -> Result<(), WriteError> {
    let invalid = |message: String| WriteError::InvalidObject(message);
    let Some(order) = form.calculation_order() else {
        return Ok(());
    };
    if calculate.is_empty() {
        if order.is_empty() {
            return Ok(());
        }
        return Err(invalid(format!(
            "AcroForm /CO entry {} is not a field with a /C calculation action",
            order[0]
        )));
    }
    let expected: std::collections::BTreeSet<&str> = calculate.iter().map(String::as_str).collect();
    if expected.len() != calculate.len() {
        return Err(invalid(
            "AcroForm /CO requires unique fully qualified names for /C fields".into(),
        ));
    }
    let mut seen = std::collections::BTreeSet::new();
    for name in order {
        if !seen.insert(name.as_str()) {
            return Err(invalid(format!("AcroForm /CO lists field {name} twice")));
        }
        if !expected.contains(name.as_str()) {
            return Err(invalid(format!(
                "AcroForm /CO entry {name} is not a field with a /C calculation action"
            )));
        }
    }
    if seen.len() != expected.len() {
        return Err(invalid(
            "AcroForm /CO must list every field with a /C calculation action".into(),
        ));
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
        .flatten()
        .chain(field.appearance_states().iter().map(|(_, handle)| *handle)),
    );
    for child in field.kids() {
        collect_appearances(child, handles);
    }
}

/// 是否存在字段级 `/AA` 附加动作（供 assemble 做 PDF 1.3 门禁）。
pub(super) fn has_additional_actions(form: &AcroForm) -> bool {
    form.fields()
        .iter()
        .any(|(_, field)| node_has_actions(field))
}

fn node_has_actions(field: &FormField) -> bool {
    !field.actions().is_empty() || field.kids().iter().any(node_has_actions)
}

/// 是否存在 `/C` 计算动作（决定是否必须写出 `/CO`）。
pub(super) fn has_calculate_actions(form: &AcroForm) -> bool {
    form.fields()
        .iter()
        .any(|(_, field)| node_has_calculate(field))
}

fn node_has_calculate(field: &FormField) -> bool {
    field
        .actions()
        .iter()
        .any(|(event, _)| *event == FieldActionEvent::Calculate)
        || field.kids().iter().any(node_has_calculate)
}

/// 按前序为整棵字段树分配对象号；返回扁平节点表与顶层根下标。
pub(super) fn allocate_fields<'a>(
    form: &'a AcroForm,
    num: &mut i64,
) -> (Vec<FieldSlot<'a>>, Vec<usize>) {
    let mut slots = Vec::new();
    let mut roots = Vec::new();
    for (page, field) in form.fields() {
        roots.push(allocate_node(field, Some(*page), None, num, &mut slots, ""));
    }
    (slots, roots)
}

fn allocate_node<'a>(
    field: &'a FormField,
    inherited_page: Option<usize>,
    parent_num: Option<i64>,
    num: &mut i64,
    slots: &mut Vec<FieldSlot<'a>>,
    parent_fqn: &str,
) -> usize {
    let index = slots.len();
    let field_num = *num;
    *num += 1;
    let fqn = match field.partial_name() {
        Some(part) if !part.is_empty() => {
            if parent_fqn.is_empty() {
                part.to_string()
            } else {
                format!("{parent_fqn}.{part}")
            }
        }
        _ => parent_fqn.to_string(),
    };
    slots.push(FieldSlot {
        field,
        num: field_num,
        parent_num,
        page: field.page().or(inherited_page),
        fqn: fqn.clone(),
        children: Vec::new(),
    });
    let page = slots[index].page;
    let mut children = Vec::with_capacity(field.kids().len());
    for child in field.kids() {
        children.push(allocate_node(
            child,
            page,
            Some(field_num),
            num,
            slots,
            &fqn,
        ));
    }
    slots[index].children = children;
    index
}

/// `/CO` 引用的字段对象号：显式名录按调用方顺序解析，缺省按字段树前序
/// 取全部计算字段。
pub(super) fn calculation_order_nums(
    form: &AcroForm,
    slots: &[FieldSlot<'_>],
) -> Result<Vec<i64>, WriteError> {
    match form.calculation_order() {
        Some(order) => order
            .iter()
            .map(|name| {
                slots
                    .iter()
                    .find(|slot| &slot.fqn == name)
                    .map(|slot| slot.num)
                    .ok_or_else(|| {
                        WriteError::InvalidObject(format!(
                            "AcroForm /CO references unknown field {name}"
                        ))
                    })
            })
            .collect(),
        None => Ok(slots
            .iter()
            .filter(|slot| {
                slot.field
                    .actions()
                    .iter()
                    .any(|(event, _)| *event == FieldActionEvent::Calculate)
            })
            .map(|slot| slot.num)
            .collect()),
    }
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
    if let Some(tooltip) = field.tooltip() {
        dict = dict.insert("TU", text_string_value(tooltip));
    }
    if !field.actions().is_empty() {
        let mut additional = Dict::new();
        for (event, action) in field.actions() {
            additional = additional.insert(event.name(), Value::Dict(action.to_dict()));
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
