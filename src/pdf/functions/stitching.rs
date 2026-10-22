//! Type 3 拼接函数（ISO 32000-1 §7.10.4，Table 42）。
//!
//! 把定义域切成 n 段（/Bounds 为 n−1 个内部分点），每段线性映射
//! （/Encode）到对应子函数的输入；逐段调用子函数求值。

use super::{err, parse_numbers, FunctionError, Result};
use crate::pdf::types::{Dict, StreamData, Value};

/// Type 3 拼接函数。
#[derive(Debug, Clone, PartialEq)]
pub struct StitchingFunction {
    /// 定义域。
    pub(crate) domain: Vec<(f64, f64)>,
    /// 值域（可选；存在则输出截断）。
    pub(crate) range: Option<Vec<(f64, f64)>>,
    /// 子函数（n ≥ 1，均应为一维输入）。
    functions: Vec<super::Function>,
    /// 内部分点（n−1 个，严格递增）。
    bounds: Vec<f64>,
    /// 每段的 (in_lo, in_hi) 编码对（n 个）。
    encode: Vec<(f64, f64)>,
}

impl StitchingFunction {
    pub(super) fn from_dict(dict: &Dict, depth: usize) -> Result<Self> {
        let domain = super::parse_ordered_pairs(dict, "Domain")?;
        if domain.len() != 1 {
            return err(format!(
                "type 3 function requires 1 input dimension, got {}",
                domain.len()
            ));
        }
        let range = super::optional_ordered_pairs(dict, "Range")?;

        // /Functions：n 个函数对象（字典或流）。
        let raw = dict
            .get("Functions")
            .ok_or_else(|| FunctionError("missing required key /Functions".into()))?;
        let arr = raw
            .as_array()
            .ok_or_else(|| FunctionError(format!("invalid /Functions: {}", raw.type_name())))?;
        if arr.is_empty() {
            return err("/Functions must not be empty");
        }
        let mut functions = Vec::with_capacity(arr.len());
        for item in arr.iter() {
            functions.push(build_sub_function(item, depth)?);
        }
        // 子函数必须一维输入。
        for (i, f) in functions.iter().enumerate() {
            if f.domain().len() != 1 {
                return err(format!(
                    "/Functions[{i}] must be 1-in (got {} input dims)",
                    f.domain().len()
                ));
            }
        }
        let n = functions.len();

        // /Bounds：n−1 个严格递增内部分点。
        let bounds = match dict.get("Bounds") {
            None | Some(Value::Null) => Vec::new(),
            Some(v) => parse_numbers(v, "Bounds")?,
        };
        if bounds.len() != n - 1 {
            return err(format!(
                "/Bounds must have {} value(s) for {n} subfunction(s), got {}",
                n - 1,
                bounds.len()
            ));
        }
        if bounds.windows(2).any(|w| w[0] >= w[1]) {
            return err("/Bounds values must be strictly increasing");
        }

        // /Encode：规范要求必填；缺省时取各子函数自身定义域（宽容恢复，
        // 符合「宽进」约定）。
        let encode = match dict.get("Encode") {
            None | Some(Value::Null) => functions
                .iter()
                .map(|f| (f.domain()[0].0, f.domain()[0].1))
                .collect(),
            Some(_) => {
                let e = super::parse_pairs(dict, "Encode")?;
                if e.len() != n {
                    return err(format!("/Encode must have {n} pair(s), got {}", e.len()));
                }
                e
            }
        };

        Ok(Self {
            domain,
            range,
            functions,
            bounds,
            encode,
        })
    }

    pub(super) fn eval(&self, input: &[f64]) -> Result<Vec<f64>> {
        let input = super::check_input(input, 1)?;
        let x = super::clamp_to(input[0], self.domain[0]);
        let n = self.functions.len();

        // 定位区间 k：左闭右开（末段右闭），即首个满足 x < bounds[k] 的下标。
        let k = self.bounds.partition_point(|&b| b <= x).min(n - 1);
        let lo = if k == 0 {
            self.domain[0].0
        } else {
            self.bounds[k - 1]
        };
        let hi = if k == n - 1 {
            self.domain[0].1
        } else {
            self.bounds[k]
        };
        let (e0, e1) = self.encode[k];
        let y = if (hi - lo).abs() < f64::EPSILON {
            e0
        } else {
            e0 + (x - lo) / (hi - lo) * (e1 - e0)
        };

        let mut out = self.functions[k].eval(&[y])?;
        super::clip_output(&mut out, self.range.as_deref());
        Ok(out)
    }
}

/// 构造 /Functions 数组中的子函数。
///
/// 字典直接递归；`Value::Stream` 携带 `StreamData::Owned` 时可用其字节
/// （Type 0/4 子函数）；`StreamData::Ref` / `Value::Ref` 无法在无
/// `Document` 上下文时取流字节，报错。
fn build_sub_function(value: &Value, depth: usize) -> Result<super::Function> {
    let dict = value
        .as_dict()
        .ok_or_else(|| FunctionError(format!("invalid /Functions element: {}", value.type_name())))?;
    let stream = match value {
        Value::Stream(_, StreamData::Owned(b)) => Some(b.as_slice()),
        Value::Stream(_, StreamData::Ref { .. }) => {
            return err(
                "sub-function stream data is a file reference; resolve via Document first",
            )
        }
        _ => None,
    };
    super::Function::from_dict_depth(dict, stream, depth + 1)
}
