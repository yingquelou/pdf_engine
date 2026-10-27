//! PDF 函数（ISO 32000-1 §7.10）：Type 0 采样 / Type 2 指数插值 /
//! Type 3 拼接 / Type 4 PostScript 计算器。
//!
//! 本模块是**纯 PDF 语义层的独立求值 API**：从函数字典（可带流数据）
//! 构造 [`Function`]，对给定的 m 维输入求 n 维输出。供后续 Shading /
//! 半调（阶段 C）等特性复用；当前不接入内容流（读端不解析内容流操作符
//! 是本库的既定约定）。
//!
//! 模块拆分：
//! - `sampled`：Type 0（采样函数：多位打包、/Encode、多线性插值）；
//! - `exponential`：Type 2（指数插值）；
//! - `stitching`：Type 3（拼接；递归构造子函数，限深 64）；
//! - `calculator`：Type 4（PostScript 计算器子集）；
//! - 本文件：`Function` 枚举、构造入口与公共参数解析辅助。
//!
//! **间接引用**：本 API 工作在已解引用的 `Value` 之上。`Value::Stream`
//! 携带 `StreamData::Owned` 时直接使用其字节；`StreamData::Ref` 或
//! `Value::Ref` 在无 `Document` 上下文时无法取流字节，构造报错——
//! 调用方应先用 `Document` 解出对象与流字节再传入。

mod calculator;
mod exponential;
mod sampled;
mod stitching;

#[cfg(test)]
mod tests;
use crate::core::types::{Dict, StreamData, Value};
use std::fmt;

/// 函数构造 / 求值失败原因（参数缺失或非法、数据损坏、求值错误）。
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionError(pub String);

impl fmt::Display for FunctionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for FunctionError {}

pub(crate) fn err<T>(msg: impl Into<String>) -> Result<T> {
    Err(FunctionError(msg.into()))
}

/// 校验 Type 4 代码流可被计算器解析（写端构建期使用）。
pub(crate) fn validate_type4_code(code: &[u8]) -> Result<()> {
    calculator::validate_code(code)
}

pub(crate) type Result<T> = std::result::Result<T, FunctionError>;

/// 嵌套函数（Type 3 的 /Functions 数组）最大构造深度，防环。
pub(crate) const MAX_NESTING_DEPTH: usize = 64;

/// PDF 函数：四种类型之一（ISO 32000-1 Table 40–43）。
#[derive(Debug, Clone, PartialEq)]
pub enum Function {
    /// Type 0：采样函数（Sampled）。
    Sampled(sampled::SampledFunction),
    /// Type 2：指数插值函数（Exponential interpolation）。
    Exponential(exponential::ExponentialFunction),
    /// Type 3：拼接函数（Stitching）。
    Stitching(stitching::StitchingFunction),
    /// Type 4：PostScript 计算器函数。
    Calculator(calculator::CalculatorFunction),
}

impl Function {
    /// 从函数对象构造。`value` 为函数字典或流；Type 0/4 需要流数据
    /// （`/Samples`、代码），按下列顺序获取：
    ///
    /// 1. `value` 为携带 `StreamData::Owned` 的 `Value::Stream` → 直接用；
    /// 2. 否则用调用方显式传入的 `stream`（已从 `Document` 解出的字节）。
    pub fn from_value(value: &Value, stream: Option<&[u8]>) -> Result<Self> {
        let dict = value
            .as_dict()
            .ok_or_else(|| FunctionError(format!("function object is {}", value.type_name())))?;
        let data = match value {
            Value::Stream(_, StreamData::Owned(b)) => Some(b.as_slice()),
            Value::Stream(_, StreamData::Ref { .. }) => {
                return err("function stream data is a file reference; resolve via Document first");
            }
            _ => stream,
        };
        Self::from_dict(dict, data)
    }

    /// 从函数字典构造。Type 0/4 必须传 `stream`（流解码后的字节）。
    pub fn from_dict(dict: &Dict, stream: Option<&[u8]>) -> Result<Self> {
        Self::from_dict_depth(dict, stream, 0)
    }

    pub(crate) fn from_dict_depth(
        dict: &Dict,
        stream: Option<&[u8]>,
        depth: usize,
    ) -> Result<Self> {
        if depth > MAX_NESTING_DEPTH {
            return err(format!(
                "function nesting exceeds {MAX_NESTING_DEPTH} levels (possible cycle)"
            ));
        }
        let ftype = match dict.get("FunctionType") {
            Some(v) => v.as_integer().ok_or_else(|| {
                FunctionError(format!("invalid /FunctionType: {}", v.type_name()))
            })?,
            None => return err("missing required key /FunctionType"),
        };
        match ftype {
            0 => Ok(Function::Sampled(sampled::SampledFunction::from_dict(
                dict, stream,
            )?)),
            2 => Ok(Function::Exponential(
                exponential::ExponentialFunction::from_dict(dict)?,
            )),
            3 => Ok(Function::Stitching(
                stitching::StitchingFunction::from_dict(dict, depth)?,
            )),
            4 => Ok(Function::Calculator(
                calculator::CalculatorFunction::from_dict(dict, stream)?,
            )),
            other => err(format!("unsupported /FunctionType {other}")),
        }
    }

    /// 对 m 维输入求值，返回 n 维输出（已按 /Range 截断）。
    ///
    /// 输入不足 m 个报错；多余输入忽略。
    pub fn eval(&self, input: &[f64]) -> Result<Vec<f64>> {
        match self {
            Function::Sampled(f) => f.eval(input),
            Function::Exponential(f) => f.eval(input),
            Function::Stitching(f) => f.eval(input),
            Function::Calculator(f) => f.eval(input),
        }
    }

