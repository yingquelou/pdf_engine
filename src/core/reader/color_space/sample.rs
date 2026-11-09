//! 颜色空间分量计数、原生值域与图像样本位流的解码（含 tint 函数求值）。

use crate::core::functions::Function;

use super::error::{ColorSpaceError, Result, malformed, unsupported};
use super::model::ColorSpaceModel;

impl ColorSpaceModel {
    /// 每像素的**样本**分量数（tint 域/索引/原生域解码前）；Pattern 无样本，
    /// 返回 `None`。
    pub fn component_count(&self) -> Option<usize> {
        match self {
            Self::DeviceGray | Self::CalGray(_) => Some(1),
            Self::DeviceRGB | Self::CalRGB(_) => Some(3),
            Self::DeviceCMYK => Some(4),
            Self::Lab(_) => Some(3),
            Self::IccBased(icc) => Some(icc.n),
            Self::Indexed(_) => Some(1),
            Self::Separation(_) => Some(1),
            Self::DeviceN(device_n) => Some(device_n.colorants.len()),
            Self::Pattern { .. } => None,
        }
    }

    /// 解码输出（原生空间）每像素分量数。
    pub fn native_component_count(&self) -> Option<usize> {
        match self {
            Self::Indexed(indexed) => indexed.base.native_component_count(),
            Self::Separation(separation) => separation.alternate.native_component_count(),
            Self::DeviceN(device_n) => device_n.alternate.native_component_count(),
            other => other.component_count(),
        }
    }

    /// 原生值域区间（解码输出未应用 `/Decode`，此处仅声明语义范围）。
    pub fn native_ranges(&self) -> Vec<(f64, f64)> {
        match self {
            Self::DeviceGray | Self::CalGray(_) => vec![(0.0, 1.0)],
            Self::DeviceRGB | Self::CalRGB(_) => vec![(0.0, 1.0); 3],
            Self::DeviceCMYK => vec![(0.0, 1.0); 4],
            Self::Lab(lab) => {
                let mut ranges = vec![(0.0, 100.0)];
                ranges.extend(lab.range.iter().copied());
                ranges
            }
            Self::IccBased(icc) => icc.range.clone(),
            Self::Indexed(indexed) => indexed.base.native_ranges(),
            Self::Separation(separation) => separation.alternate.native_ranges(),
            Self::DeviceN(device_n) => device_n.alternate.native_ranges(),
            Self::Pattern { base } => match base {
                Some(base) => base.native_ranges(),
                None => Vec::new(),
            },
        }
    }

    /// 把图像样本位流解码为规格化原生分量（像素优先、展平）。
    ///
    /// - `bits_per_component`：每样本位宽（设备/CIE/ICC/Separation/DeviceN
    ///   支持 1/2/4/8/16；Indexed 索引支持 1/2/4/8）；
    /// - `pixels_per_row`：每行像素数，行尾按字节补位；
    /// - Indexed：索引经调色板展开为基空间分量（调色板字节按 8 位归一）；
    /// - Separation / DeviceN：tint 规格值送 tint 函数求值到备用空间
    ///   （函数缺失时透传）。
    pub fn decode_samples(
        &self,
        bits_per_component: usize,
        pixels_per_row: usize,
        data: &[u8],
    ) -> Result<Vec<f64>> {
        self.decode_samples_with_decode(bits_per_component, pixels_per_row, data, None)
    }

