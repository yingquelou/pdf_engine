//! Type 2 指数插值函数（ISO 32000-1 §7.10.3，Table 41）。
//!
//! 一维输入 n 维输出：`y_k = C0_k + (C1_k − C0_k) · x^N`。
//! 规范要求 m = 1（一维输入）。

use super::{Result, err, parse_numbers, required_number};
use crate::core::types::{Dict, Value};

/// Type 2 指数插值函数。
#[derive(Debug, Clone, PartialEq)]
pub struct ExponentialFunction {
    /// 定义域（m = 1）。
    pub(crate) domain: Vec<(f64, f64)>,
    /// 值域（可选；存在则输出截断）。
    pub(crate) range: Option<Vec<(f64, f64)>>,
    /// /C0（默认 0）。
    pub(crate) c0: Vec<f64>,
    /// /C1（默认 1）。
    c1: Vec<f64>,
    /// 指数 /N。
    n: f64,
}

impl ExponentialFunction {
    pub(super) fn from_dict(dict: &Dict) -> Result<Self> {
        let domain = super::parse_ordered_pairs(dict, "Domain")?;
        if domain.len() != 1 {
            return err(format!(
                "type 2 function requires 1 input dimension, got {}",
                domain.len()
            ));
        }
        let range = super::optional_ordered_pairs(dict, "Range")?;

        // /C0 /C1：缺省 [0] / [1]；两数组长度不齐时用缺省值补齐。
        let mut c0 = match dict.get("C0") {
            None | Some(Value::Null) => vec![0.0],
            Some(v) => parse_numbers(v, "C0")?,
        };
        let mut c1 = match dict.get("C1") {
            None | Some(Value::Null) => vec![1.0],
            Some(v) => parse_numbers(v, "C1")?,
        };
        let n_out = c0
            .len()
            .max(c1.len())
            .max(range.as_ref().map_or(1, |r| r.len()));
        if c0.is_empty() || c1.is_empty() {
            return err("/C0 and /C1 must not be empty");
        }
        c0.resize(n_out, 0.0);
        c1.resize(n_out, 1.0);

        let n = required_number(dict, "N")?;
        if n.is_nan() || n.is_infinite() {
            return err("/N must be a finite number");
        }

        Ok(Self {
            domain,
            range,
            c0,
            c1,
            n,
        })
    }

    pub(super) fn eval(&self, input: &[f64]) -> Result<Vec<f64>> {
        let input = super::check_input(input, 1)?;
        let x = super::clamp_to(input[0], self.domain[0]);
        let p = pow(x, self.n);
        let mut out: Vec<f64> = self
            .c0
            .iter()
            .zip(&self.c1)
            .map(|(&a, &b)| a + (b - a) * p)
            .collect();
        super::clip_output(&mut out, self.range.as_deref());
        Ok(out)
    }
}

/// `x^N`：负底数仅在 N 为整数时有实值，其余按 0 处理（宽进语义）；
/// `0^0` 约定为 1。
fn pow(x: f64, n: f64) -> f64 {
    if x == 0.0 {
        return if n == 0.0 { 1.0 } else { 0.0 };
    }
    if x < 0.0 {
        if n.fract() == 0.0 && n.abs() <= i32::MAX as f64 {
            return x.powi(n as i32);
        }
        return 0.0;
    }
    x.powf(n)
}
