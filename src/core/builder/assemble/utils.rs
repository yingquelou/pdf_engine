//! `assemble` 阶段共享的小工具：实数数组、十六进制串、函数对象写出与组属性字典。

use std::fmt::Write as _;

use crate::core::builder::{Function, GroupXObject};
use crate::core::types::{Array, Dict, Document, Object, StreamData, Value};
use crate::core::writer::WriteError;

use super::colorspace::color_space_value;

pub(crate) fn real_array(values: &[f64]) -> Value {
    Value::Array(Array(values.iter().map(|v| Value::Float(*v)).collect()))
}

/// 把函数写成对象/字典并返回可嵌入的值。
///
/// Type 0/4 必须为流：分配对象号、补 `/Length` 后写出，返回间接引用；
/// Type 2/3 返回直接字典，其中 Type 3 的子函数递归处理。
pub(crate) fn write_function(
    doc: &mut Document,
    function: &Function,
    num: &mut i64,
) -> Result<Value, WriteError> {
    let mut nested = |nested_function: &Function| -> Result<Value, String> {
        write_function(doc, nested_function, num).map_err(|error| error.to_string())
    };
    let object = function
        .to_object(&mut nested)
        .map_err(|message| WriteError::InvalidObject(format!("function: {message}")))?;
    match object.stream {
        Some(data) => {
            let object_num = *num;
            *num += 1;
            let dict = object
                .dict
                .insert("Length", Value::Integer(data.len() as i64));
            doc.set_object(Object::new(
                object_num,
                0,
                Value::Stream(dict, StreamData::Owned(data)),
            ));
            Ok(Value::Ref(object_num, 0))
        }
        None => Ok(Value::Dict(object.dict)),
    }
}

/// 组属性字典（Table 96）：Form XObject 与 Page 的 `/Group` 共用。
pub(super) fn group_dict(
    group: &GroupXObject,
    doc: &mut Document,
    num: &mut i64,
) -> Result<Dict, WriteError> {
    let mut dict = Dict::new()
        .insert("Type", Value::Name("Group".into()))
        .insert("S", Value::Name("Transparency".into()));
    if let Some(color_space) = &group.color_space {
        dict = dict.insert("CS", color_space_value(color_space, doc, num)?);
    }
    if group.isolated {
        dict = dict.insert("I", Value::Bool(true));
    }
    if group.knockout {
        dict = dict.insert("K", Value::Bool(true));
    }
    Ok(dict)
}

pub(crate) fn hex_bytes(bytes: &[u8]) -> String {
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(hex, "{byte:02X}");
    }
    hex
}
