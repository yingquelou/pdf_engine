//! Indirect-reference resolution helpers shared by the extractor.

use crate::core::types::{Document, Value};

use super::matrix::Matrix;

pub(super) fn resolve_value(document: &Document, value: &Value) -> Option<Value> {
    match value {
        // 按对象号取最新修订，忽略引用中的代号（增量更新后引用可能写旧代）。
        Value::Ref(num, _) => document.get_object(*num).ok().flatten().map(|o| o.value),
        _ => Some(value.clone()),
    }
}

/// Read a form XObject's `/Matrix`, falling back to the identity matrix.
pub(super) fn form_matrix(document: &Document, value: Option<&Value>) -> Matrix {
    let Some(value) = value.and_then(|value| resolve_value(document, value)) else {
        return Matrix::IDENTITY;
    };
    let Some(array) = value.as_array() else {
        return Matrix::IDENTITY;
    };
    if array.len() != 6 {
        return Matrix::IDENTITY;
    }
    let mut values = [0.0; 6];
    for (slot, item) in values.iter_mut().zip(array.iter()) {
        let Some(item) = resolve_value(document, item) else {
            return Matrix::IDENTITY;
        };
        *slot = match item {
            Value::Integer(number) => number as f64,
            Value::Float(number) => number,
            _ => return Matrix::IDENTITY,
        };
    }
    Matrix::from_values(values)
}
