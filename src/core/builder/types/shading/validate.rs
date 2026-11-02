//! 渐变构造的共享校验：数值有限性与参数域、颜色函数维数匹配、
//! 网格位深规范枚举以及网格顶点与网格片的颜色分量解码范围校验。

use crate::core::builder::types::{ColorSpace, Function};

use super::mesh::{MeshFunction, MeshVertex, mesh_component_ranges};

pub(super) fn ensure_finite(values: &[f64], role: &str) -> Result<(), String> {
    if values.iter().any(|v| !v.is_finite()) {
        return Err(format!("{role} must contain finite numbers"));
    }
    Ok(())
}

pub(super) fn validate_domain(domain: [f64; 2], role: &str) -> Result<(), String> {
    ensure_finite(&domain, role)?;
    if domain[0] >= domain[1] {
        return Err(format!("{role} minimum must be less than maximum"));
    }
    Ok(())
}

/// 校验渐变颜色函数：输入维数与颜色空间分量数必须匹配。
pub(super) fn validate_color_function(
    role: &str,
    color_space: &ColorSpace,
    function: &Function,
    inputs: usize,
) -> Result<(), String> {
    color_space
        .validate()
        .map_err(|message| format!("{role} color space: {message}"))?;
    let actual_inputs = function.input_count();
    if actual_inputs != inputs {
        return Err(format!(
            "{role} function must take {inputs} input(s), got {actual_inputs}"
        ));
    }
    let channels = color_space.channels();
    if let Some(outputs) = function.output_count()
        && outputs != channels
    {
        return Err(format!(
            "{role} function must produce {channels} output(s) for its color space, got {outputs}"
        ));
    }
    Ok(())
}

/// 规范允许的网格位深（Table 82–85）。
const MESH_COORDINATE_BITS: [u8; 8] = [1, 2, 4, 8, 12, 16, 24, 32];
const MESH_COMPONENT_BITS: [u8; 6] = [1, 2, 4, 8, 12, 16];

fn validate_mesh_bits(
    role: &str,
    bits_per_coordinate: u8,
    bits_per_component: u8,
) -> Result<(), String> {
    if !MESH_COORDINATE_BITS.contains(&bits_per_coordinate) {
        return Err(format!(
            "{role} BitsPerCoordinate must be one of 1/2/4/8/12/16/24/32, got {bits_per_coordinate}"
        ));
    }
    if !MESH_COMPONENT_BITS.contains(&bits_per_component) {
        return Err(format!(
            "{role} BitsPerComponent must be one of 1/2/4/8/12/16, got {bits_per_component}"
        ));
    }
    Ok(())
}

/// 网格 `/Function`：单输入、输出数等于颜色空间分量数，且不得用于 Indexed。
fn validate_mesh_function(
    role: &str,
    color_space: &ColorSpace,
    function: &MeshFunction,
) -> Result<(), String> {
    color_space
        .validate()
        .map_err(|message| format!("{role} color space: {message}"))?;
    if matches!(color_space, ColorSpace::Indexed(_)) {
        return Err(format!(
            "{role} Function must not be used with an Indexed colour space"
        ));
    }
    let channels = color_space.channels();
    match function {
        MeshFunction::Single(function) => {
            let inputs = function.input_count();
            if inputs != 1 {
                return Err(format!("{role} Function must take 1 input, got {inputs}"));
            }
            if let Some(outputs) = function.output_count()
                && outputs != channels
            {
                return Err(format!(
                    "{role} Function must produce {channels} output(s), got {outputs}"
                ));
            }
        }
        MeshFunction::Components(functions) => {
            if functions.len() != channels {
                return Err(format!(
                    "{role} Function array must contain {channels} function(s), got {}",
                    functions.len()
                ));
            }
            for (index, function) in functions.iter().enumerate() {
                let inputs = function.input_count();
                if inputs != 1 {
                    return Err(format!(
                        "{role} Function array entry {index} must take 1 input, got {inputs}"
                    ));
                }
                if let Some(outputs) = function.output_count()
                    && outputs != 1
                {
                    return Err(format!(
                        "{role} Function array entry {index} must produce 1 output, got {outputs}"
                    ));
                }
            }
        }
    }
    Ok(())
}

