//! Type 2 指数插值函数测试。

use super::{d, num, nums};
use crate::core::functions::Function;

#[test]
fn type2_basic_exponential() {
    // y = x^2，定义域/值域 [0,1]
    let f = Function::from_dict(
        &d(&[
            ("FunctionType", num(2)),
            ("Domain", nums(&[0.0, 1.0])),
            ("Range", nums(&[0.0, 1.0])),
            ("C0", nums(&[0.0])),
            ("C1", nums(&[1.0])),
            ("N", num(2)),
        ]),
        None,
    )
    .expect("construct");
    assert_eq!(f.function_type(), 2);
    assert_eq!(f.eval(&[0.0]).unwrap(), vec![0.0]);
    assert_eq!(f.eval(&[1.0]).unwrap(), vec![1.0]);
    assert!((f.eval(&[0.5]).unwrap()[0] - 0.25).abs() < 1e-12);
}

#[test]
fn type2_multi_output_and_defaults() {
    // 缺省 C0=[0]、C1=[1]；两路输出由数组长度给出
    let f = Function::from_dict(
        &d(&[
            ("FunctionType", num(2)),
            ("Domain", nums(&[-1.0, 2.0])),
            ("C0", nums(&[0.0, 10.0])),
            ("C1", nums(&[2.0, 12.0])),
            ("N", num(1)),
        ]),
        None,
    )
    .unwrap();
    assert_eq!(f.eval(&[0.5]).unwrap(), vec![1.0, 11.0]);
}

#[test]
fn type2_input_clamped_to_domain() {
    // 输入超出定义域按边界截断：x=2 → 按 1 求值
    let f = Function::from_dict(
        &d(&[
            ("FunctionType", num(2)),
            ("Domain", nums(&[0.0, 1.0])),
            ("N", num(1)),
        ]),
        None,
    )
    .unwrap();
    assert_eq!(f.eval(&[2.0]).unwrap(), vec![1.0]);
    assert_eq!(f.eval(&[-3.0]).unwrap(), vec![0.0]);
}

#[test]
fn type2_negative_base_integer_exponent() {
    // 负底数 + 整数指数：(-2)^2 = 4，值域截断
    let f = Function::from_dict(
        &d(&[
            ("FunctionType", num(2)),
            ("Domain", nums(&[-2.0, 2.0])),
            ("Range", nums(&[0.0, 3.0])),
            ("N", num(2)),
        ]),
        None,
    )
    .unwrap();
    assert_eq!(f.eval(&[-2.0]).unwrap(), vec![3.0]); // 4 → 截断到 3
}

#[test]
fn type2_missing_n_errors() {
    let e = Function::from_dict(
        &d(&[("FunctionType", num(2)), ("Domain", nums(&[0.0, 1.0]))]),
        None,
    )
    .unwrap_err();
    assert!(e.0.contains("/N"));
}

#[test]
fn type2_rejects_multi_dim_domain() {
    let e = Function::from_dict(
        &d(&[
            ("FunctionType", num(2)),
            ("Domain", nums(&[0.0, 1.0, 0.0, 1.0])),
            ("N", num(1)),
        ]),
        None,
    )
    .unwrap_err();
    assert!(e.0.contains("1 input"));
}
