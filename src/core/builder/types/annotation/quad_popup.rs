//! 区域条目的构造方法与读端：高亮四边形 `/QuadPoints`（Table 179）与弹出
//! 窗口互指句柄 `/Popup`、`/Parent`（12.5.2、Table 163/183）。

use super::PdfAnnotation;

impl PdfAnnotation {
    /// 高亮区域四边形 `/QuadPoints`；每四边形 8 个数，总长须为 8 的倍数。
    pub fn with_quad_points(mut self, quad_points: Vec<f64>) -> Self {
        self.quad_points = Some(quad_points);
        self
    }

    /// 按四边形分组的 `/QuadPoints`（每四边形 8 个数、逆时针四点，
    /// Table 179/173/192）；展平后写入，与非分组方法互斥（后调用者生效）。
    pub fn with_quad_point_groups(mut self, quads: Vec<[f64; 8]>) -> Self {
        self.quad_points = Some(quads.concat());
        self
    }

    /// 弹出窗口引用 `/Popup`；参数为另一注解的注册句柄，该注解须是
    /// `Popup` 子类型。只声明本端即可：弹出框的 `/Parent` 由写出阶段自动
    /// 补出（12.5.2、Table 163/183）。
    pub fn with_popup(mut self, handle: usize) -> Self {
        self.popup = Some(handle);
        self
    }

    /// Popup 父注解 `/Parent`（Table 183，仅 Popup 子类型）；参数为另一注解的注册句柄。
    /// 只声明本端即可：父注解的 `/Popup` 由写出阶段自动补出。
    pub fn with_popup_parent(mut self, handle: usize) -> Self {
        self.popup_parent = Some(handle);
        self
    }

    pub(crate) fn quad_points(&self) -> Option<&[f64]> {
        self.quad_points.as_deref()
    }

    pub(crate) fn popup(&self) -> Option<usize> {
        self.popup
    }

    pub(crate) fn popup_parent(&self) -> Option<usize> {
        self.popup_parent
    }
}