/// 校验网格渐变的公共参数：位深、顶点坐标与颜色分量。
pub(super) fn validate_mesh(
    role: &str,
    color_space: &ColorSpace,
    vertices: &[MeshVertex],
    function: Option<&MeshFunction>,
    decode: Option<[f64; 4]>,
    bits_per_coordinate: u8,
    bits_per_component: u8,
) -> Result<(), String> {
    color_space
        .validate()
        .map_err(|message| format!("{role} color space: {message}"))?;
    validate_mesh_bits(role, bits_per_coordinate, bits_per_component)?;
    if vertices.is_empty() {
        return Err(format!("{role} requires at least one vertex"));
    }
    if let Some(decode) = decode {
        ensure_finite(&decode, &format!("{role} Decode"))?;
        if decode[0] >= decode[1] || decode[2] >= decode[3] {
            return Err(format!("{role} Decode minimum must be less than maximum"));
        }
    }
    if let Some(function) = function {
        validate_mesh_function(role, color_space, function)?;
    }
    let ranges = mesh_component_ranges(color_space, function.is_some());
    for vertex in vertices {
        if !vertex.x.is_finite() || !vertex.y.is_finite() {
            return Err(format!("{role} vertex coordinates must be finite"));
        }
        if vertex.color.len() != ranges.len() {
            return Err(format!(
                "{role} vertex must have {} colour component(s), got {}",
                ranges.len(),
                vertex.color.len()
            ));
        }
        for (value, [min, max]) in vertex.color.iter().zip(&ranges) {
            if !value.is_finite() || *value < *min || *value > *max {
                return Err(format!(
                    "{role} colour component {value} is outside [{min}, {max}]"
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn validate_patch_common(
    role: &str,
    color_space: &ColorSpace,
    function: Option<&MeshFunction>,
    decode: Option<[f64; 4]>,
    bits_per_coordinate: u8,
    bits_per_component: u8,
) -> Result<(), String> {
    color_space
        .validate()
        .map_err(|message| format!("{role} color space: {message}"))?;
    validate_mesh_bits(role, bits_per_coordinate, bits_per_component)?;
    if let Some(decode) = decode {
        ensure_finite(&decode, &format!("{role} Decode"))?;
        if decode[0] >= decode[1] || decode[2] >= decode[3] {
            return Err(format!("{role} Decode minimum must be less than maximum"));
        }
    }
    if let Some(function) = function {
        validate_mesh_function(role, color_space, function)?;
    }
    Ok(())
}

pub(super) fn validate_patch_points(role: &str, points: &[[f64; 2]]) -> Result<(), String> {
    if points.iter().flatten().any(|value| !value.is_finite()) {
        return Err(format!("{role} control point coordinates must be finite"));
    }
    Ok(())
}

pub(super) fn validate_patch_colors(
    role: &str,
    color_space: &ColorSpace,
    colors: &[Vec<f64>],
    has_function: bool,
) -> Result<(), String> {
    let ranges = mesh_component_ranges(color_space, has_function);
    if colors.len() != 4 {
        return Err(format!(
            "{role} patch must have 4 corner colours, got {}",
            colors.len()
        ));
    }
    for color in colors {
        if color.len() != ranges.len() {
            return Err(format!(
                "{role} corner colour must have {} component(s), got {}",
                ranges.len(),
                color.len()
            ));
        }
        for (value, [min, max]) in color.iter().zip(&ranges) {
            if !value.is_finite() || *value < *min || *value > *max {
                return Err(format!(
                    "{role} colour component {value} is outside [{min}, {max}]"
                ));
            }
        }
    }
    Ok(())
}
