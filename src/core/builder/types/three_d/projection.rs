//! 3D 投影字典（Table 305）。
//!
//! `/Subtype` 必填；透视投影要求正数 `/N` 与 `0..=180` 的 `/FOV`，正交
//! 投影的 `/N` 可为 0 且 `/OS` 须为正数。`/CS` `/PS` `/OS` `/OB` 的缺省
//! 值（`ANF`、`W`、`1`、`Absolute`）不物化；对投影类型无意义的条目在
//! 写出前拒绝，避免读端按规范丢弃后无法往返。

use super::super::validate_name_value;
use crate::core::reader::three_d::{
    ThreeDClipping, ThreeDProjectionBinding, ThreeDProjectionScale, ThreeDProjectionType,
};
use crate::core::types::{Dict, Value};

/// 3D 投影字典（Table 305）的构建态。
#[derive(Debug, Clone, PartialEq)]
pub struct ThreeDProjection {
    subtype: ThreeDProjectionType,
    clipping: Option<ThreeDClipping>,
    far: Option<f64>,
    near: Option<f64>,
    field_of_view: Option<f64>,
    scale: Option<ThreeDProjectionScale>,
    orthographic_scale: Option<f64>,
    binding: Option<ThreeDProjectionBinding>,
}

impl ThreeDProjection {
    /// 新建投影字典；`subtype` 为 `/O`（正交）或 `/P`（透视）。
    pub fn new(subtype: ThreeDProjectionType) -> Self {
        Self {
            subtype,
            clipping: None,
            far: None,
            near: None,
            field_of_view: None,
            scale: None,
            orthographic_scale: None,
            binding: None,
        }
    }

    /// 裁剪方式 `/CS`；缺省 `ANF` 不写出。
    pub fn with_clipping(mut self, clipping: ThreeDClipping) -> Self {
        self.clipping = Some(clipping);
        self
    }

    /// 远裁剪距离 `/F`（仅 `/CS /XNF` 时有意义）。
    pub fn with_far(mut self, far: f64) -> Self {
        self.far = Some(far);
        self
    }

    /// 近裁剪距离 `/N`（透视投影必填；正交投影缺省 0）。
    pub fn with_near(mut self, near: f64) -> Self {
        self.near = Some(near);
        self
    }

    /// 视场角 `/FOV`（度，透视投影必填）。
    pub fn with_field_of_view(mut self, degrees: f64) -> Self {
        self.field_of_view = Some(degrees);
        self
    }

    /// 透视缩放 `/PS`；缺省 `W` 不写出。
    pub fn with_scale(mut self, scale: ThreeDProjectionScale) -> Self {
        self.scale = Some(scale);
        self
    }

    /// 正交缩放因子 `/OS`；缺省 1 不写出。
    pub fn with_orthographic_scale(mut self, scale: f64) -> Self {
        self.orthographic_scale = Some(scale);
        self
    }

    /// 正交绑定策略 `/OB`；缺省 `Absolute` 不写出。
    pub fn with_binding(mut self, binding: ThreeDProjectionBinding) -> Self {
        self.binding = Some(binding);
        self
    }

    /// 校验 Table 305 的必填项与取值依赖。
    pub(crate) fn validate(&self) -> Result<(), String> {
        if let Some(far) = self.far
            && !far.is_finite()
        {
            return Err("/F (far clipping distance) must be finite".into());
        }
        if let Some(near) = self.near
            && !near.is_finite()
        {
            return Err("/N (near clipping distance) must be finite".into());
        }
        if let Some(degrees) = self.field_of_view
            && (!degrees.is_finite() || !(0.0..=180.0).contains(&degrees))
        {
            return Err("/FOV must be within 0..=180".into());
        }
        if let Some(scale) = &self.scale {
            match scale {
                ThreeDProjectionScale::Diameter(value) if !value.is_finite() || *value <= 0.0 => {
                    return Err("/PS diameter must be a positive finite number".into());
                }
                ThreeDProjectionScale::Diameter(_) => {}
                ThreeDProjectionScale::Other(name) => {
                    validate_name_value(name, "projection /PS")?;
                }
                _ => {}
            }
        }
        if let Some(scale) = self.orthographic_scale
            && (!scale.is_finite() || scale <= 0.0)
        {
            return Err("/OS must be a positive finite number".into());
        }
        if let Some(ThreeDProjectionBinding::Other(name)) = &self.binding {
            validate_name_value(name, "projection /OB")?;
        }
        match self.subtype {
            ThreeDProjectionType::Perspective => {
                if self.near.is_none() {
                    return Err("perspective projection requires /N".into());
                }
                if self.field_of_view.is_none() {
                    return Err("perspective projection requires /FOV".into());
                }
                if self.near.is_some_and(|near| near <= 0.0) {
                    return Err("perspective /N must be positive".into());
                }
                if self.orthographic_scale.is_some() || self.binding.is_some() {
                    return Err(
                        "/OS and /OB are meaningful only for orthographic projection".into(),
                    );
                }
            }
            ThreeDProjectionType::Orthographic => {
                if self.near.is_some_and(|near| near < 0.0) {
                    return Err("orthographic /N must not be negative".into());
                }
                if self.field_of_view.is_some() || self.scale.is_some() {
                    return Err(
                        "/FOV and /PS are meaningful only for perspective projection".into(),
                    );
                }
            }
        }
        Ok(())
    }

    /// 写成投影字典；缺省条目不物化。
    pub(crate) fn to_dict(&self) -> Dict {
        let mut dict = Dict::new().insert(
            "Subtype",
            Value::Name(
                match self.subtype {
                    ThreeDProjectionType::Orthographic => "O",
                    ThreeDProjectionType::Perspective => "P",
                }
                .into(),
            ),
        );
        if let Some(clipping) = &self.clipping {
            dict = dict.insert(
                "CS",
                Value::Name(
                    match clipping {
                        ThreeDClipping::Explicit => "XNF",
                        ThreeDClipping::Automatic => "ANF",
                    }
                    .into(),
                ),
            );
        }
        if let Some(far) = self.far {
            dict = dict.insert("F", Value::Float(far));
        }
        if let Some(near) = self.near {
            dict = dict.insert("N", Value::Float(near));
        }
        if let Some(degrees) = self.field_of_view {
            dict = dict.insert("FOV", Value::Float(degrees));
        }
        if let Some(scale) = &self.scale {
            let value = match scale {
                ThreeDProjectionScale::Diameter(diameter) => Value::Float(*diameter),
                ThreeDProjectionScale::Width => Value::Name("W".into()),
                ThreeDProjectionScale::Height => Value::Name("H".into()),
                ThreeDProjectionScale::Min => Value::Name("Min".into()),
                ThreeDProjectionScale::Max => Value::Name("Max".into()),
                ThreeDProjectionScale::Other(name) => Value::Name(name.clone()),
            };
            dict = dict.insert("PS", value);
        }
        if let Some(scale) = self.orthographic_scale {
            dict = dict.insert("OS", Value::Float(scale));
        }
        if let Some(binding) = &self.binding {
            let value = match binding {
                ThreeDProjectionBinding::Width => Value::Name("W".into()),
                ThreeDProjectionBinding::Height => Value::Name("H".into()),
                ThreeDProjectionBinding::Min => Value::Name("Min".into()),
                ThreeDProjectionBinding::Max => Value::Name("Max".into()),
                ThreeDProjectionBinding::Absolute => Value::Name("Absolute".into()),
                ThreeDProjectionBinding::Other(name) => Value::Name(name.clone()),
            };
            dict = dict.insert("OB", value);
        }
        dict
    }
}
