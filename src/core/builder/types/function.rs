//! PDF 函数（ISO 32000-1 §7.10）：Type 0 采样 / Type 2 指数插值 /
//! Type 3 拼接 / Type 4 PostScript 计算器。
//!
//! 与读端 [`crate::core::functions`] 的求值 API 对应：这里只负责构造、
//! 校验与字典/流字节生成；Type 0 样本按 MSB-first 逐位打包，Type 4
//! 代码在构建期交由读端计算器解析器校验。
//!
//! Type 0 与 Type 4 必须写成流对象；Type 2 与 Type 3 写成字典即可。
//! Type 3 的子函数由组装阶段递归生成（流子函数分配独立对象号）。

use crate::core::functions::validate_type4_code;
use crate::core::types::{Array, Dict, Value};

/// 采样函数每样本位数上限（Table 40 允许 1/2/4/8/12/16/24/32）。
const MAX_SAMPLE_BITS: u32 = 32;

/// 函数对象：四种类型之一。
#[derive(Debug, Clone, PartialEq)]
pub enum Function {
    /// Type 0 采样函数。
    Sampled(SampledFunction),
    /// Type 2 指数插值函数。
    Exponential(ExponentialFunction),
    /// Type 3 拼接函数。
    Stitching(StitchingFunction),
    /// Type 4 PostScript 计算器函数。
    Calculator(CalculatorFunction),
}

impl Function {
    /// 函数类型号（0/2/3/4）。
    pub fn function_type(&self) -> i64 {
        match self {
            Self::Sampled(_) => 0,
            Self::Exponential(_) => 2,
            Self::Stitching(_) => 3,
            Self::Calculator(_) => 4,
        }
    }

    /// 输入维数（`/Domain` 的对数）。
    pub(crate) fn input_count(&self) -> usize {
        function_domain(self).len() / 2
    }

    /// 输出维数；无法从函数定义确定时返回 `None`。
    ///
    /// Type 0/4 的 `/Range` 必填；Type 2 省略 `/C0` `/C1` `/Range` 时按
    /// 规范默认为 1 个输出；Type 3 省略 `/Range` 时取首个子函数的输出维数。
    pub(crate) fn output_count(&self) -> Option<usize> {
        match self {
            Self::Sampled(function) => Some(function.range.len() / 2),
            Self::Exponential(function) => function
                .c0
                .as_ref()
                .or(function.c1.as_ref())
                .map(Vec::len)
                .or_else(|| function.range.as_ref().map(|range| range.len() / 2))
                .or(Some(1)),
            Self::Stitching(function) => function
                .range
                .as_ref()
                .map(|range| range.len() / 2)
                .or_else(|| function.functions.first().and_then(Self::output_count)),
            Self::Calculator(function) => Some(function.range.len() / 2),
        }
    }

    /// 校验参数并生成对象内容（构建阶段调用）。
    ///
    /// `nested` 由组装阶段提供，用于写出 Type 3 的子函数并返回其值
    /// （字典或流对象引用）。流函数的 `/Length` 由调用方补充。
    pub(crate) fn to_object<F>(&self, nested: &mut F) -> Result<FunctionObject, String>
    where
        F: FnMut(&Function) -> Result<Value, String>,
    {
        match self {
            Self::Sampled(function) => {
                let (dict, samples) = function.to_object()?;
                Ok(FunctionObject {
                    dict,
                    stream: Some(samples),
                })
            }
            Self::Exponential(function) => Ok(FunctionObject {
                dict: function.to_object()?,
                stream: None,
            }),
            Self::Stitching(function) => Ok(FunctionObject {
                dict: function.to_object(nested)?,
                stream: None,
            }),
            Self::Calculator(function) => {
                let dict = function.to_object()?;
                Ok(FunctionObject {
                    dict,
                    stream: Some(function.code.clone()),
                })
            }
        }
    }
}

