//! 颜色与透明度条目（Table 165/166）的构造方法与读端：`/C` 边框/背景颜色、
//! `/IC` 填充颜色、`/CA` 不透明度与 `/BM` 混合模式。

use super::PdfAnnotation;

impl PdfAnnotation {
    /// 边框/背景颜色 `/C`；分量数限 1/3/4，各分量须为有限数。
    pub fn with_color(mut self, color: Vec<f64>) -> Self {
        self.color = Some(color);
        self
    }

    /// 填充颜色 `/IC`（仅对几何与标记类注解有意义）。
    pub fn with_interior_color(mut self, color: Vec<f64>) -> Self {
        self.interior_color = Some(color);
        self
    }

    /// 注解不透明度 `/CA`，取值 `0.0..=1.0`（PDF 1.4）。
    pub fn with_opacity(mut self, opacity: f64) -> Self {
        self.opacity = Some(opacity);
        self
    }

    /// 混合模式 `/BM`（PDF 1.4 名称，如 `Multiply`）。
    pub fn with_blend_mode(mut self, mode: impl Into<String>) -> Self {
        self.blend_mode = Some(mode.into());
        self
    }

    pub(crate) fn color(&self) -> Option<&[f64]> {
        self.color.as_deref()
    }

    pub(crate) fn interior_color(&self) -> Option<&[f64]> {
        self.interior_color.as_deref()
    }

    pub(crate) fn opacity(&self) -> Option<f64> {
        self.opacity
    }

    pub(crate) fn blend_mode(&self) -> Option<&str> {
        self.blend_mode.as_deref()
    }
}
