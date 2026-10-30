//! 3D 注解专有条目（12.5.6.19 Table 298）的构建态与构建期校验。
//!
//! `/3DD` 是 3D artwork 流的间接引用（3D 注解必填），句柄来自
//! [`PdfBuilder::add_three_d_artwork`](crate::core::builder::PdfBuilder::add_three_d_artwork)；
//! `/3DV` 选择初始视图，`/3DA` 为激活字典，`/3DI` 控制交互，`/3DB` 为
//! 视图框。视图索引/名称相对注册 artwork 的 `/VA` 校验，留给 assemble
//! 阶段（此处只有句柄，没有视图表）。

use super::{AnnotationKind, PdfAnnotation};
use crate::core::builder::types::{ThreeDActivation, ThreeDViewSelection};

impl PdfAnnotation {
    /// 3D artwork 引用 `/3DD`（Table 298，3D 注解必填）；参数为
    /// [`PdfBuilder::add_three_d_artwork`](crate::core::builder::PdfBuilder::add_three_d_artwork)
    /// 返回的句柄。
    pub fn with_three_d(mut self, handle: usize) -> Self {
        self.three_d = Some(handle);
        self
    }

    /// 初始视图 `/3DV`（内联视图、`/VA` 下标、`/IN` 名称或 `F`/`L`）。
    pub fn with_three_d_view(mut self, view: ThreeDViewSelection) -> Self {
        self.three_d_view = Some(view);
        self
    }

    /// 激活字典 `/3DA`（Table 299）；缺省时省略。
    pub fn with_three_d_activation(mut self, activation: ThreeDActivation) -> Self {
        self.three_d_activation = Some(activation);
        self
    }

    /// 是否供交互使用 `/3DI`（缺省 true 由呈现端应用）。
    pub fn with_three_d_interactive(mut self, interactive: bool) -> Self {
        self.three_d_interactive = Some(interactive);
        self
    }

    /// 视图框 `/3DB`（注解目标坐标系，四个有限数）；缺省时由呈现端
    /// 按 `/Rect` 计算。以 `Vec<f64>` 接收以便在构建期校验项数。
    pub fn with_three_d_view_box(mut self, view_box: impl Into<Vec<f64>>) -> Self {
        self.three_d_view_box = Some(view_box.into());
        self
    }

    /// 校验 3D 条目的子类型归属、`/3DD` 必填性与内联视图/激活字典/
    /// 视图框的取值；选择目标相对 `/VA` 的校验在 assemble 阶段完成。
    pub(super) fn validate_three_d_entries(&self) -> Result<(), String> {
        let has_entries = self.three_d.is_some()
            || self.three_d_view.is_some()
            || self.three_d_activation.is_some()
            || self.three_d_interactive.is_some()
            || self.three_d_view_box.is_some();
        if self.kind == AnnotationKind::ThreeD {
            if self.three_d.is_none() {
                return Err("3D annotation requires /3DD (3D artwork handle)".into());
            }
        } else if has_entries {
            return Err(format!(
                "annotation /3DD and related entries are specific to 3D annotations, got {}",
                self.kind.name()
            ));
        }
        if let Some(view_box) = &self.three_d_view_box {
            if view_box.len() != 4 {
                return Err(format!(
                    "annotation /3DB must hold exactly four numbers, got {}",
                    view_box.len()
                ));
            }
            if view_box.iter().any(|value| !value.is_finite()) {
                return Err("annotation /3DB entries must be finite".into());
            }
        }
        if let Some(ThreeDViewSelection::Inline(view)) = &self.three_d_view {
            view.validate()
                .map_err(|message| format!("annotation /3DV: {message}"))?;
        }
        if let Some(activation) = &self.three_d_activation {
            activation
                .validate()
                .map_err(|message| format!("annotation /3DA: {message}"))?;
        }
        Ok(())
    }
}
