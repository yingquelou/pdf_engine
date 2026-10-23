//! functions 模块测试（按函数类型分文件）。
//!
//! - `type0`：采样函数（打包 / 插值 / 负向）；
//! - `type2`：指数插值；
//! - `type3`：拼接（区间 / 嵌套限深）；
//! - `type4`：PostScript 计算器；
//! - 本文件：公共构造辅助与构造入口（公共路径）测试。

mod type0;
mod type2;
mod type3;
mod type4;

use crate::core::functions::Function;
use crate::core::types::{Array, Dict, Value};

/// 便捷构造字典。
pub(super) fn d(pairs: &[(&str, Value)]) -> Dict {
    let mut dict = Dict::new();
    for (k, v) in pairs {
        dict.0.insert(k.to_string(), v.clone());
    }
    dict
}

/// f64 数组 → `Value::Array`（整数值存 Integer，便于与解析产物一致）。
pub(super) fn nums(vals: &[f64]) -> Value {
    Value::Array(Array(
        vals.iter()
            .map(|&x| {
                if x.fract() == 0.0 && x.abs() < 1e15 {
                    Value::Integer(x as i64)
                } else {
                    Value::Float(x)
                }
            })
            .collect(),
    ))
}

pub(super) fn num(v: i64) -> Value {
    Value::Integer(v)
}

/// 便于测试的 Type 2 子函数字典（y = C0 + (C1−C0)·x，N=1）。
pub(super) fn exp_fn(c0: f64, c1: f64) -> Value {
    Value::Dict(d(&[
        ("FunctionType", num(2)),
        ("Domain", nums(&[0.0, 1.0])),
        ("C0", nums(&[c0])),
        ("C1", nums(&[c1])),
        ("N", num(1)),
    ]))
}

// ---------- 公共路径 ----------

#[test]
fn missing_function_type_errors() {
    let e = Function::from_dict(&d(&[("Domain", nums(&[0.0, 1.0]))]), None).unwrap_err();
    assert!(e.0.contains("/FunctionType"));
}

#[test]
fn unsupported_function_type_errors() {
    let e = Function::from_dict(
        &d(&[("FunctionType", num(1)), ("Domain", nums(&[0.0, 1.0]))]),
        None,
    )
    .unwrap_err();
    assert!(e.0.contains("unsupported /FunctionType 1"));
}

#[test]
fn non_function_object_errors() {
    let e = Function::from_value(&Value::Integer(3), None).unwrap_err();
    assert!(e.0.contains("function object"));
}
