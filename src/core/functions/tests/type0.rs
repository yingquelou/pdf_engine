//! Type 0 采样函数测试。

use super::{d, num, nums};
use crate::core::functions::Function;
use crate::core::types::{StreamData, Value};

/// 构造 8 位、单输出、size=[n] 的采样函数。
fn sampled_1d(values: &[u8], size: usize) -> Function {
    Function::from_dict(
        &d(&[
            ("FunctionType", num(0)),
            ("Domain", nums(&[0.0, 1.0])),
            ("Range", nums(&[0.0, 1.0])),
            ("Size", Value::Array(crate::core::types::Array(vec![Value::Integer(
                size as i64,
            )]))),
            ("BitsPerSample", num(8)),
        ]),
        Some(values),
    )
    .expect("construct")
}

#[test]
fn type0_sample_values_and_interpolation() {
    // 样本 [0, 128, 255] → 0, 128/255, 1
    let f = sampled_1d(&[0, 128, 255], 3);
    assert_eq!(f.function_type(), 0);
    assert_eq!(f.eval(&[0.0]).unwrap(), vec![0.0]);
    assert_eq!(f.eval(&[1.0]).unwrap(), vec![1.0]);
    let mid = f.eval(&[0.5]).unwrap()[0]; // 恰好落在下标 1
    assert!((mid - 128.0 / 255.0).abs() < 1e-12);
    let mid2 = f.eval(&[0.75]).unwrap()[0]; // 下标 1 与 2 的中点
    let expect = (128.0 / 255.0 + 1.0) / 2.0;
    assert!((mid2 - expect).abs() < 1e-12);
}

#[test]
fn type0_two_dimensional() {
    // 2×2 网格，8 位：样本维度主序（第一维最慢）
    // (0,0)=0 (0,1)=64 (1,0)=128 (1,1)=192，经 /Decode 默认 Range 映射
    let f = Function::from_dict(
        &d(&[
            ("FunctionType", num(0)),
            ("Domain", nums(&[0.0, 1.0, 0.0, 1.0])),
            ("Range", nums(&[0.0, 1.0])),
            ("Size", nums(&[2.0, 2.0])),
            ("BitsPerSample", num(8)),
        ]),
        Some(&[0, 64, 128, 192]),
    )
    .unwrap();
    // 角点
    assert_eq!(f.eval(&[0.0, 0.0]).unwrap(), vec![0.0]);
    assert_eq!(f.eval(&[1.0, 1.0]).unwrap(), vec![192.0 / 255.0]);
    // 中心点 = 四角均值
    let c = f.eval(&[0.5, 0.5]).unwrap()[0];
    assert!((c - (0.0 + 64.0 / 255.0 + 128.0 / 255.0 + 192.0 / 255.0) / 4.0).abs() < 1e-12);
}

#[test]
fn type0_multi_output() {
    // 每点 2 个输出：size=[2]，样本 [0, 255, 128, 64]
    let f = Function::from_dict(
        &d(&[
            ("FunctionType", num(0)),
            ("Domain", nums(&[0.0, 1.0])),
            ("Range", nums(&[0.0, 1.0, 0.0, 1.0])),
            ("Size", nums(&[2.0])),
            ("BitsPerSample", num(8)),
        ]),
        Some(&[0, 255, 128, 64]),
    )
    .unwrap();
    assert_eq!(f.eval(&[1.0]).unwrap(), vec![128.0 / 255.0, 64.0 / 255.0]);
}

#[test]
fn type0_small_bit_widths() {
    // 4 位样本：[0, 15] → 0, 1（一个字节装两个样本）
    let f = Function::from_dict(
        &d(&[
            ("FunctionType", num(0)),
            ("Domain", nums(&[0.0, 1.0])),
            ("Range", nums(&[0.0, 1.0])),
            ("Size", nums(&[2.0])),
            ("BitsPerSample", num(4)),
        ]),
        Some(&[0x0F]),
    )
    .unwrap();
    assert_eq!(f.eval(&[0.0]).unwrap(), vec![0.0]);
    assert_eq!(f.eval(&[1.0]).unwrap(), vec![1.0]);
}

