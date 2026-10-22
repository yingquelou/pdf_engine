//! Type 4 PostScript 计算器函数测试。

use super::{d, num, nums};
use crate::pdf::functions::Function;

/// 构造 m 入 n 出的计算器函数（宽定义域 / 大值域便于断言原始结果）。
fn calc(code: &[u8], m: usize, n: usize) -> Function {
    let mut domain = Vec::new();
    for _ in 0..m {
        domain.extend_from_slice(&[-100.0, 100.0]);
    }
    let mut range = Vec::new();
    for _ in 0..n {
        range.extend_from_slice(&[-1.0e6, 1.0e6]);
    }
    Function::from_dict(
        &d(&[
            ("FunctionType", num(4)),
            ("Domain", nums(&domain)),
            ("Range", nums(&range)),
        ]),
        Some(code),
    )
    .expect("construct")
}

fn eval1(f: &Function, input: &[f64]) -> Vec<f64> {
    f.eval(input).unwrap()
}

#[test]
fn type4_arithmetic() {
    assert_eq!(eval1(&calc(b"add", 2, 1), &[1.0, 2.0]), vec![3.0]);
    assert_eq!(eval1(&calc(b"sub", 2, 1), &[5.0, 3.0]), vec![2.0]);
    assert_eq!(eval1(&calc(b"mul", 2, 1), &[3.0, 4.0]), vec![12.0]);
    assert_eq!(eval1(&calc(b"div", 2, 1), &[1.0, 4.0]), vec![0.25]);
    assert_eq!(eval1(&calc(b"idiv", 2, 1), &[7.0, 2.0]), vec![3.0]);
    assert_eq!(eval1(&calc(b"mod", 2, 1), &[7.0, 3.0]), vec![1.0]);
    assert_eq!(eval1(&calc(b"neg", 1, 1), &[5.0]), vec![-5.0]);
    assert_eq!(eval1(&calc(b"abs", 1, 1), &[-3.0]), vec![3.0]);
    assert_eq!(eval1(&calc(b"ceiling", 1, 1), &[1.2]), vec![2.0]);
    assert_eq!(eval1(&calc(b"floor", 1, 1), &[1.8]), vec![1.0]);
    assert_eq!(eval1(&calc(b"round", 1, 1), &[2.5]), vec![3.0]);
    assert_eq!(eval1(&calc(b"truncate", 1, 1), &[-2.7]), vec![-2.0]);
    assert_eq!(eval1(&calc(b"sqrt", 1, 1), &[9.0]), vec![3.0]);
    assert_eq!(eval1(&calc(b"exp", 1, 1), &[0.0]), vec![1.0]);
    let f = calc(b"ln", 1, 1);
    assert!((eval1(&f, &[std::f64::consts::E])[0] - 1.0).abs() < 1e-12);
    assert_eq!(eval1(&calc(b"log", 1, 1), &[100.0]), vec![2.0]);
}

#[test]
fn type4_relational_and_boolean() {
    assert_eq!(eval1(&calc(b"gt", 2, 1), &[2.0, 1.0]), vec![1.0]);
    assert_eq!(eval1(&calc(b"gt", 2, 1), &[1.0, 2.0]), vec![0.0]);
    assert_eq!(eval1(&calc(b"eq", 2, 1), &[2.0, 2.0]), vec![1.0]);
    assert_eq!(eval1(&calc(b"ne", 2, 1), &[2.0, 2.0]), vec![0.0]);
    assert_eq!(eval1(&calc(b"le", 2, 1), &[2.0, 2.0]), vec![1.0]);
    assert_eq!(eval1(&calc(b"and", 2, 1), &[1.0, 1.0]), vec![1.0]);
    assert_eq!(eval1(&calc(b"and", 2, 1), &[1.0, 0.0]), vec![0.0]);
    assert_eq!(eval1(&calc(b"or", 2, 1), &[0.0, 1.0]), vec![1.0]);
    assert_eq!(eval1(&calc(b"xor", 2, 1), &[1.0, 1.0]), vec![0.0]);
    assert_eq!(eval1(&calc(b"not", 1, 1), &[0.0]), vec![1.0]);
    assert_eq!(eval1(&calc(b"not", 1, 1), &[1.0]), vec![0.0]);
}

