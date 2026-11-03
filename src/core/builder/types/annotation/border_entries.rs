//! 边框条目的构造方法与读端：`/BS` 边框样式字典（Table 162 的 `/W` 宽度、
//! `/S` 样式、`/D` 虚线模式）与旧式 `/Border` 数组（Table 164）；二者互斥的
//! 归属校验见 `validation`。

use super::{AnnotationBorderStyle, BorderArray, PdfAnnotation};

impl PdfAnnotation {
    /// 边框宽度（`/BS` 字典 `/W`，非负）。
    pub fn with_border_width(mut self, width: f64) -> Self {
        self.border_width = Some(width);
        self
    }

    /// 边框样式（`/BS` 字典 `/S`）。
    pub fn with_border_style(mut self, style: AnnotationBorderStyle) -> Self {
        self.border_style = Some(style);
        self
    }

    /// 虚线模式（`/BS` 字典 `/D`；至少两项）。
    pub fn with_border_dash(mut self, dash: Vec<f64>) -> Self {
        self.border_dash = dash;
        self
    }

    /// 旧式边框数组（`/Border`，Table 164）；与 [`Self::with_border_width`]、
    /// [`Self::with_border_style`]、[`Self::with_border_dash`] 互斥。
    pub fn with_border_array(mut self, border: BorderArray) -> Self {
        self.border_array = Some(border);
        self
    }

    pub(crate) fn border_width(&self) -> Option<f64> {
        self.border_width
    }

    pub(crate) fn border_style(&self) -> Option<AnnotationBorderStyle> {
        self.border_style
    }

    pub(crate) fn border_dash(&self) -> &[f64] {
        &self.border_dash
    }

    pub(crate) fn border_array(&self) -> Option<&BorderArray> {
        self.border_array.as_ref()
    }
}
