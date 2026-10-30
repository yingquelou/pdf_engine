//! Watermark 注解的固定打印字典 `/FixedPrint`（12.5.6.22，Table 190/191）。
//!
//! Table 190 只给 Watermark 声明一项专有条目 `/FixedPrint`，其内容按 Table 191
//! 写成直接字典 `<< /Type /FixedPrint /Matrix [...] /H ... /V ... >>`；
//! 规范缺省（单位矩阵、`/H` `/V` 为 0）不物化，由渲染端按表补。
//! `/Matrix` 与两个平移量都只要求有限数值：Table 191 对负平移用的是
//! should not（建议），构建期不作拒绝。

use super::{AnnotationKind, PdfAnnotation};

/// 固定打印字典（Table 191）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FixedPrint {
    matrix: Option<[f64; 6]>,
    horizontal: Option<f64>,
    vertical: Option<f64>,
}

impl FixedPrint {
    /// 新建空字典：全部条目取 Table 191 的缺省值。
    pub fn new() -> Self {
        Self::default()
    }

    /// `/Matrix` 渲染前预变换注解矩形的矩阵。
    pub fn with_matrix(mut self, matrix: [f64; 6]) -> Self {
        self.matrix = Some(matrix);
        self
    }

    /// `/H` 水平平移量，按目标介质宽度的百分比（1.0 = 100%）。
    pub fn with_horizontal_offset(mut self, fraction: f64) -> Self {
        self.horizontal = Some(fraction);
        self
    }

    /// `/V` 垂直平移量，按目标介质高度的百分比（1.0 = 100%）。
    pub fn with_vertical_offset(mut self, fraction: f64) -> Self {
        self.vertical = Some(fraction);
        self
    }

    /// `/Matrix`。
    pub(crate) fn matrix(&self) -> Option<[f64; 6]> {
        self.matrix
    }

    /// `/H`。
    pub(crate) fn horizontal(&self) -> Option<f64> {
        self.horizontal
    }

    /// `/V`。
    pub(crate) fn vertical(&self) -> Option<f64> {
        self.vertical
    }

    /// 按 Table 191 校验取值。
    fn validate(&self) -> Result<(), String> {
        if let Some(matrix) = self.matrix
            && matrix.iter().any(|value| !value.is_finite())
        {
            return Err("fixed print /Matrix entries must be finite".into());
        }
        for (key, value) in [("/H", self.horizontal), ("/V", self.vertical)] {
            if let Some(fraction) = value
                && !fraction.is_finite()
            {
                return Err(format!("fixed print {key} must be a finite number"));
            }
        }
        Ok(())
    }
}

impl PdfAnnotation {
    /// 固定打印字典 `/FixedPrint`（Table 190；仅 Watermark 注解）。
    pub fn with_fixed_print(mut self, fixed_print: FixedPrint) -> Self {
        self.fixed_print = Some(fixed_print);
        self
    }
}

impl PdfAnnotation {
    /// 已声明的固定打印字典（Table 190）。
    pub(crate) fn fixed_print(&self) -> Option<&FixedPrint> {
        self.fixed_print.as_ref()
    }

    /// 固定打印字典的子类型归属与取值校验；由 [`PdfAnnotation::validate`] 调用。
    pub(super) fn validate_fixed_print(&self) -> Result<(), String> {
        let Some(fixed_print) = &self.fixed_print else {
            return Ok(());
        };
        self.require_kind(
            AnnotationKind::Watermark,
            "Watermark annotation /FixedPrint",
        )?;
        fixed_print.validate()
    }
}
