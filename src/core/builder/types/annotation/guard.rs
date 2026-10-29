//! 子类型归属守卫：把「某条目只许挂在某些子类型上」的报错文案集中一处。
//!
//! 由 [`PdfAnnotation::validate`] 的各子模块校验函数调用。

use super::{AnnotationKind, PdfAnnotation};

impl PdfAnnotation {
    /// 条目只许挂在 `expected` 子类型上。
    pub(super) fn require_kind(
        &self,
        expected: AnnotationKind,
        entries: &str,
    ) -> Result<(), String> {
        if self.kind == expected {
            return Ok(());
        }
        Err(format!(
            "{entries} are specific to {} annotations, got {}",
            expected.name(),
            self.kind.name()
        ))
    }

    /// 条目只许挂在 `expected` 列出的子类型之一上。
    pub(super) fn require_kind_in(
        &self,
        expected: &[AnnotationKind],
        entries: &str,
    ) -> Result<(), String> {
        if expected.contains(&self.kind) {
            return Ok(());
        }
        let names: Vec<&str> = expected.iter().map(|kind| kind.name()).collect();
        Err(format!(
            "{entries} are specific to {} annotations, got {}",
            names.join(" or "),
            self.kind.name()
        ))
    }
}
