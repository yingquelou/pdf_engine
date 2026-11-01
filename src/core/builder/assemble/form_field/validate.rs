//! 字段树校验：页号、矩形、`/FT` 继承、字段类型专属条目、外观句柄、
//! `/AA` 触发事件、同名字段多重表示、`/TU` 版本门禁与 `/CO` 计算顺序
//! （Table 220–231）。

use crate::core::builder::types::{
    AcroForm, Action, FieldActionEvent, FieldValue, FormField, FormFieldType, validate_name_value,
};
use crate::core::types::{PdfFeature, PdfVersion};
use crate::core::writer::WriteError;

/// 校验整棵字段树：页号、矩形、`/FT` 继承、字段类型专属条目、外观句柄、
/// `/AA` 触发事件、同名字段多重表示与 `/CO` 计算顺序。
pub(crate) fn validate(
    form: &AcroForm,
    page_count: usize,
    form_count: usize,
    version: PdfVersion,
) -> Result<(), WriteError> {
    super::super::acro_form::validate_default_resources(form)?;
    let mut calculate = Vec::new();
    for (page, field) in form.fields() {
        validate_node(
            field,
            Some(*page),
            None,
            page_count,
            form_count,
            version,
            "",
            &mut calculate,
        )?;
    }
    validate_shared_representations(form)?;
    validate_calculation_order(form, &calculate)?;
    // `/TU` 替代字段名自 PDF 1.3 引入（Table 220）：在分配对象号之前拒绝，
    // 保持零字节契约。
    form.validate_alternate_name_version(version)
}

#[allow(clippy::too_many_arguments)]
fn validate_node(
    field: &FormField,
    inherited_page: Option<usize>,
    inherited_type: Option<FormFieldType>,
    page_count: usize,
    form_count: usize,
    version: PdfVersion,
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
    if !field.options().is_empty() {
        match field_type {
            Some(FormFieldType::Choice) => {}
            Some(FormFieldType::Button) => {
                if !version.supports(PdfFeature::FormButtonOptions) {
                    return Err(WriteError::UnsupportedVersion(format!(
                        "{} require PDF {}, got {}",
                        PdfFeature::FormButtonOptions,
                        PdfFeature::FormButtonOptions.min_version().as_str(),
                        version.as_str()
                    )));
                }
                validate_button_options(field, &fqn)?;
            }
            _ => {
                return Err(invalid(format!(
                    "form field {name}: /Opt is only valid for choice and button fields"
                )));
            }
        }
    }
    validate_choice_indices(field, field_type, version, &fqn)?;
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
            child, page, field_type, page_count, form_count, version, &fqn, calculate,
        )?;
    }
    Ok(())
}

/// 最近具名祖先（即当前全限定名的所有者）声明的 `/V` 与 `/DV`。
struct FieldOwner<'a> {
    fqn: String,
    value: Option<&'a FieldValue>,
    default_value: Option<&'a FieldValue>,
}

/// 同一全限定名的多重表示（12.7.3.2）：无 `/T` 的节点复用父链的名字，
/// 因而是同一底层字段的表示，其显式声明的 `/V`、`/DV` 须与最近具名祖先一致
/// （缺省即继承，不构成冲突）；`/FT` 由继承得出，无需另查。
fn validate_shared_representations(form: &AcroForm) -> Result<(), WriteError> {
    for (_, field) in form.fields() {
        check_representations(field, "", None)?;
    }
    Ok(())
}

fn check_representations<'a>(
    field: &'a FormField,
    parent_fqn: &str,
    owner: Option<&FieldOwner<'a>>,
) -> Result<(), WriteError> {
    let part = field.partial_name().filter(|name| !name.is_empty());
    let fqn = match part {
        Some(name) if parent_fqn.is_empty() => name.to_string(),
        Some(name) => format!("{parent_fqn}.{name}"),
        None => parent_fqn.to_string(),
    };
    let named = match part {
        Some(_) => FieldOwner {
            fqn: fqn.clone(),
            value: field.value(),
            default_value: field.default_value(),
        },
        None => {
            if let Some(owner) = owner {
                for (entry, declared, inherited) in [
                    ("/V", field.value(), owner.value),
                    ("/DV", field.default_value(), owner.default_value),
                ] {
                    if declared.or(inherited) != inherited {
                        return Err(WriteError::InvalidObject(format!(
                            "form field {}: representations sharing a fully qualified name must declare the same {entry}",
                            owner.fqn
                        )));
                    }
                }
            }
            // 无 `/T` 的节点只能是 Widget 表示，规范不允许其再有子节点。
            return Ok(());
        }
    };
    for child in field.kids() {
        check_representations(child, &fqn, Some(&named))?;
    }
    Ok(())
}

