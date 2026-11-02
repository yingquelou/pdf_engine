//! PatternType 2 着色图案字典：以渐变填充图案空间。

use super::dict::Shading;
use super::validate::ensure_finite;

/// PatternType 2 着色图案：以渐变填充图案空间。
#[derive(Debug, Clone, PartialEq)]
pub struct ShadingPattern {
    pub shading: Shading,
    /// 从图案空间到默认坐标系的变换矩阵。
    pub matrix: Option<[f64; 6]>,
}

impl ShadingPattern {
    pub fn new(shading: Shading) -> Self {
        Self {
            shading,
            matrix: None,
        }
    }

    pub fn with_matrix(mut self, matrix: [f64; 6]) -> Self {
        self.matrix = Some(matrix);
        self
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        if let Some(matrix) = self.matrix {
            ensure_finite(&matrix, "shading pattern Matrix")?;
        }
        self.shading.validate()
    }
}
