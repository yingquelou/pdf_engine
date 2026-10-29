//! 旧式注解边框数组 `/Border`（Table 164）：三数 + 可选虚线数组。

use crate::core::types::{Array, Value};

/// 旧式 `/Border` 数组（Table 164）。
///
/// PDF 1.0 起为 `[水平圆角半径 垂直圆角半径 边框宽度]`，PDF 1.1 起可带
/// 第四项虚线数组。与 `/BS` 边框样式字典互斥：Table 164 规定注解字典
/// 同时带 `/BS` 时 `/Border` 被忽略，写端因此拒绝二者并存。
#[derive(Debug, Clone, PartialEq)]
pub struct BorderArray {
    horizontal_radius: f64,
    vertical_radius: f64,
    width: f64,
    dash: Vec<f64>,
}

impl BorderArray {
    /// 新建三数边框数组（PDF 1.0），单位为默认用户空间。
    pub fn new(horizontal_radius: f64, vertical_radius: f64, width: f64) -> Self {
        Self {
            horizontal_radius,
            vertical_radius,
            width,
            dash: Vec::new(),
        }
    }

    /// 虚线数组（第四项，PDF 1.1）；至少两项、有限且非负。
    pub fn with_dash(mut self, dash: Vec<f64>) -> Self {
        self.dash = dash;
        self
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        for (name, value) in [
            ("horizontal radius", self.horizontal_radius),
            ("vertical radius", self.vertical_radius),
            ("width", self.width),
        ] {
            if !value.is_finite() || value < 0.0 {
                return Err(format!(
                    "annotation /Border {name} must be a finite nonnegative number"
                ));
            }
        }
        if !self.dash.is_empty() {
            if self.dash.len() < 2 {
                return Err("annotation /Border dash pattern needs at least two entries".into());
            }
            if self
                .dash
                .iter()
                .any(|value| !value.is_finite() || *value < 0.0)
            {
                return Err("annotation /Border dash entries must be finite nonnegative".into());
            }
        }
        Ok(())
    }

    pub(crate) fn has_dash(&self) -> bool {
        !self.dash.is_empty()
    }

    pub(crate) fn to_array(&self) -> Array {
        let mut items = vec![
            Value::Float(self.horizontal_radius),
            Value::Float(self.vertical_radius),
            Value::Float(self.width),
        ];
        if !self.dash.is_empty() {
            items.push(Value::Array(Array(
                self.dash.iter().map(|value| Value::Float(*value)).collect(),
            )));
        }
        Array(items)
    }
}
