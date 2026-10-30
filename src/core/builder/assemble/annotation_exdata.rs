//! 注解 `/ExData` 3D 标记（13.6.6、Table 313）的跨注解校验。
//!
//! `/3DA` 指向另一条 3D 注解：注解对象号在写出阶段才分配，因此句柄范围、
//! 目标子类型与自引用都在对象号分配与任何字节写出之前统一校验；失败一律
//! 报 `WriteError::InvalidObject`（零字节契约）。

use crate::core::builder::types::{AnnotationKind, PdfAnnotation};
use crate::core::writer::WriteError;

/// 校验全部注解的 `/ExData` `/3DA` 目标。
///
/// 错误消息带注解句柄与 PDF 键，便于调用方定位；只校验声明了 `/ExData`
/// 的注解，未声明者不受影响。
pub(super) fn validate_markup_3d(annotations: &[&PdfAnnotation]) -> Result<(), WriteError> {
    for (handle, annot) in annotations.iter().enumerate() {
        let Some(markup) = annot.ex_data() else {
            continue;
        };
        let target = markup.annotation();
        if target == handle {
            return Err(WriteError::InvalidObject(format!(
                "annotation handle {handle}: /ExData /3DA must not reference the annotation itself"
            )));
        }
        let Some(target_annot) = annotations.get(target) else {
            return Err(WriteError::InvalidObject(format!(
                "annotation handle {handle}: /ExData /3DA references unknown annotation handle {target}"
            )));
        };
        // Table 313 规定 `/3DA` 指向 3D 注解（`/Subtype /3D`）。
        if target_annot.kind() != AnnotationKind::ThreeD {
            return Err(WriteError::InvalidObject(format!(
                "annotation handle {handle}: /ExData /3DA must reference a 3D annotation, handle {target} is {}",
                target_annot.kind().name()
            )));
        }
    }
    Ok(())
}
