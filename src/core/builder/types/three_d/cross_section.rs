//! 3D 截面字典（Table 311）的构建态。
//!
//! `/C` `/O` `/PO` `/PC` `/IV` `/IC` 仅在调用 `with_*` 后写出，缺省值
//! （`[0 0 0]`、`[null 0 0]`、0.5、白色、false、绿色）不物化。`/O` 必须恰有
//! 一个 `null`（`None` 槽位），`/PO` 须在 `0..=1`；`/IC` 仅在 `/IV true` 时
//! 有意义，否则写出前拒绝。

use crate::core::reader::three_d::ThreeDColor;
use crate::core::types::{Array, Dict, Value};

/// 3D 截面字典（Table 311）的构建态。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ThreeDCrossSection {
    center: Option<[f64; 3]>,
    orientation: Option<[Option<f64>; 3]>,
    plane_opacity: Option<f64>,
    plane_color: Option<ThreeDColor>,
    intersection_visible: Option<bool>,
    intersection_color: Option<ThreeDColor>,
}

impl ThreeDCrossSection {
    /// 新建空截面；所有条目缺省。
    pub fn new() -> Self {
        Self::default()
    }

    /// 截平面中心 `/C`；缺省 `[0 0 0]` 不写出。
    pub fn with_center(mut self, center: [f64; 3]) -> Self {
        self.center = Some(center);
        self
    }

    /// 平面方向 `/O`：恰有一个 `None`（写成 `null`），其余为角度。
    pub fn with_orientation(mut self, orientation: [Option<f64>; 3]) -> Self {
        self.orientation = Some(orientation);
        self
    }

    /// 平面不透明度 `/PO`（`0..=1`）；缺省 0.5 不写出。
    pub fn with_plane_opacity(mut self, opacity: f64) -> Self {
        self.plane_opacity = Some(opacity);
        self
    }

    /// 平面颜色 `/PC`；缺省白色不写出。
    pub fn with_plane_color(mut self, color: ThreeDColor) -> Self {
        self.plane_color = Some(color);
        self
    }

    /// 是否显示交线 `/IV`；缺省 false 不写出。
    pub fn with_intersection_visible(mut self, visible: bool) -> Self {
        self.intersection_visible = Some(visible);
        self
    }

    /// 交线颜色 `/IC`（仅 `/IV true` 时有意义）；缺省绿色不写出。
    pub fn with_intersection_color(mut self, color: ThreeDColor) -> Self {
        self.intersection_color = Some(color);
        self
    }

    /// 校验 Table 311 的取值依赖与数值范围。
    pub(crate) fn validate(&self) -> Result<(), String> {
        if let Some(center) = self.center
            && center.iter().any(|value| !value.is_finite())
        {
            return Err("3D cross section /C entries must be finite".into());
        }
        if let Some(orientation) = self.orientation {
            if orientation.iter().filter(|value| value.is_none()).count() != 1 {
                return Err(
                    "3D cross section /O must hold exactly one null entry (Table 311)".into(),
                );
            }
            if orientation.iter().flatten().any(|value| !value.is_finite()) {
                return Err("3D cross section /O entries must be finite".into());
            }
        }
        if let Some(opacity) = self.plane_opacity
            && (!opacity.is_finite() || !(0.0..=1.0).contains(&opacity))
        {
            return Err("3D cross section /PO must be within 0..=1".into());
        }
        if self.intersection_color.is_some() && self.intersection_visible != Some(true) {
            return Err(
                "3D cross section /IC is meaningful only if /IV is true (Table 311)".into(),
            );
        }
        Ok(())
    }

    /// 写成截面字典；缺省条目不物化。
    pub(crate) fn to_dict(&self) -> Dict {
        let mut dict = Dict::new().insert("Type", Value::Name("3DCrossSection".into()));
        if let Some(center) = self.center {
            let values = center.iter().map(|value| Value::Float(*value)).collect();
            dict = dict.insert("C", Value::Array(Array(values)));
        }
        if let Some(orientation) = self.orientation {
            let values = orientation
                .iter()
                .map(|value| match value {
                    Some(degrees) => Value::Float(*degrees),
                    None => Value::Null,
                })
                .collect();
            dict = dict.insert("O", Value::Array(Array(values)));
        }
        if let Some(opacity) = self.plane_opacity {
            dict = dict.insert("PO", Value::Float(opacity));
        }
        if let Some(color) = &self.plane_color {
            dict = dict.insert("PC", color.to_value());
        }
        if let Some(visible) = self.intersection_visible {
            dict = dict.insert("IV", Value::Bool(visible));
        }
        if let Some(color) = &self.intersection_color {
            dict = dict.insert("IC", color.to_value());
        }
        dict
    }
}
