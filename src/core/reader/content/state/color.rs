//! 单一绘画方向的当前颜色值（§8.6.8）与设备色算子的分量助手。

use super::error::{ContentStateError, invalid, require_finite};

/// Current colour in one painting direction (§8.6.8).
#[derive(Debug, Clone, PartialEq)]
pub struct ColorValue {
    /// Colour space resource name or device family for shorthand operators.
    pub space: Option<String>,
    /// Component values last set by `SC`/`sc`/`SCN`/`scn` or the shorthand
    /// gray/RGB/CMYK operators.
    pub components: Vec<f64>,
    /// Pattern resource name for `SCN`/`scn` with a trailing name (§8.7.3).
    pub pattern: Option<String>,
}

impl Default for ColorValue {
    fn default() -> Self {
        // Initial text/printing colour is device gray 0.0 (Table 57).
        Self {
            space: Some("DeviceGray".into()),
            components: vec![0.0],
            pattern: None,
        }
    }
}

pub(super) fn set_device_color(
    color: &mut ColorValue,
    operator: &str,
    first: f64,
    space: &str,
) -> std::result::Result<(), ContentStateError> {
    require_finite(operator, &[first])?;
    if !(0.0..=1.0).contains(&first) {
        return Err(invalid(operator, "component must be between 0.0 and 1.0"));
    }
    color.space = Some(space.into());
    color.components = vec![first];
    color.pattern = None;
    Ok(())
}

pub(super) fn push_device_components(
    color: &mut ColorValue,
    operator: &str,
    values: &[f64],
) -> std::result::Result<(), ContentStateError> {
    for v in values {
        require_finite(operator, &[*v])?;
        if !(0.0..=1.0).contains(v) {
            return Err(invalid(operator, "component must be between 0.0 and 1.0"));
        }
        color.components.push(*v);
    }
    Ok(())
}