/// 复选框与单选按钮字段的 `/Opt`（Table 227）：元素只能是单个文本串，
/// 条目数须等于 Widget 数；存在 `/Opt` 时各 Widget 的非 `Off` 外观
/// 状态名须为其在 `/Kids` 中的位置序号（`/0`、`/1` …）。
fn validate_button_options(field: &FormField, fqn: &str) -> Result<(), WriteError> {
    let invalid = |message: String| WriteError::InvalidObject(message);
    if field.flags() & FormField::PUSHBUTTON != 0 {
        return Err(invalid(format!(
            "form field {fqn}: /Opt is not valid for pushbutton fields"
        )));
    }
    for option in field.options() {
        if option.display().is_some() {
            return Err(invalid(format!(
                "form field {fqn}: button /Opt entries must be single text strings"
            )));
        }
    }
    let widget_count = field.kids().len().max(1);
    if field.options().len() != widget_count {
        return Err(invalid(format!(
            "form field {fqn}: /Opt has {} entries but the field has {widget_count} widget annotations",
            field.options().len()
        )));
    }
    for (index, kid) in field.kids().iter().enumerate() {
        if kid.rect().is_none() {
            return Err(invalid(format!(
                "form field {fqn}: /Opt requires every child to be a widget annotation"
            )));
        }
        let positional = index.to_string();
        let on_states: Vec<&str> = kid
            .appearance_states()
            .iter()
            .map(|(state, _)| state.as_str())
            .filter(|state| *state != "Off")
            .collect();
        if on_states != [positional.as_str()] {
            return Err(invalid(format!(
                "form field {fqn}: /Opt requires widget {index} on state /{positional}, found {on_states:?}"
            )));
        }
    }
    Ok(())
}

/// 选择字段的 `/TI` 与 `/I`（Table 231）：下标须落在同一节点的 `/Opt`
/// 之内；`/I` 另要求开启多选（Table 230 bit 22）、严格升序且 PDF 1.4 起。
fn validate_choice_indices(
    field: &FormField,
    field_type: Option<FormFieldType>,
    version: PdfVersion,
    fqn: &str,
) -> Result<(), WriteError> {
    let invalid = |message: String| WriteError::InvalidObject(message);
    if field.top_index().is_none() && field.alternate_indices().is_empty() {
        return Ok(());
    }
    if field_type != Some(FormFieldType::Choice) {
        return Err(invalid(format!(
            "form field {fqn}: /TI and /I are only valid for choice fields"
        )));
    }
    let option_count = field.options().len();
    if option_count == 0 {
        return Err(invalid(format!(
            "form field {fqn}: /TI and /I require a /Opt array"
        )));
    }
    if let Some(top_index) = field.top_index()
        && top_index as usize >= option_count
    {
        return Err(invalid(format!(
            "form field {fqn}: /TI {top_index} is outside the {option_count} entries in /Opt"
        )));
    }
    if field.alternate_indices().is_empty() {
        return Ok(());
    }
    if !version.supports(PdfFeature::FormChoiceIndices) {
        return Err(WriteError::UnsupportedVersion(format!(
            "{} require PDF {}, got {}",
            PdfFeature::FormChoiceIndices,
            PdfFeature::FormChoiceIndices.min_version().as_str(),
            version.as_str()
        )));
    }
    if field.flags() & FormField::MULTI_SELECT == 0 {
        return Err(invalid(format!(
            "form field {fqn}: /I requires the MultiSelect flag (Table 230)"
        )));
    }
    let mut previous: Option<u32> = None;
    for &index in field.alternate_indices() {
        if index as usize >= option_count {
            return Err(invalid(format!(
                "form field {fqn}: /I index {index} is outside the {option_count} entries in /Opt"
            )));
        }
        if let Some(previous) = previous
            && index <= previous
        {
            return Err(invalid(format!(
                "form field {fqn}: /I must list indices in strictly ascending order, {previous} then {index}"
            )));
        }
        previous = Some(index);
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
pub(crate) fn appearance_handles(form: &AcroForm) -> Vec<usize> {
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
pub(crate) fn has_additional_actions(form: &AcroForm) -> bool {
    form.fields()
        .iter()
        .any(|(_, field)| node_has_actions(field))
}

fn node_has_actions(field: &FormField) -> bool {
    !field.actions().is_empty() || field.kids().iter().any(node_has_actions)
}

/// 是否存在 `/C` 计算动作（决定是否必须写出 `/CO`）。
pub(crate) fn has_calculate_actions(form: &AcroForm) -> bool {
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
