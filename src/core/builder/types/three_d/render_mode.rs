//! 3D 渲染模式字典（Table 307/308）的构建态。
//!
//! `/Type` 恒为 `3DRenderMode`，`/Subtype` 由
//! [`ThreeDRenderMode::new`] 显式声明；`/AC` `/FC` `/O` `/CV` 仅在调用
//! `with_*` 后写出，缺省值（`[/DeviceRGB 0 0 0]`、`BG`、0.5、45）不物化。
//! `/O` 须在 `0..=1`；`/FC` 仅对 `Illustration` 有意义（Table 307）。

use super::super::validate_name_value;
use crate::core::reader::three_d::{ThreeDColor, ThreeDFaceColor, ThreeDRenderModeType};
use crate::core::types::{Dict, Value};

/// 3D 渲染模式字典（Table 307）的构建态。
#[derive(Debug, Clone, PartialEq)]
pub struct ThreeDRenderMode {
    subtype: ThreeDRenderModeType,
    auxiliary_color: Option<ThreeDColor>,
    face_color: Option<ThreeDFaceColor>,
    opacity: Option<f64>,
    crease_value: Option<f64>,
}

impl ThreeDRenderMode {
    /// 新建渲染模式字典；`subtype` 为 `/Subtype`（Table 308）。
    pub fn new(subtype: ThreeDRenderModeType) -> Self {
        Self {
            subtype,
            auxiliary_color: None,
            face_color: None,
            opacity: None,
            crease_value: None,
        }
    }

    /// 线框与顶点颜色 `/AC`；缺省黑色不写出。
    pub fn with_auxiliary_color(mut self, color: ThreeDColor) -> Self {
        self.auxiliary_color = Some(color);
        self
    }

    /// 面颜色 `/FC`（仅 `Illustration` 有意义）；缺省 `BG` 不写出。
    pub fn with_face_color(mut self, color: ThreeDFaceColor) -> Self {
        self.face_color = Some(color);
        self
    }

    /// 不透明度 `/O`（`0..=1`）；缺省 0.5 不写出。
    pub fn with_opacity(mut self, opacity: f64) -> Self {
        self.opacity = Some(opacity);
        self
    }

    /// 折痕角 `/CV`（度）；缺省 45 不写出。
    pub fn with_crease_value(mut self, degrees: f64) -> Self {
        self.crease_value = Some(degrees);
        self
    }

    /// 校验 `/Subtype` 名字语法、`/O` `/CV` 取值与 `/FC` 的适用性。
    pub(crate) fn validate(&self) -> Result<(), String> {
        if let ThreeDRenderModeType::Other(name) = &self.subtype {
            validate_name_value(name, "3D render mode /Subtype")?;
        }
        validate_color(self.auxiliary_color.as_ref(), "/AC")?;
        if let Some(color) = &self.face_color {
            if self.subtype != ThreeDRenderModeType::Illustration {
                return Err(
                    "3D render mode /FC is meaningful only for Illustration (Table 307)".into(),
                );
            }
            if let ThreeDFaceColor::DeviceRgb(values) = color
                && values.iter().any(|value| !value.is_finite())
            {
                return Err("3D render mode /FC components must be finite".into());
            }
        }
        if let Some(opacity) = self.opacity {
            if !opacity.is_finite() {
                return Err("3D render mode /O must be finite".into());
            }
            if !(0.0..=1.0).contains(&opacity) {
                return Err("3D render mode /O must be within 0..=1".into());
            }
        }
        if let Some(degrees) = self.crease_value
            && !degrees.is_finite()
        {
            return Err("3D render mode /CV must be finite".into());
        }
        Ok(())
    }

    /// 写成渲染模式字典；缺省条目不物化。
    pub(crate) fn to_dict(&self) -> Dict {
        let mut dict = Dict::new()
            .insert("Type", Value::Name("3DRenderMode".into()))
            .insert("Subtype", Value::Name(self.subtype.as_name().to_string()));
        if let Some(color) = &self.auxiliary_color {
            dict = dict.insert("AC", color.to_value());
        }
        if let Some(color) = &self.face_color {
            dict = dict.insert("FC", color.to_value());
        }
        if let Some(opacity) = self.opacity {
            dict = dict.insert("O", Value::Float(opacity));
        }
        if let Some(degrees) = self.crease_value {
            dict = dict.insert("CV", Value::Float(degrees));
        }
        dict
    }
}

/// `/AC`：DeviceRGB 分量须有限。
fn validate_color(color: Option<&ThreeDColor>, key: &str) -> Result<(), String> {
    if let Some(ThreeDColor::DeviceRgb(values)) = color
        && values.iter().any(|value| !value.is_finite())
    {
        return Err(format!("3D render mode {key} components must be finite"));
    }
    Ok(())
}