    /// 同 [`ColorSpaceModel::decode_samples`]，但把 `/Decode` 数组应用于
    /// **查表/求值之前**的原始码值。
    ///
    /// 仅对 Indexed（映射调色板索引）与 Separation/DeviceN（映射 tint
    /// 输入）生效；其余色彩空间由调用方在解码后按原生分量映射，故此处
    /// 忽略 `decode`。Indexed 的映射结果规范未定义取整方式，本库约定
    /// 四舍五入并裁剪到 `[0, hival]`；tint 输入按变换域 `[0, 1]` 裁剪。
    pub fn decode_samples_with_decode(
        &self,
        bits_per_component: usize,
        pixels_per_row: usize,
        data: &[u8],
        decode: Option<&[f64]>,
    ) -> Result<Vec<f64>> {
        let raw_n = self
            .component_count()
            .ok_or_else(|| unsupported("pattern color space carries no image samples"))?;
        if pixels_per_row == 0 {
            return Err(malformed("pixels_per_row must be positive"));
        }
        let allowed: &[usize] = if matches!(self, Self::Indexed(_)) {
            &[1, 2, 4, 8]
        } else {
            &[1, 2, 4, 8, 16]
        };
        if !allowed.contains(&bits_per_component) {
            return Err(malformed(format!(
                "unsupported bits per component {bits_per_component}"
            )));
        }
        let max_code = (1u64 << bits_per_component) - 1;
        let decode = match (decode, self) {
            (Some(values), Self::Indexed(_) | Self::Separation(_) | Self::DeviceN(_)) => {
                if values.len() != raw_n * 2 {
                    return Err(malformed(format!(
                        "/Decode must contain {} numbers for a {raw_n}-component image, got {}",
                        raw_n * 2,
                        values.len()
                    )));
                }
                if values.iter().any(|value| !value.is_finite()) {
                    return Err(malformed("/Decode must contain finite numbers"));
                }
                Some(values)
            }
            _ => None,
        };
        let row_bytes = (bits_per_component * raw_n * pixels_per_row).div_ceil(8);
        if data.is_empty() || !data.len().is_multiple_of(row_bytes) {
            return Err(malformed(format!(
                "sample data length {} is not a whole number of {row_bytes}-byte rows",
                data.len()
            )));
        }

        let mut out = Vec::with_capacity(data.len() / row_bytes * pixels_per_row);
        for row in data.chunks(row_bytes) {
            for pixel in 0..pixels_per_row {
                let base_bit = pixel * raw_n * bits_per_component;
                let codes: Vec<u32> = (0..raw_n)
                    .map(|i| {
                        read_bits_msb(row, base_bit + i * bits_per_component, bits_per_component)
                    })
                    .collect();
                match self {
                    Self::Indexed(indexed) => {
                        let stride = indexed
                            .base
                            .native_component_count()
                            .ok_or_else(|| unsupported("Indexed base has no sample components"))?;
                        let index = match decode {
                            Some(pair) => {
                                // 规范未定义索引取整：本库四舍五入并裁剪到 [0, hival]。
                                let mapped = pair[0]
                                    + (codes[0] as f64 / max_code as f64) * (pair[1] - pair[0]);
                                mapped.clamp(0.0, indexed.hival as f64).round() as usize
                            }
                            None => (codes[0] as usize).min(indexed.hival),
                        };
                        let end = (index + 1)
                            .checked_mul(stride)
                            .ok_or_else(|| malformed("palette offset overflow"))?;
                        if indexed.lookup.len() < end {
                            return Err(malformed(format!(
                                "palette has {} bytes, index {index} needs {end}",
                                indexed.lookup.len()
                            )));
                        }
                        for byte in &indexed.lookup[index * stride..end] {
                            out.push(*byte as f64 / 255.0);
                        }
                    }
                    Self::Separation(separation) => {
                        let tints = decode_tints(&codes, max_code, decode);
                        out.extend(eval_tint(&separation.tint, &tints)?);
                    }
                    Self::DeviceN(device_n) => {
                        let tints = decode_tints(&codes, max_code, decode);
                        out.extend(eval_tint(&device_n.tint, &tints)?);
                    }
                    _ => out.extend(normalize_codes(&codes, max_code)),
                }
            }
        }
        Ok(out)
    }
}

fn normalize_codes(codes: &[u32], max_code: u64) -> Vec<f64> {
    codes
        .iter()
        .map(|code| *code as f64 / max_code as f64)
        .collect()
}

/// 把原始码值规格化并按 `/Decode` 映射为 tint 输入。
///
/// 映射结果裁剪到 tint 变换输入域 `[0, 1]`（规范未定义越界语义，本库
/// 约定裁剪；`Dmin > Dmax` 的镜像映射照常支持）。
fn decode_tints(codes: &[u32], max_code: u64, decode: Option<&[f64]>) -> Vec<f64> {
    let mut tints = normalize_codes(codes, max_code);
    if let Some(pairs) = decode {
        for (index, tint) in tints.iter_mut().enumerate() {
            let pair = &pairs[index * 2..index * 2 + 2];
            *tint = (pair[0] + *tint * (pair[1] - pair[0])).clamp(0.0, 1.0);
        }
    }
    tints
}

/// 从行位流按 MSB-first 读取 `bits` 位（1/2/4/8/16）。
fn read_bits_msb(row: &[u8], bit_offset: usize, bits: usize) -> u32 {
    let mut value = 0u32;
    for i in 0..bits {
        let bit = bit_offset + i;
        let on = (row[bit / 8] >> (7 - bit % 8)) & 1 == 1;
        value = (value << 1) | on as u32;
    }
    value
}

fn eval_tint(tint: &Option<Function>, tints: &[f64]) -> Result<Vec<f64>> {
    match tint {
        None => Ok(tints.to_vec()),
        Some(function) => function
            .eval(tints)
            .map_err(|error| ColorSpaceError::Function(error.to_string())),
    }
}
