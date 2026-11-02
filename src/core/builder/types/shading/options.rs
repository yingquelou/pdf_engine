//! 所有渐变字典共有的可选条目建模（ISO 32000-1 Table 78）与公共校验。

use crate::core::builder::types::ColorSpace;

use super::validate::ensure_finite;

/// 所有渐变字典共有的可选条目（ISO 32000-1 Table 78）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ShadingOptions {
    /// `/Background`：渐变绘制前的背景色，分量数须与颜色空间一致。
    pub background: Option<Vec<f64>>,
    /// `/BBox`：`[left bottom right top]`，限制渐变的作用区域。
    pub bbox: Option<[f64; 4]>,
    /// `/AntiAlias`：是否抗锯齿；省略时由查看器决定。
    pub anti_alias: Option<bool>,
}

impl ShadingOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_background(mut self, background: impl Into<Vec<f64>>) -> Self {
        self.background = Some(background.into());
        self
    }

    pub fn with_bbox(mut self, bbox: [f64; 4]) -> Self {
        self.bbox = Some(bbox);
        self
    }

    pub fn with_anti_alias(mut self, anti_alias: bool) -> Self {
        self.anti_alias = Some(anti_alias);
        self
    }

    pub(crate) fn validate(&self, color_space: &ColorSpace) -> Result<(), String> {
        if let Some(background) = &self.background {
            ensure_finite(background, "shading Background")?;
            let channels = color_space.channels();
            if background.len() != channels {
                return Err(format!(
                    "shading Background must have {channels} component(s), got {}",
                    background.len()
                ));
            }
        }
        if let Some(bbox) = self.bbox {
            ensure_finite(&bbox, "shading BBox")?;
            if bbox[0] >= bbox[2] || bbox[1] >= bbox[3] {
                return Err("shading BBox minimum must be less than maximum".into());
            }
        }
        Ok(())
    }
}