#[test]
fn type0_explicit_decode() {
    // /Decode [1 0] 反转映射：样本 255 → 0，0 → 1
    let f = Function::from_dict(
        &d(&[
            ("FunctionType", num(0)),
            ("Domain", nums(&[0.0, 1.0])),
            ("Range", nums(&[0.0, 1.0])),
            ("Size", nums(&[2.0])),
            ("BitsPerSample", num(8)),
            ("Decode", nums(&[1.0, 0.0])),
        ]),
        Some(&[0, 255]),
    )
    .unwrap();
    assert_eq!(f.eval(&[0.0]).unwrap(), vec![1.0]);
    assert_eq!(f.eval(&[1.0]).unwrap(), vec![0.0]);
}

#[test]
fn type0_truncated_samples_error() {
    let e = Function::from_dict(
        &d(&[
            ("FunctionType", num(0)),
            ("Domain", nums(&[0.0, 1.0])),
            ("Range", nums(&[0.0, 1.0])),
            ("Size", nums(&[4.0])),
            ("BitsPerSample", num(8)),
        ]),
        Some(&[1, 2, 3]), // 需要 4 字节
    )
    .unwrap_err();
    assert!(e.0.contains("truncated"));
}

#[test]
fn type0_invalid_bits_error() {
    let e = Function::from_dict(
        &d(&[
            ("FunctionType", num(0)),
            ("Domain", nums(&[0.0, 1.0])),
            ("Range", nums(&[0.0, 1.0])),
            ("Size", nums(&[2.0])),
            ("BitsPerSample", num(5)),
        ]),
        Some(&[0, 255]),
    )
    .unwrap_err();
    assert!(e.0.contains("BitsPerSample"));
}

#[test]
fn type0_order3_unsupported() {
    let e = Function::from_dict(
        &d(&[
            ("FunctionType", num(0)),
            ("Domain", nums(&[0.0, 1.0])),
            ("Range", nums(&[0.0, 1.0])),
            ("Size", nums(&[2.0])),
            ("BitsPerSample", num(8)),
            ("Order", num(3)),
        ]),
        Some(&[0, 255]),
    )
    .unwrap_err();
    assert!(e.0.contains("Order"));
}

#[test]
fn type0_missing_stream_errors() {
    let e = Function::from_dict(
        &d(&[
            ("FunctionType", num(0)),
            ("Domain", nums(&[0.0, 1.0])),
            ("Range", nums(&[0.0, 1.0])),
            ("Size", nums(&[2.0])),
            ("BitsPerSample", num(8)),
        ]),
        None,
    )
    .unwrap_err();
    assert!(e.0.contains("stream"));
}

#[test]
fn type0_owned_stream_value() {
    // Value::Stream + StreamData::Owned 直接携带样本
    let v = Value::Stream(
        d(&[
            ("FunctionType", num(0)),
            ("Domain", nums(&[0.0, 1.0])),
            ("Range", nums(&[0.0, 1.0])),
            ("Size", nums(&[2.0])),
            ("BitsPerSample", num(8)),
        ]),
        StreamData::Owned(vec![0, 255]),
    );
    let f = Function::from_value(&v, None).unwrap();
    assert_eq!(f.eval(&[1.0]).unwrap(), vec![1.0]);
}

#[test]
fn stream_ref_value_rejected() {
    // Value::Stream + StreamData::Ref 无法在无 Document 上下文取字节
    let v = Value::Stream(
        d(&[
            ("FunctionType", num(4)),
            ("Domain", nums(&[0.0, 1.0])),
            ("Range", nums(&[0.0, 1.0])),
        ]),
        StreamData::Ref { offset: 10, len: 5 },
    );
    let e = Function::from_value(&v, None).unwrap_err();
    assert!(e.0.contains("resolve via Document"));
}

#[test]
fn too_few_inputs_error() {
    let f = sampled_1d(&[0, 128, 255], 3);
    let e = f.eval(&[]).unwrap_err();
    assert!(e.0.contains("input"));
}