/// 组装阶段的函数对象：字典 + 可选流字节。
pub(crate) struct FunctionObject {
    pub(crate) dict: Dict,
    /// `Some` 表示必须写成流对象（Type 0/4）。
    pub(crate) stream: Option<Vec<u8>>,
}

/// Type 0 采样函数（Table 40）。
#[derive(Debug, Clone, PartialEq)]
pub struct SampledFunction {
    /// 定义域，`2m` 个值。
    pub domain: Vec<f64>,
    /// 值域，`2n` 个值。
    pub range: Vec<f64>,
    /// 各维网格点数（`/Size`，均为正整数）。
    pub size: Vec<u32>,
    /// 每样本位数。
    pub bits_per_sample: u32,
    /// 可选 `/Encode`，`2m` 个值。
    pub encode: Option<Vec<f64>>,
    /// 可选 `/Decode`，`2n` 个值。
    pub decode: Option<Vec<f64>>,
    /// 样本原始整数码（按维度主序、每点 n 个连续值）。
    pub samples: Vec<f64>,
}

impl SampledFunction {
    /// 构造采样函数。
    pub fn new(
        domain: impl Into<Vec<f64>>,
        range: impl Into<Vec<f64>>,
        size: impl Into<Vec<u32>>,
        bits_per_sample: u32,
        samples: impl Into<Vec<f64>>,
    ) -> Self {
        Self {
            domain: domain.into(),
            range: range.into(),
            size: size.into(),
            bits_per_sample,
            encode: None,
            decode: None,
            samples: samples.into(),
        }
    }

    /// 设置 `/Encode`。
    pub fn with_encode(mut self, encode: impl Into<Vec<f64>>) -> Self {
        self.encode = Some(encode.into());
        self
    }

    /// 设置 `/Decode`。
    pub fn with_decode(mut self, decode: impl Into<Vec<f64>>) -> Self {
        self.decode = Some(decode.into());
        self
    }

    fn to_object(&self) -> Result<(Dict, Vec<u8>), String> {
        let m = validate_domain(&self.domain)?;
        let n = validate_range(&self.range)?;
        if self.size.len() != m {
            return Err(format!(
                "/Size must have {m} value(s), got {}",
                self.size.len()
            ));
        }
        if self.size.contains(&0) {
            return Err("/Size values must be positive integers".into());
        }
        if !matches!(
            self.bits_per_sample,
            1 | 2 | 4 | 8 | 12 | 16 | 24 | MAX_SAMPLE_BITS
        ) {
            return Err(format!(
                "/BitsPerSample must be one of 1/2/4/8/12/16/24/32, got {}",
                self.bits_per_sample
            ));
        }
        if let Some(encode) = &self.encode {
            validate_numbers(encode, "/Encode")?;
            if encode.len() != 2 * m {
                return Err(format!(
                    "/Encode must contain {} value(s), got {}",
                    2 * m,
                    encode.len()
                ));
            }
        }
        if let Some(decode) = &self.decode {
            validate_numbers(decode, "/Decode")?;
            if decode.len() != 2 * n {
                return Err(format!(
                    "/Decode must contain {} value(s), got {}",
                    2 * n,
                    decode.len()
                ));
            }
        }
        let expected = self
            .size
            .iter()
            .try_fold(n as u64, |acc, &size| acc.checked_mul(u64::from(size)))
            .ok_or_else(|| "sample count overflow".to_string())?;
        if self.samples.len() as u64 != expected {
            return Err(format!(
                "expected {expected} sample value(s), got {}",
                self.samples.len()
            ));
        }
        let bytes = pack_samples(&self.samples, self.bits_per_sample)?;

        let mut dict = Dict::new()
            .insert("FunctionType", Value::Integer(0))
            .insert("Domain", real_array(&self.domain))
            .insert("Range", real_array(&self.range))
            .insert(
                "Size",
                Value::Array(Array(
                    self.size
                        .iter()
                        .map(|&size| Value::Integer(i64::from(size)))
                        .collect(),
                )),
            )
            .insert(
                "BitsPerSample",
                Value::Integer(i64::from(self.bits_per_sample)),
            );
        if let Some(encode) = &self.encode {
            dict = dict.insert("Encode", real_array(encode));
        }
        if let Some(decode) = &self.decode {
            dict = dict.insert("Decode", real_array(decode));
        }
        Ok((dict, bytes))
    }
}