    /// 定义域，`2m` 个数折叠为 m 个 `(min, max)`。
    pub fn domain(&self) -> &[(f64, f64)] {
        match self {
            Function::Sampled(f) => &f.domain,
            Function::Exponential(f) => &f.domain,
            Function::Stitching(f) => &f.domain,
            Function::Calculator(f) => &f.domain,
        }
    }

    /// 值域（Type 0/4 必有；Type 2/3 可选）。
    pub fn range(&self) -> Option<&[(f64, f64)]> {
        match self {
            Function::Sampled(f) => Some(&f.range),
            Function::Exponential(f) => f.range.as_deref(),
            Function::Stitching(f) => f.range.as_deref(),
            Function::Calculator(f) => Some(&f.range),
        }
    }

    /// 输出分量数：Type 0/4 取 `/Range` 对数；Type 2 取 `/C0` 元素数；
    /// Type 3 有 `/Range` 时取对数，否则取首个子函数的输出数。
    pub fn output_count(&self) -> Option<usize> {
        match self {
            Function::Sampled(f) => Some(f.range.len()),
            Function::Exponential(f) => Some(f.c0.len()),
            Function::Stitching(f) => match &f.range {
                Some(range) => Some(range.len()),
                None => f.functions.first().and_then(|first| first.output_count()),
            },
            Function::Calculator(f) => Some(f.range.len()),
        }
    }

    /// 函数类型号（0/2/3/4）。
    pub fn function_type(&self) -> i64 {
        match self {
            Function::Sampled(_) => 0,
            Function::Exponential(_) => 2,
            Function::Stitching(_) => 3,
            Function::Calculator(_) => 4,
        }
    }
}

// ---------- 公共参数解析辅助（各类型子模块共用） ----------

/// 把单个 `Value` 解析为 f64（Integer / Float）。
pub(crate) fn parse_number(v: &Value) -> Option<f64> {
    match v {
        Value::Integer(i) => Some(*i as f64),
        Value::Float(f) => Some(*f),
        _ => None,
    }
}

/// 把 `Value::Array`（或单个数值）解析为 f64 数组。
pub(crate) fn parse_numbers(v: &Value, what: &str) -> Result<Vec<f64>> {
    match v {
        Value::Array(a) => {
            let mut out = Vec::with_capacity(a.len());
            for item in a.iter() {
                out.push(parse_number(item).ok_or_else(|| {
                    FunctionError(format!(
                        "invalid {what}: non-numeric element {}",
                        item.type_name()
                    ))
                })?);
            }
            Ok(out)
        }
        other => parse_number(other)
            .map(|n| vec![n])
            .ok_or_else(|| FunctionError(format!("invalid {what}: {}", other.type_name()))),
    }
}

/// 取必填数值键。
pub(crate) fn required_number(dict: &Dict, key: &str) -> Result<f64> {
    let v = dict
        .get(key)
        .ok_or_else(|| FunctionError(format!("missing required key /{key}")))?;
    parse_number(v).ok_or_else(|| FunctionError(format!("invalid /{key}: {}", v.type_name())))
}

/// 把 `2m` 个数的数组折叠为 m 个 `(lo, hi)` 区间，**保持原顺序**。
///
/// 用于 /Encode、/Decode——二者允许 `lo > hi` 表达反向映射
/// （规范 7.10.4 示例即用 `Encode [1.0 0.0]` 实现 g(x) = f(1−x)）。
pub(crate) fn parse_pairs(dict: &Dict, key: &str) -> Result<Vec<(f64, f64)>> {
    let v = dict
        .get(key)
        .ok_or_else(|| FunctionError(format!("missing required key /{key}")))?;
    let nums = parse_numbers(v, key)?;
    if nums.is_empty() || nums.len() % 2 != 0 {
        return err(format!("/{key} must contain an even number of values"));
    }
    Ok(nums.chunks(2).map(|c| (c[0], c[1])).collect())
}

/// 同 [`parse_pairs`]，但对 `lo > hi` 的区间做交换归一。
///
/// 用于 /Domain、/Range——规范要求二者按升序给出，交换属于宽容恢复。
pub(crate) fn parse_ordered_pairs(dict: &Dict, key: &str) -> Result<Vec<(f64, f64)>> {
    let pairs = parse_pairs(dict, key)?;
    Ok(pairs
        .into_iter()
        .map(|(lo, hi)| if lo > hi { (hi, lo) } else { (lo, hi) })
        .collect())
}

/// 取可选区间数组，升序归一（用于 /Range）。
pub(crate) fn optional_ordered_pairs(dict: &Dict, key: &str) -> Result<Option<Vec<(f64, f64)>>> {
    match dict.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(_) => parse_ordered_pairs(dict, key).map(Some),
    }
}

/// 把输入截断到定义域。
pub(crate) fn clamp_to(x: f64, (lo, hi): (f64, f64)) -> f64 {
    x.clamp(lo, hi)
}

/// 把输出截断到值域（若提供）。
pub(crate) fn clip_output(out: &mut [f64], range: Option<&[(f64, f64)]>) {
    if let Some(range) = range {
        for (y, (lo, hi)) in out.iter_mut().zip(range) {
            *y = y.clamp(*lo, *hi);
        }
    }
}

/// 校验「输入个数 ≥ m」并返回截断到 m 的输入切片。
pub(crate) fn check_input(input: &[f64], m: usize) -> Result<&[f64]> {
    if input.len() < m {
        return err(format!(
            "function expects {m} input(s), got {}",
            input.len()
        ));
    }
    Ok(&input[..m])
}
