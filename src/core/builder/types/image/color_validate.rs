//! 色彩空间参数的构建期校验：CIE-Based 数值有限性与范围、ICCBased 分量数
//! 与备用空间、Indexed 调色板长度，以及 Separation/DeviceN 的色料名与
//! tint 变换维数。

use crate::core::builder::types::Function;

use super::color_space::ColorSpace;

impl ColorSpace {
    /// 校验 CIE-Based 参数；设备空间恒通过。
    pub(crate) fn validate(&self) -> Result<(), String> {
        match self {
            ColorSpace::DeviceGray | ColorSpace::DeviceRGB | ColorSpace::DeviceCMYK => Ok(()),
            ColorSpace::CalGray(cal) => {
                validate_triplet("CalGray WhitePoint", &cal.white_point)?;
                if let Some(black_point) = cal.black_point {
                    validate_triplet("CalGray BlackPoint", &black_point)?;
                }
                if let Some(gamma) = cal.gamma
                    && (!gamma.is_finite() || gamma <= 0.0)
                {
                    return Err("CalGray Gamma must be a finite positive number".into());
                }
                Ok(())
            }
            ColorSpace::CalRGB(cal) => {
                validate_triplet("CalRGB WhitePoint", &cal.white_point)?;
                if let Some(black_point) = cal.black_point {
                    validate_triplet("CalRGB BlackPoint", &black_point)?;
                }
                if let Some(gamma) = cal.gamma
                    && gamma.iter().any(|g| !g.is_finite() || *g <= 0.0)
                {
                    return Err("CalRGB Gamma must contain 3 finite positive numbers".into());
                }
                if let Some(matrix) = cal.matrix
                    && matrix.iter().any(|v| !v.is_finite())
                {
                    return Err("CalRGB Matrix must contain 9 finite numbers".into());
                }
                Ok(())
            }
            ColorSpace::Lab(lab) => {
                validate_triplet("Lab WhitePoint", &lab.white_point)?;
                if let Some(black_point) = lab.black_point {
                    validate_triplet("Lab BlackPoint", &black_point)?;
                }
                if let Some(range) = lab.range {
                    if range.iter().any(|v| !v.is_finite()) {
                        return Err("Lab Range must contain 4 finite numbers".into());
                    }
                    if range[0] > range[1] || range[2] > range[3] {
                        return Err("Lab Range minimum must not exceed maximum".into());
                    }
                }
                Ok(())
            }
            ColorSpace::ICCBased(icc) => {
                if !matches!(icc.n, 1 | 3 | 4) {
                    return Err(format!("ICCBased N must be 1, 3 or 4, got {}", icc.n));
                }
                if icc.data.is_empty() {
                    return Err("ICCBased profile data must not be empty".into());
                }
                if let Some(range) = &icc.range {
                    let expected = icc.n as usize * 2;
                    if range.len() != expected {
                        return Err(format!(
                            "ICCBased Range must contain {expected} numbers, got {}",
                            range.len()
                        ));
                    }
                    if range.iter().any(|v| !v.is_finite()) {
                        return Err("ICCBased Range must contain finite numbers".into());
                    }
                    if range
                        .as_chunks::<2>()
                        .0
                        .iter()
                        .any(|pair| pair[0] > pair[1])
                    {
                        return Err("ICCBased Range minimum must not exceed maximum".into());
                    }
                }
                if let Some(alternate) = &icc.alternate {
                    if matches!(**alternate, ColorSpace::ICCBased(_)) {
                        return Err(
                            "ICCBased Alternate must not be another ICCBased color space".into(),
                        );
                    }
                    if alternate.channels() != icc.n as usize {
                        return Err(format!(
                            "ICCBased Alternate has {} components, expected {}",
                            alternate.channels(),
                            icc.n
                        ));
                    }
                    alternate
                        .validate()
                        .map_err(|message| format!("ICCBased Alternate: {message}"))?;
                }
                Ok(())
            }
            ColorSpace::Indexed(indexed) => {
                if matches!(*indexed.base, ColorSpace::Indexed(_)) {
                    return Err("Indexed base must not be another Indexed color space".into());
                }
                indexed
                    .base
                    .validate()
                    .map_err(|message| format!("Indexed base: {message}"))?;
                let expected = (indexed.hival as usize + 1) * indexed.base.channels();
                if indexed.lookup.len() != expected {
                    return Err(format!(
                        "Indexed lookup must contain {expected} bytes for hival {} and a {}-component base, got {}",
                        indexed.hival,
                        indexed.base.channels(),
                        indexed.lookup.len()
                    ));
                }
                Ok(())
            }
            ColorSpace::Separation(separation) => {
                if separation.colorant.is_empty() {
                    return Err("Separation colorant must not be empty".into());
                }
                validate_alternate("Separation", &separation.alternate)?;
                validate_tint_transform(
                    "Separation",
                    &separation.tint_transform,
                    1,
                    separation.alternate.channels(),
                )
            }
            ColorSpace::DeviceN(devicen) => {
                if devicen.colorants.is_empty() {
                    return Err("DeviceN must have at least one colorant".into());
                }
                let mut seen = std::collections::BTreeSet::new();
                for colorant in &devicen.colorants {
                    if colorant.is_empty() {
                        return Err("DeviceN colorant names must not be empty".into());
                    }
                    if colorant == "All" {
                        return Err("DeviceN must not use the reserved colorant name All".into());
                    }
                    if colorant != "None" && !seen.insert(colorant.as_str()) {
                        return Err(format!(
                            "DeviceN colorant names must be distinct, got duplicate {colorant}"
                        ));
                    }
                }
                validate_alternate("DeviceN", &devicen.alternate)?;
                validate_tint_transform(
                    "DeviceN",
                    &devicen.tint_transform,
                    devicen.colorants.len(),
                    devicen.alternate.channels(),
                )
            }
        }
    }
}

/// Separation/DeviceN 的备用空间只能是设备或 CIE-Based 空间。
fn validate_alternate(role: &str, alternate: &ColorSpace) -> Result<(), String> {
    match alternate {
        ColorSpace::DeviceGray
        | ColorSpace::DeviceRGB
        | ColorSpace::DeviceCMYK
        | ColorSpace::CalGray(_)
        | ColorSpace::CalRGB(_)
        | ColorSpace::Lab(_)
        | ColorSpace::ICCBased(_) => alternate
            .validate()
            .map_err(|message| format!("{role} alternate color space: {message}")),
        ColorSpace::Indexed(_) | ColorSpace::Separation(_) | ColorSpace::DeviceN(_) => Err(
            format!("{role} alternate color space must be a device or CIE-based color space"),
        ),
    }
}

/// tint 变换的输入/输出维数必须与色料数和备用空间通道数一致。
fn validate_tint_transform(
    role: &str,
    transform: &Function,
    inputs: usize,
    outputs: usize,
) -> Result<(), String> {
    let actual_inputs = transform.input_count();
    if actual_inputs != inputs {
        return Err(format!(
            "{role} tint transform must take {inputs} input(s), got {actual_inputs}"
        ));
    }
    if let Some(actual_outputs) = transform.output_count()
        && actual_outputs != outputs
    {
        return Err(format!(
            "{role} tint transform must produce {outputs} output(s) for the alternate color space, got {actual_outputs}"
        ));
    }
    Ok(())
}

fn validate_triplet(name: &str, values: &[f64; 3]) -> Result<(), String> {
    if values.iter().all(|v| v.is_finite()) {
        Ok(())
    } else {
        Err(format!("{name} must contain 3 finite numbers"))
    }
}