/// Type 2 指数插值函数（Table 41）。
#[derive(Debug, Clone, PartialEq)]
pub struct ExponentialFunction {
    /// 定义域，恰好 2 个值（一维输入）。
    pub domain: Vec<f64>,
    /// 可选值域，`2n` 个值。
    pub range: Option<Vec<f64>>,
    /// 可选 `/C0`（缺省 `[0]`）。
    pub c0: Option<Vec<f64>>,
    /// 可选 `/C1`（缺省 `[1]`）。
    pub c1: Option<Vec<f64>>,
    /// 指数 `/N`。
    pub n: f64,
}

impl ExponentialFunction {
    /// 构造一维指数插值函数。
    pub fn new(domain: impl Into<Vec<f64>>, n: f64) -> Self {
        Self {
            domain: domain.into(),
            range: None,
            c0: None,
            c1: None,
            n,
        }
    }

    /// 设置 `/C0`。
    pub fn with_c0(mut self, c0: impl Into<Vec<f64>>) -> Self {
        self.c0 = Some(c0.into());
        self
    }

    /// 设置 `/C1`。
    pub fn with_c1(mut self, c1: impl Into<Vec<f64>>) -> Self {
        self.c1 = Some(c1.into());
        self
    }

    /// 设置 `/Range`。
    pub fn with_range(mut self, range: impl Into<Vec<f64>>) -> Self {
        self.range = Some(range.into());
        self
    }

    fn to_object(&self) -> Result<Dict, String> {
        let m = validate_domain(&self.domain)?;
        if m != 1 {
            return Err(format!(
                "type 2 function requires 1 input dimension, got {m}"
            ));
        }
        let range_len = match &self.range {
            Some(range) => Some(validate_range(range)?),
            None => None,
        };
        if let Some(c0) = &self.c0 {
            validate_numbers(c0, "/C0")?;
        }
        if let Some(c1) = &self.c1 {
            validate_numbers(c1, "/C1")?;
        }
        let c0_len = self.c0.as_ref().map(Vec::len);
        let c1_len = self.c1.as_ref().map(Vec::len);
        if let (Some(a), Some(b)) = (c0_len, c1_len)
            && a != b
        {
            return Err(format!(
                "/C0 and /C1 must have equal lengths, got {a} and {b}"
            ));
        }
        let outputs = c0_len.or(c1_len).or(range_len);
        match (outputs, range_len) {
            (Some(outputs), Some(range_len)) if outputs != range_len => {
                return Err(format!(
                    "/Range describes {range_len} output(s), but /C0//C1 describe {outputs}"
                ));
            }
            (None, Some(0)) | (Some(0), _) => {
                return Err("/C0, /C1 and /Range must not be empty".into());
            }
            _ => {}
        }
        if !self.n.is_finite() {
            return Err("/N must be a finite number".into());
        }
        let mut dict = Dict::new()
            .insert("FunctionType", Value::Integer(2))
            .insert("Domain", real_array(&self.domain))
            .insert("N", Value::Float(self.n));
        if let Some(range) = &self.range {
            dict = dict.insert("Range", real_array(range));
        }
        if let Some(c0) = &self.c0 {
            dict = dict.insert("C0", real_array(c0));
        }
        if let Some(c1) = &self.c1 {
            dict = dict.insert("C1", real_array(c1));
        }
        Ok(dict)
    }
}