#[test]
fn type4_stack_operations() {
    // dup：平方
    assert_eq!(eval1(&calc(b"dup mul", 1, 1), &[7.0]), vec![49.0]);
    // exch sub：b - a
    assert_eq!(eval1(&calc(b"exch sub", 2, 1), &[5.0, 3.0]), vec![-2.0]);
    // pop：丢弃第二个输入
    assert_eq!(eval1(&calc(b"pop", 2, 1), &[9.0, 4.0]), vec![9.0]);
    // copy：复制栈顶 2 个 → 栈 [3, 4, 3, 4] → add → [3, 4, 7]，取栈顶 2 个
    assert_eq!(
        eval1(&calc(b"2 copy add", 2, 2), &[3.0, 4.0]),
        vec![4.0, 7.0]
    );
    // index：0 = 栈顶
    assert_eq!(eval1(&calc(b"1 index add", 2, 1), &[10.0, 5.0]), vec![15.0]);
    // roll：a b c 3 1 roll → b c a
    assert_eq!(
        eval1(&calc(b"3 1 roll", 3, 3), &[1.0, 2.0, 3.0]),
        vec![2.0, 3.0, 1.0]
    );
    // roll：a b c 3 -1 roll → c a b
    assert_eq!(
        eval1(&calc(b"3 -1 roll", 3, 3), &[1.0, 2.0, 3.0]),
        vec![3.0, 1.0, 2.0]
    );
}

#[test]
fn type4_conditionals_and_constants() {
    // x < 0 → -1，否则 1（lt 消耗输入，分支值即输出）
    let f = calc(b"0 lt { -1 } { 1 } ifelse", 1, 1);
    assert_eq!(eval1(&f, &[-5.0]), vec![-1.0]);
    assert_eq!(eval1(&f, &[5.0]), vec![1.0]);
    // 条件分支保留输入：dup 后比较，分支内乘 100
    let f = calc(b"dup 0 gt { 100 mul } if", 1, 1);
    assert_eq!(eval1(&f, &[3.0]), vec![300.0]);
    assert_eq!(eval1(&f, &[0.0]), vec![0.0]);
    // true / false / pi（零输入语义用一维哑输入表达：输入压栈后被 pop）
    assert_eq!(eval1(&calc(b"pop true", 1, 1), &[0.0]), vec![1.0]);
    assert_eq!(eval1(&calc(b"pop false", 1, 1), &[0.0]), vec![0.0]);
    assert_eq!(
        eval1(&calc(b"pop pi", 1, 1), &[0.0]),
        vec![std::f64::consts::PI]
    );
}

#[test]
fn type4_numbers_with_signs_and_decimals() {
    assert_eq!(eval1(&calc(b"pop 2.5 1.5 add", 1, 1), &[0.0]), vec![4.0]);
    assert_eq!(eval1(&calc(b"pop -1.5 -2.5 add", 1, 1), &[0.0]), vec![-4.0]);
    assert_eq!(eval1(&calc(b"pop .5 2 mul", 1, 1), &[0.0]), vec![1.0]);
    assert_eq!(eval1(&calc(b"pop 1e2", 1, 1), &[0.0]), vec![100.0]);
}

#[test]
fn type4_output_clipped_to_range() {
    let f = Function::from_dict(
        &d(&[
            ("FunctionType", num(4)),
            ("Domain", nums(&[0.0, 100.0])),
            ("Range", nums(&[0.0, 10.0])),
        ]),
        Some(b"dup mul"),
    )
    .unwrap();
    assert_eq!(f.eval(&[4.0]).unwrap(), vec![10.0]); // 16 → 截断到 10
}

#[test]
fn type4_negative_inputs_pass_through() {
    // 输入原样压栈（已按定义域截断），负数可用
    let f = calc(b"abs", 1, 1);
    assert_eq!(eval1(&f, &[-7.0]), vec![7.0]);
}

#[test]
fn type4_errors() {
    // 除零
    let e = calc(b"div", 2, 1).eval(&[1.0, 0.0]).unwrap_err();
    assert!(e.0.contains("division by zero"));
    // 栈下溢
    let e = calc(b"pop pop", 1, 1).eval(&[1.0]).unwrap_err();
    assert!(e.0.contains("underflow"));
    // 未知操作符（构造期报错）
    let e = Function::from_dict(
        &d(&[
            ("FunctionType", num(4)),
            ("Domain", nums(&[0.0, 1.0])),
            ("Range", nums(&[0.0, 1.0])),
        ]),
        Some(b"forbidden"),
    )
    .unwrap_err();
    assert!(e.0.contains("unsupported operator"));
    // 花括号不平衡
    let e = Function::from_dict(
        &d(&[
            ("FunctionType", num(4)),
            ("Domain", nums(&[0.0, 1.0])),
            ("Range", nums(&[0.0, 1.0])),
        ]),
        Some(b"{ add"),
    )
    .unwrap_err();
    assert!(e.0.contains("'{'"));
    // 输出个数不足
    let e = calc(b"pop", 2, 2).eval(&[1.0, 2.0]).unwrap_err();
    assert!(e.0.contains("need 2"));
}
