//! 前序对象号分配与 `/CO` 引用解析。

use crate::core::builder::types::{AcroForm, FieldActionEvent, FormField};
use crate::core::writer::WriteError;

use super::FieldSlot;

/// 按前序为整棵字段树分配对象号；返回扁平节点表与顶层根下标。
pub(crate) fn allocate_fields<'a>(
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
pub(crate) fn calculation_order_nums(
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