/// Type 3 拼接函数（Table 42）。
#[derive(Debug, Clone, PartialEq)]
pub struct StitchingFunction {
    /// 定义域，恰好 2 个值（一维输入）。
    pub domain: Vec<f64>,
    /// 可选值域，`2n` 个值。
    pub range: Option<Vec<f64>>,
    /// 子函数（非空，均为一维输入）。
    pub functions: Vec<Function>,
    /// 内部分点，数量为子函数数 − 1，严格递增。
    pub bounds: Vec<f64>,
    /// 可选 `/Encode`，`2n` 个值。
    pub encode: Option<Vec<f64>>,
}

impl StitchingFunction {
    /// 构造拼接函数。
    pub fn new(
        domain: impl Into<Vec<f64>>,
        functions: Vec<Function>,
        bounds: impl Into<Vec<f64>>,
    ) -> Self {
        Self {
            domain: domain.into(),
            range: None,
            functions,
            bounds: bounds.into(),
            encode: None,
        }
    }

    /// 设置 `/Range`。
    pub fn with_range(mut self, range: impl Into<Vec<f64>>) -> Self {
        self.range = Some(range.into());
        self
    }

    /// 设置 `/Encode`。
    pub fn with_encode(mut self, encode: impl Into<Vec<f64>>) -> Self {
        self.encode = Some(encode.into());
        self
    }

    fn to_object<F>(&self, nested: &mut F) -> Result<Dict, String>
    where
        F: FnMut(&Function) -> Result<Value, String>,
    {
        let m = validate_domain(&self.domain)?;
        if m != 1 {
            return Err(format!(
                "type 3 function requires 1 input dimension, got {m}"
            ));
        }
        let (lo, hi) = (self.domain[0], self.domain[1]);
        if self.functions.is_empty() {
            return Err("/Functions must not be empty".into());
        }
        let n = self.functions.len();
        if self.bounds.len() != n - 1 {
            return Err(format!(
                "/Bounds must have {} value(s) for {n} subfunction(s), got {}",
                n - 1,
                self.bounds.len()
            ));
        }
        validate_numbers(&self.bounds, "/Bounds")?;
        if self.bounds.windows(2).any(|window| window[0] >= window[1]) {
            return Err("/Bounds values must be strictly increasing".into());
        }
        if self.bounds.iter().any(|&bound| bound <= lo || bound >= hi) {
            return Err("/Bounds values must lie inside /Domain".into());
        }
        if let Some(range) = &self.range {
            validate_range(range)?;
        }
        if let Some(encode) = &self.encode {
            validate_numbers(encode, "/Encode")?;
            if encode.len() != 2 * n {
                return Err(format!(
                    "/Encode must contain {} value(s), got {}",
                    2 * n,
                    encode.len()
                ));
            }
        }
        let mut values = Array::new();
        for function in &self.functions {
            let domain = function_domain(function);
            if domain.len() != 2 {
                return Err(format!(
                    "type 3 subfunction must have 1 input dimension, got {}",
                    domain.len() / 2
                ));
            }
            values = values.push(nested(function)?);
        }
        let mut dict = Dict::new()
            .insert("FunctionType", Value::Integer(3))
            .insert("Domain", real_array(&self.domain))
            .insert("Functions", Value::Array(values))
            .insert("Bounds", real_array(&self.bounds));
        if let Some(range) = &self.range {
            dict = dict.insert("Range", real_array(range));
        }
        if let Some(encode) = &self.encode {
            dict = dict.insert("Encode", real_array(encode));
        }
        Ok(dict)
    }
}

/// Type 4 PostScript 计算器函数（Table 43）。
#[derive(Debug, Clone, PartialEq)]
pub struct CalculatorFunction {
    /// 定义域，`2m` 个值。
    pub domain: Vec<f64>,
    /// 值域，`2n` 个值。
    pub range: Vec<f64>,
    /// 计算器代码流。
    pub code: Vec<u8>,
}

