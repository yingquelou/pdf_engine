//! Type 0 采样函数（ISO 32000-1 §7.10.2，Table 40）。
//!
//! 样本存于流中按 `/BitsPerSample`（1/2/4/8/12/16/24/32）逐位 MSB-first
//! 打包，维度主序展开（第一维变化最慢），每点连续放 n 个输出值。
//! 求值：输入经 /Encode 归一到各维网格下标，做**多线性插值**
//! （`/Order` 3 三次插值暂不支持，构造时报错），输出经 /Decode 映射并
//! 截断到 /Range。

use super::{err, parse_numbers, FunctionError, Result};
use crate::core::types::Dict;

/// Type 0 采样函数。
#[derive(Debug, Clone, PartialEq)]
pub struct SampledFunction {
    /// 定义域。
    pub(crate) domain: Vec<(f64, f64)>,
    /// 值域（Type 0 必填）。
    pub(crate) range: Vec<(f64, f64)>,
    /// 各维网格点数（/Size）。
    size: Vec<usize>,
    /// 每样本位数（/BitsPerSample）。
    #[allow(dead_code)]
    bits_per_sample: u32,
    /// /Encode（默认 [0 size-1] 每维）。
    encode: Vec<(f64, f64)>,
    /// 已解码为实数的样本表：`维度主序` 展开，每点 n_out 个值。
    samples: Vec<f64>,
}

impl SampledFunction {
    pub(super) fn from_dict(dict: &Dict, stream: Option<&[u8]>) -> Result<Self> {
        let domain = super::parse_ordered_pairs(dict, "Domain")?;
        let range = super::parse_ordered_pairs(dict, "Range")?;
        let m = domain.len();
        let n_out = range.len();

        // /Size：m 个正整数
        let size = dict
            .get("Size")
            .ok_or_else(|| FunctionError("missing required key /Size".into()))
            .and_then(|v| parse_numbers(v, "Size"))?;
        if size.len() != m {
            return err(format!(
                "/Size must have {m} value(s), got {}",
                size.len()
            ));
        }
        let size: Vec<usize> = size
            .iter()
            .map(|&s| {
                if s < 1.0 || s.fract() != 0.0 {
                    return Err(FunctionError(format!(
                        "/Size values must be positive integers, got {s}"
                    )));
                }
                Ok(s as usize)
            })
            .collect::<Result<_>>()?;

        // /BitsPerSample ∈ {1, 2, 4, 8, 12, 16, 24, 32}
        let bits_raw = super::required_number(dict, "BitsPerSample")?;
        if !matches!(bits_raw as i64, 1 | 2 | 4 | 8 | 12 | 16 | 24 | 32) || bits_raw.fract() != 0.0
        {
            return err(format!(
                "/BitsPerSample must be one of 1/2/4/8/12/16/24/32, got {bits_raw}"
            ));
        }
        let bits = bits_raw as u32;

        // /Order：1 线性（默认）或 3 三次（不支持，明确报错）
        match dict.get("Order") {
            None | Some(crate::core::types::Value::Null) => {}
            Some(v) => {
                let order = v
                    .as_integer()
                    .ok_or_else(|| FunctionError(format!("invalid /Order: {}", v.type_name())))?;
                if order != 1 {
                    return err(format!(
                        "sampled function /Order {order} (cubic) is not supported"
                    ));
                }
            }
        }

        // /Encode：默认 [0, size_j - 1] 每维
        let encode = match dict.get("Encode") {
            None | Some(crate::core::types::Value::Null) => size
                .iter()
                .map(|&s| (0.0, (s - 1) as f64))
                .collect(),
            Some(_) => {
                let e = super::parse_pairs(dict, "Encode")?;
                if e.len() != m {
                    return err(format!("/Encode must have {m} pair(s), got {}", e.len()));
                }
                e
            }
        };

        // /Samples：流数据，按位解包后经 /Decode 映射为实数
        let data = stream
            .ok_or_else(|| FunctionError("type 0 function requires /Samples stream data".into()))?;
        let total_points: usize = size.iter().product();
        let total_values = total_points
            .checked_mul(n_out)
            .ok_or_else(|| FunctionError("sample count overflow".into()))?;
        let raw = read_bits(data, bits, total_values)?;
        let max_code = ((1u64 << bits) - 1) as f64;
        let decode = match dict.get("Decode") {
            None | Some(crate::core::types::Value::Null) => range.clone(),
            Some(_) => {
                // /Decode 允许 lo > hi 的反向映射，保持原顺序
                let d = super::parse_pairs(dict, "Decode")?;
                if d.len() != n_out {
                    return err(format!("/Decode must have {n_out} pair(s), got {}", d.len()));
                }
                d
            }
        };
        // 解包顺序：维度主序、每点 n_out 个连续值（与插值索引一致）。
        let mut samples = Vec::with_capacity(total_values);
        for &code in &raw {
            let normalized = code as f64 / max_code;
            let k = samples.len() % n_out;
            samples.push(decode[k].0 + (decode[k].1 - decode[k].0) * normalized);
        }

        Ok(Self {
            domain,
            range,
            size,
            bits_per_sample: bits,
            encode,
            samples,
        })
    }

