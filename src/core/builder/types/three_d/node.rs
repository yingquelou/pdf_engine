//! 3D 节点字典（Table 312）的构建态。
//!
//! `/N` 必填节点名（非空），`/O` `/V` `/M` 仅在调用 `with_*` 后写出；
//! 不透明度须在 `0..=1`，矩阵的 12 个分量须有限。

use crate::core::builder::types::text_string_value;
use crate::core::types::{Array, Dict, Value};

/// 3D 节点字典（Table 312）的构建态。
#[derive(Debug, Clone, PartialEq)]
pub struct ThreeDNode {
    name: String,
    opacity: Option<f64>,
    visible: Option<bool>,
    matrix: Option<[f64; 12]>,
}

impl ThreeDNode {
    /// 新建节点；`name` 为必填节点名 `/N`。
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            opacity: None,
            visible: None,
            matrix: None,
        }
    }

    /// 不透明度 `/O`（`0..=1`）；缺省表示继承。
    pub fn with_opacity(mut self, opacity: f64) -> Self {
        self.opacity = Some(opacity);
        self
    }

    /// 可见性 `/V`；缺省表示继承。
    pub fn with_visible(mut self, visible: bool) -> Self {
        self.visible = Some(visible);
        self
    }

    /// 3D 矩阵 `/M`（12 个有限数）。
    pub fn with_matrix(mut self, matrix: [f64; 12]) -> Self {
        self.matrix = Some(matrix);
        self
    }

    /// 校验 Table 312 的必填项与数值范围。
    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.name.is_empty() {
            return Err("3D node /N must not be empty".into());
        }
        if let Some(opacity) = self.opacity
            && (!opacity.is_finite() || !(0.0..=1.0).contains(&opacity))
        {
            return Err("3D node /O must be within 0..=1".into());
        }
        if let Some(matrix) = self.matrix
            && matrix.iter().any(|value| !value.is_finite())
        {
            return Err("3D node /M entries must be finite".into());
        }
        Ok(())
    }

    /// 写成节点字典；缺省条目不物化。
    pub(crate) fn to_dict(&self) -> Dict {
        let mut dict = Dict::new()
            .insert("Type", Value::Name("3DNode".into()))
            .insert("N", text_string_value(&self.name));
        if let Some(opacity) = self.opacity {
            dict = dict.insert("O", Value::Float(opacity));
        }
        if let Some(visible) = self.visible {
            dict = dict.insert("V", Value::Bool(visible));
        }
        if let Some(matrix) = self.matrix {
            let values = matrix.iter().map(|value| Value::Float(*value)).collect();
            dict = dict.insert("M", Value::Array(Array(values)));
        }
        dict
    }
}