impl CalculatorFunction {
    /// 构造计算器函数；代码在构建阶段校验。
    pub fn new(
        domain: impl Into<Vec<f64>>,
        range: impl Into<Vec<f64>>,
        code: impl AsRef<[u8]>,
    ) -> Self {
        Self {
            domain: domain.into(),
            range: range.into(),
            code: code.as_ref().to_vec(),
        }
    }

    fn to_object(&self) -> Result<Dict, String> {
        validate_domain(&self.domain)?;
        validate_range(&self.range)?;
        validate_type4_code(&self.code)
            .map_err(|error| format!("invalid /Code stream: {error}"))?;
        Ok(Dict::new()
            .insert("FunctionType", Value::Integer(4))
            .insert("Domain", real_array(&self.domain))
            .insert("Range", real_array(&self.range)))
    }
}

/// 函数定义域（`2m` 个数折叠为 m 对）。
fn function_domain(function: &Function) -> &[f64] {
    match function {
        Function::Sampled(function) => &function.domain,
        Function::Exponential(function) => &function.domain,
        Function::Stitching(function) => &function.domain,
        Function::Calculator(function) => &function.domain,
    }
}

/// 校验定义域：非空偶数个有限值，每对 `lo <= hi`；返回输入维数 m。
fn validate_domain(domain: &[f64]) -> Result<usize, String> {
    validate_numbers(domain, "/Domain")?;
    if domain.is_empty() || !domain.len().is_multiple_of(2) {
        return Err("/Domain must contain an even, non-zero number of values".into());
    }
    for pair in domain.as_chunks::<2>().0 {
        if pair[0] > pair[1] {
            return Err("/Domain ranges must be in ascending order".into());
        }
    }
    Ok(domain.len() / 2)
}

/// 校验值域：非空偶数个有限值，每对 `lo <= hi`；返回值维数 n。
fn validate_range(range: &[f64]) -> Result<usize, String> {
    validate_numbers(range, "/Range")?;
    if range.is_empty() || !range.len().is_multiple_of(2) {
        return Err("/Range must contain an even, non-zero number of values".into());
    }
    for pair in range.as_chunks::<2>().0 {
        if pair[0] > pair[1] {
            return Err("/Range ranges must be in ascending order".into());
        }
    }
    Ok(range.len() / 2)
}

fn validate_numbers(values: &[f64], key: &str) -> Result<(), String> {
    if values.iter().any(|value| !value.is_finite()) {
        return Err(format!("{key} values must be finite"));
    }
    Ok(())
}

/// 把样本整数码按 MSB-first 逐位打包。
pub(crate) fn pack_samples(samples: &[f64], bits: u32) -> Result<Vec<u8>, String> {
    if !(1..=MAX_SAMPLE_BITS).contains(&bits) {
        return Err(format!("/BitsPerSample is out of range: {bits}"));
    }
    let max_code = ((1u64 << bits) - 1) as f64;
    let mut bytes = Vec::with_capacity((samples.len() * bits as usize).div_ceil(8));
    let mut accumulator: u64 = 0;
    let mut pending = 0u32;
    for &sample in samples {
        if !sample.is_finite() || sample < 0.0 || sample.fract() != 0.0 || sample > max_code {
            return Err(format!(
                "sample {sample} does not fit in {bits} bit(s) as an unsigned integer"
            ));
        }
        accumulator = (accumulator << bits) | sample as u64;
        pending += bits;
        while pending >= 8 {
            pending -= 8;
            bytes.push((accumulator >> pending) as u8);
        }
    }
    if pending > 0 {
        bytes.push((accumulator << (8 - pending)) as u8);
    }
    Ok(bytes)
}

fn real_array(values: &[f64]) -> Value {
    Value::Array(Array(
        values.iter().map(|&value| Value::Float(value)).collect(),
    ))
}
