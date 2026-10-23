//! Type 3 拼接函数测试。

use super::{d, exp_fn, num, nums};
use crate::core::functions::Function;
use crate::core::types::{Array, Value};

#[test]
fn type3_stitches_two_subfunctions() {
    // /Encode 必填：两段区间各自线性映射到子函数定义域。
    // Encode [0,0.5, 0.5,1] 为恒等映射 → 整体 y = x（第二段斜率 2）。
    let f = Function::from_dict(
        &d(&[
            ("FunctionType", num(3)),
            ("Domain", nums(&[0.0, 1.0])),
            (
                "Functions",
                Value::Array(Array(vec![exp_fn(0.0, 1.0), exp_fn(0.0, 2.0)])),
            ),
            ("Bounds", nums(&[0.5])),
            ("Encode", nums(&[0.0, 0.5, 0.5, 1.0])),
        ]),
        None,
    )
    .expect("construct");
    assert_eq!(f.function_type(), 3);
    assert!((f.eval(&[0.25]).unwrap()[0] - 0.25).abs() < 1e-12);
    assert!((f.eval(&[0.5]).unwrap()[0] - 1.0).abs() < 1e-12);
    assert!((f.eval(&[0.75]).unwrap()[0] - 1.5).abs() < 1e-12);
}

#[test]
fn type3_interval_half_open() {
    // 区间左闭右开：x=0.5 落在第二段
    let f = Function::from_dict(
        &d(&[
            ("FunctionType", num(3)),
            ("Domain", nums(&[0.0, 1.0])),
            (
                "Functions",
                Value::Array(Array(vec![exp_fn(0.0, 1.0), exp_fn(10.0, 11.0)])),
            ),
            ("Bounds", nums(&[0.5])),
            ("Encode", nums(&[0.0, 1.0, 0.0, 1.0])),
        ]),
        None,
    )
    .unwrap();
    // x=0.5⁻ 属第一段（趋近 1.0）；x=0.5 属第二段，e=0 → C0=10
    let at = f.eval(&[0.5]).unwrap()[0];
    assert!((at - 10.0).abs() < 1e-12);
    let just_below = f.eval(&[0.4999999999]).unwrap()[0];
    assert!(just_below < 2.0);
}

#[test]
fn type3_explicit_encode() {
    // 第二段把 [0.5, 1] 映射到子函数输入 [0, 0.25]
    let f = Function::from_dict(
        &d(&[
            ("FunctionType", num(3)),
            ("Domain", nums(&[0.0, 1.0])),
            (
                "Functions",
                Value::Array(Array(vec![exp_fn(0.0, 1.0), exp_fn(0.0, 1.0)])),
            ),
            ("Bounds", nums(&[0.5])),
            ("Encode", nums(&[0.0, 1.0, 0.0, 0.25])),
        ]),
        None,
    )
    .unwrap();
    // x=0.75 → 子函数输入 0.125 → y = 0.125
    assert!((f.eval(&[0.75]).unwrap()[0] - 0.125).abs() < 1e-12);
}

#[test]
fn type3_spec_example_inverted_encode() {
    // 规范 7.10.4 示例：f 定义域 [0 1]，g 定义域 [0 1]、单子函数 f、
    // Encode [1.0 0.0] → g(x) = f(1−x)
    let f = Function::from_dict(
        &d(&[
            ("FunctionType", num(3)),
            ("Domain", nums(&[0.0, 1.0])),
            ("Functions", Value::Array(Array(vec![exp_fn(0.0, 1.0)]))),
            ("Encode", nums(&[1.0, 0.0])),
        ]),
        None,
    )
    .unwrap();
    // f 为恒等：g(0.25) = f(0.75) = 0.75
    assert!((f.eval(&[0.25]).unwrap()[0] - 0.75).abs() < 1e-12);
    assert!((f.eval(&[1.0]).unwrap()[0] - 0.0).abs() < 1e-12);
}

#[test]
fn type3_parent_range_clips_output() {
    let f = Function::from_dict(
        &d(&[
            ("FunctionType", num(3)),
            ("Domain", nums(&[0.0, 1.0])),
            ("Range", nums(&[0.0, 0.5])),
            ("Functions", Value::Array(Array(vec![exp_fn(0.0, 2.0)]))),
        ]),
        None,
    )
    .unwrap();
    assert_eq!(f.eval(&[1.0]).unwrap(), vec![0.5]);
}

#[test]
fn type3_bounds_count_mismatch_errors() {
    let e = Function::from_dict(
        &d(&[
            ("FunctionType", num(3)),
            ("Domain", nums(&[0.0, 1.0])),
            (
                "Functions",
                Value::Array(Array(vec![exp_fn(0.0, 1.0), exp_fn(0.0, 1.0)])),
            ),
            ("Bounds", nums(&[0.3, 0.6])),
        ]),
        None,
    )
    .unwrap_err();
    assert!(e.0.contains("/Bounds"));
}

#[test]
fn type3_bounds_must_be_increasing() {
    let e = Function::from_dict(
        &d(&[
            ("FunctionType", num(3)),
            ("Domain", nums(&[0.0, 1.0])),
            (
                "Functions",
                Value::Array(Array(vec![
                    exp_fn(0.0, 1.0),
                    exp_fn(0.0, 1.0),
                    exp_fn(0.0, 1.0),
                ])),
            ),
            ("Bounds", nums(&[0.6, 0.3])),
        ]),
        None,
    )
    .unwrap_err();
    assert!(e.0.contains("strictly increasing"));
}

#[test]
fn type3_nesting_depth_guard() {
    // 递归嵌套超过 64 层报错（而非栈溢出）
    let mut inner = exp_fn(0.0, 1.0);
    for _ in 0..70 {
        inner = Value::Dict(d(&[
            ("FunctionType", num(3)),
            ("Domain", nums(&[0.0, 1.0])),
            ("Functions", Value::Array(Array(vec![inner]))),
        ]));
    }
    let e = Function::from_value(&inner, None).unwrap_err();
    assert!(e.0.contains("nesting"));
}
