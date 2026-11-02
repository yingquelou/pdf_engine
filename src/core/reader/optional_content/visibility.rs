//! Optional content membership (OCMD) visibility evaluation: the `/P` policy
//! matrix and the `/VE` visibility expression (PDF 8.11.2.2, Table 100).

use crate::core::types::{Dict, Value};

/// Evaluate an optional content membership dictionary (Table 100).
///
/// `/VE` takes precedence over `/P`; without either, `/P` defaults to
/// `/AllOn`. Returns `None` when the dictionary carries no `/OCGs` array, the
/// policy is unknown, or the state of a referenced group cannot be
/// determined (for example an `/Unchanged` base state).
pub fn membership_visibility(
    dict: &Dict,
    is_visible: &dyn Fn(i64) -> Option<bool>,
) -> Option<bool> {
    if let Some(expression) = dict.get("VE") {
        return visibility_expression(expression, is_visible);
    }
    let groups = dict.get("OCGs")?.as_array()?;
    let policy = dict.get("P").and_then(Value::as_name).unwrap_or("AllOn");
    let mut any_visible = false;
    let mut any_hidden = false;
    for group in groups.iter() {
        match group.as_ref().and_then(|(object, _)| is_visible(object)) {
            Some(true) => any_visible = true,
            Some(false) => any_hidden = true,
            None => return None,
        }
    }
    match policy {
        "AllOn" => Some(!any_hidden),
        "AnyOn" => Some(any_visible),
        "AnyOff" => Some(any_hidden),
        "AllOff" => Some(!any_visible),
        _ => None,
    }
}

/// Evaluate a `/VE` visibility expression: `[/And …]`, `[/Or …]`,
/// `[/Not expr]`, or an OCG reference.
fn visibility_expression(value: &Value, is_visible: &dyn Fn(i64) -> Option<bool>) -> Option<bool> {
    match value {
        Value::Ref(object, _) => is_visible(*object),
        Value::Array(items) => {
            let mut operands = items.iter();
            let operator = operands.next()?.as_name()?;
            match operator {
                "And" | "Or" => {
                    let dominates = operator == "Or";
                    let mut state = Some(!dominates);
                    for item in operands {
                        state = combine(state, visibility_expression(item, is_visible), dominates);
                    }
                    state
                }
                "Not" => Some(!visibility_expression(operands.next()?, is_visible)?),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Fold one visibility operand into a short-circuiting `And`/`Or` result.
///
/// `dominates` is the operator's absorbing value: `false` for `And`, `true`
/// for `Or`. An unknown operand only survives when no absorbing value was
/// seen.
fn combine(state: Option<bool>, operand: Option<bool>, dominates: bool) -> Option<bool> {
    match (state, operand) {
        (Some(value), _) if value == dominates => Some(dominates),
        (_, Some(value)) if value == dominates => Some(dominates),
        (Some(true), Some(true)) | (Some(false), Some(false)) => state,
        _ => None,
    }
}
