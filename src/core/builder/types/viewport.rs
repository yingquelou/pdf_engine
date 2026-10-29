//! 页面视口字典（12.9.2，Table 260）的构建态与校验。
//!
//! 视口把页面上一块矩形区域（`/BBox`）与测量字典（`/Measure`，Table 261–263）
//! 关联起来，经 [`PdfBuilder::set_page_viewports`] 写为页面字典的 `/VP` 数组
//! （Table 30，PDF 1.6 起）。
//!
//! [`PdfBuilder::set_page_viewports`]: crate::core::builder::PdfBuilder::set_page_viewports

use crate::core::types::{Array, Dict, Value};

use super::{Measure, text_string_value};

/// 页面视口字典（Table 260）。
#[derive(Debug, Clone, PartialEq)]
pub struct Viewport {
    bbox: [f64; 4],
    name: Option<String>,
    measure: Option<Measure>,
}

impl Viewport {
    /// 新建视口；`bbox` 为 `[x0, y0, x1, y1]`，须按左下、右上规范化。
    pub fn new(bbox: [f64; 4]) -> Self {
        Self {
            bbox,
            name: None,
            measure: None,
        }
    }

    /// 描述名 `/Name`（非空文本串）。
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// 测量字典 `/Measure`（Table 261，直角坐标测量）。
    pub fn with_measure(mut self, measure: Measure) -> Self {
        self.measure = Some(measure);
        self
    }

    /// 校验视口条目；非法时返回可读原因。
    pub(crate) fn validate(&self) -> Result<(), String> {
        if !self.bbox.iter().all(|value| value.is_finite()) {
            return Err("viewport /BBox must hold finite numbers".into());
        }
        if self.bbox[0] > self.bbox[2] || self.bbox[1] > self.bbox[3] {
            return Err("viewport /BBox must be normalized (lower-left then upper-right)".into());
        }
        if let Some(name) = &self.name
            && name.is_empty()
        {
            return Err("viewport /Name must not be empty".into());
        }
        if let Some(measure) = &self.measure {
            measure.validate()?;
        }
        Ok(())
    }

    /// 写出 Table 260 字典；`/Type` 规范为可选，此处总是写出。
    pub(crate) fn to_dict(&self) -> Dict {
        let mut dict = Dict::new()
            .insert("Type", Value::Name("Viewport".into()))
            .insert(
                "BBox",
                Value::Array(Array(
                    self.bbox.iter().map(|value| Value::Float(*value)).collect(),
                )),
            );
        if let Some(name) = &self.name {
            dict = dict.insert("Name", text_string_value(name));
        }
        if let Some(measure) = &self.measure {
            dict = dict.insert("Measure", Value::Dict(measure.to_dict()));
        }
        dict
    }
}
