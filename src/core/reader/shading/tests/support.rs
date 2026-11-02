//! Shading/Pattern 读端测试的跨主题共享夹具：文档与 PDF 对象值构造、
//! 指数/双输入计算函数样本、轴向渐变字典与单页资源文档。仅服务单一
//! 主题的夹具（`make_page_doc`）随主题留在对应子文件。

use crate::core::types::{Array, Dict, Document, Object, StreamData, Value};

pub(super) fn doc_with_objects(objects: Vec<(i64, Value)>) -> Document {
    let doc = Document::new();
    for (num, value) in objects {
        doc.set_object(Object::new(num, 0, value));
    }
    doc
}

pub(super) fn name(text: &str) -> Value {
    Value::Name(text.into())
}

pub(super) fn array(items: Vec<Value>) -> Value {
    Value::Array(Array(items))
}

pub(super) fn reals(values: &[f64]) -> Value {
    Value::Array(Array(
        values.iter().map(|v| Value::Float(*v)).collect::<Vec<_>>(),
    ))
}

pub(super) fn dict(items: &[(&str, Value)]) -> Value {
    let mut dict = Dict::new();
    for (key, value) in items {
        dict = dict.insert(*key, value.clone());
    }
    Value::Dict(dict)
}

pub(super) fn exponential(domain: &[f64], c0: &[f64], c1: &[f64]) -> Value {
    dict(&[
        ("FunctionType", Value::Integer(2)),
        ("Domain", reals(domain)),
        ("N", Value::Integer(1)),
        ("C0", reals(c0)),
        ("C1", reals(c1)),
    ])
}

pub(super) fn two_input_calculator() -> Value {
    Value::Stream(
        Dict::new()
            .insert("FunctionType", Value::Integer(4))
            .insert("Domain", reals(&[0.0, 1.0, 0.0, 1.0]))
            .insert("Range", reals(&[0.0, 1.0, 0.0, 1.0, 0.0, 1.0])),
        StreamData::Owned(b"0.5 0.5 0.5".to_vec()),
    )
}

pub(super) fn axial_dict(extra: &[(&str, Value)]) -> Value {
    let mut items = vec![
        ("ShadingType", Value::Integer(2)),
        ("ColorSpace", name("DeviceRGB")),
        ("Coords", reals(&[0.0, 0.0, 1.0, 1.0])),
        (
            "Function",
            exponential(&[0.0, 1.0], &[0.0, 0.0, 0.0], &[1.0, 1.0, 1.0]),
        ),
    ];
    items.extend(extra.iter().map(|(k, v)| (*k, v.clone())));
    dict(&items)
}