    pub(super) fn eval(&self, input: &[f64]) -> Result<Vec<f64>> {
        let m = self.domain.len();
        let n_out = self.range.len();
        let input = super::check_input(input, m)?;

        // 归一到各维网格下标：floor + 小数部分。
        let mut idx = vec![0usize; m];
        let mut frac = vec![0.0f64; m];
        for j in 0..m {
            let x = super::clamp_to(input[j], self.domain[j]);
            let (e0, e1) = self.encode[j];
            let s_max = (self.size[j] - 1) as f64;
            let span = self.domain[j].1 - self.domain[j].0;
            let mut e = if span.abs() < f64::EPSILON {
                e0
            } else {
                e0 + (x - self.domain[j].0) / span * (e1 - e0)
            };
            if e < 0.0 {
                e = 0.0;
            } else if e > s_max {
                e = s_max;
            }
            let i = e.floor();
            if i as usize >= self.size[j] {
                idx[j] = self.size[j] - 1;
                frac[j] = 0.0;
            } else {
                idx[j] = i as usize;
                frac[j] = e - i;
            }
        }

        // 遍历 2^m 个插值角点，权重 = 各维 (1-frac) / frac 的乘积。
        let mut out = vec![0.0f64; n_out];
        for corner in 0..(1u32 << m) {
            let mut weight = 1.0f64;
            let mut flat = 0usize;
            for j in 0..m {
                let bit = (corner >> j) & 1;
                weight *= if bit == 1 { frac[j] } else { 1.0 - frac[j] };
                flat = flat * self.size[j] + idx[j] + bit as usize;
            }
            if weight == 0.0 {
                continue;
            }
            let base = flat * n_out;
            for (k, o) in out.iter_mut().enumerate() {
                *o += weight * self.samples[base + k];
            }
        }

        super::clip_output(&mut out, Some(&self.range));
        Ok(out)
    }
}

/// 从 MSB-first 位流读取 `count` 个 `width` 位无符号整数；数据不足报错。
fn read_bits(data: &[u8], width: u32, count: usize) -> Result<Vec<u32>> {
    let total_bits = count
        .checked_mul(width as usize)
        .ok_or_else(|| FunctionError("bit count overflow".into()))?;
    if data.len() * 8 < total_bits {
        return err(format!(
            "sample data truncated: need {total_bits} bits, got {}",
            data.len() * 8
        ));
    }
    let mut out = Vec::with_capacity(count);
    let mut bit_pos = 0usize;
    for _ in 0..count {
        let mut v = 0u32;
        for _ in 0..width {
            let byte = data[bit_pos >> 3];
            let bit = (byte >> (7 - (bit_pos & 7))) & 1;
            v = (v << 1) | bit as u32;
            bit_pos += 1;
        }
        out.push(v);
    }
    Ok(out)
}
