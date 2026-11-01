//! 属性对象校验与流提升（14.7.5 Table 327–329）：`/ClassMap` 值、
//! 元素 `/A` 属性对象与用户属性 `/P` 条目的形态校验，以及属性流
//! 提升为间接对象。

use super::limits::validate_name;
use crate::core::builder::navigation::invalid;
use crate::core::builder::navigation::structure_standard::{self, StandardUsage};
use crate::core::reader::structure::MAX_STRUCTURE_CHILDREN;
use crate::core::types::{Dict, Document, Object, Value};
use crate::core::writer::WriteError;

/// `/ClassMap` 的单个值：属性对象或属性对象数组（14.7.5.2）。
///
/// 类属性可能被不同类型的元素复用，标准条目只做值形态检查，不做元素
/// 适用性检查（适用性只对元素 `/A`）。
pub(super) fn validate_class_attributes(
    value: &Value,
    key: &str,
    uses_user_properties: &mut bool,
    standard: &mut StandardUsage,
) -> Result<(), WriteError> {
    let context = format!("structure class map /{key}");
    match value {
        Value::Array(items) => {
            if items.0.len() > MAX_STRUCTURE_CHILDREN {
                return Err(invalid(format!(
                    "{context} has {} attribute objects (limit {MAX_STRUCTURE_CHILDREN})",
                    items.0.len()
                )));
            }
            for item in &items.0 {
                validate_attribute_object(item, &context, uses_user_properties)?;
                structure_standard::validate_class_attribute(item, &context, standard)?;
            }
        }
        single => {
            validate_attribute_object(single, &context, uses_user_properties)?;
            structure_standard::validate_class_attribute(single, &context, standard)?;
        }
    }
    Ok(())
}

/// 单个属性对象（Table 327）：直接字典或流，`/O` 为合法非空名字。
pub(super) fn validate_attribute_object(
    value: &Value,
    context: &str,
    uses_user_properties: &mut bool,
) -> Result<(), WriteError> {
    let dict = match value {
        Value::Dict(dict) | Value::Stream(dict, _) => dict,
        other => {
            return Err(invalid(format!(
                "{context} must be an attribute dictionary or stream ({})",
                other.type_name()
            )));
        }
    };
    let owner = match dict.get("O") {
        Some(Value::Name(name)) => name,
        Some(other) => {
            return Err(invalid(format!(
                "{context} /O must be a name ({})",
                other.type_name()
            )));
        }
        None => return Err(invalid(format!("{context} /O is required (Table 327)"))),
    };
    validate_name(owner, &format!("{context} /O"))?;
    if owner == "UserProperties" {
        validate_user_properties(dict, context)?;
        *uses_user_properties = true;
    }
    Ok(())
}

/// 用户属性属性对象（14.7.5.3 Table 328/329）：`/P` 为属性字典数组。
fn validate_user_properties(dict: &Dict, context: &str) -> Result<(), WriteError> {
    let properties = match dict.get("P") {
        Some(Value::Array(items)) => items,
        Some(other) => {
            return Err(invalid(format!(
                "{context} /P must be an array of user property dictionaries ({})",
                other.type_name()
            )));
        }
        None => return Err(invalid(format!("{context} /P is required (Table 328)"))),
    };
    if properties.0.len() > MAX_STRUCTURE_CHILDREN {
        return Err(invalid(format!(
            "{context} /P has {} entries (limit {MAX_STRUCTURE_CHILDREN})",
            properties.0.len()
        )));
    }
    for (index, item) in properties.0.iter().enumerate() {
        let Value::Dict(property) = item else {
            return Err(invalid(format!(
                "{context} /P entry {index} must be a user property dictionary ({})",
                item.type_name()
            )));
        };
        match property.get("N") {
            Some(Value::String(_) | Value::HexString(_)) => {}
            Some(other) => {
                return Err(invalid(format!(
                    "{context} /P entry {index} /N must be a text string ({})",
                    other.type_name()
                )));
            }
            None => {
                return Err(invalid(format!(
                    "{context} /P entry {index} /N is required (Table 329)"
                )));
            }
        }
        if !property.0.contains_key("V") {
            return Err(invalid(format!(
                "{context} /P entry {index} /V is required (Table 329)"
            )));
        }
        if let Some(formatted) = property.get("F")
            && !matches!(formatted, Value::String(_) | Value::HexString(_))
        {
            return Err(invalid(format!(
                "{context} /P entry {index} /F must be a text string ({})",
                formatted.type_name()
            )));
        }
        if let Some(hidden) = property.get("H")
            && !matches!(hidden, Value::Bool(_))
        {
            return Err(invalid(format!(
                "{context} /P entry {index} /H must be a boolean ({})",
                hidden.type_name()
            )));
        }
    }
    Ok(())
}

/// 把属性值中的流提升为间接对象并写回引用；数组只处理直接元素
/// （ClassMap 数组项本身就是属性对象）。校验已保证数组项为字典/流。
pub(super) fn promote_attribute_streams(doc: &mut Document, value: &mut Value, next_num: &mut i64) {
    match value {
        Value::Stream(..) => {
            let num = *next_num;
            *next_num += 1;
            doc.set_object(Object::new(num, 0, value.clone()));
            *value = Value::Ref(num, 0);
        }
        Value::Array(items) => {
            for item in &mut items.0 {
                if matches!(item, Value::Stream(..)) {
                    let num = *next_num;
                    *next_num += 1;
                    doc.set_object(Object::new(num, 0, item.clone()));
                    *item = Value::Ref(num, 0);
                }
            }
        }
        _ => {}
    }
}
