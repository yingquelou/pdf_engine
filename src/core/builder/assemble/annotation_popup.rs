//! `/Popup` 与 `/Parent` 的成对（12.5.2、Table 163、Table 183）。
//!
//! 父注解的 `/Popup` 与弹出框的 `/Parent` 描述同一个关联，调用方只需声明
//! 一侧，另一侧由 [`pair_popups`] 在写出前补出；两端声明矛盾、`/Popup` 指向
//! 非 `Popup` 注解、以及最终没有 `/Parent` 的弹出框（Table 183 必填）都在
//! 字节写出前报 `WriteError::InvalidObject`。

use crate::core::builder::types::{AnnotationKind, PdfAnnotation};
use crate::core::writer::WriteError;

/// 成对结果：`parents[h]` 是应写入句柄 `h` 的 `/Parent` 的父注解句柄，
/// `children[h]` 是应写入句柄 `h` 的 `/Popup` 的弹出框句柄。
#[derive(Debug)]
pub(super) struct PopupPairs {
    parents: Vec<Option<usize>>,
    children: Vec<Option<usize>>,
}

impl PopupPairs {
    /// 由其他注解的 `/Popup` 反推出来的、句柄 `handle` 自己的 `/Parent`。
    pub(super) fn parent_of(&self, handle: usize) -> Option<usize> {
        self.parents.get(handle).copied().flatten()
    }

    /// 由弹出框的 `/Parent` 反推出来的、句柄 `handle` 的 `/Popup`。
    pub(super) fn popup_of(&self, handle: usize) -> Option<usize> {
        self.children.get(handle).copied().flatten()
    }
}

fn invalid(message: String) -> WriteError {
    WriteError::InvalidObject(message)
}

/// 扫描全部注解，按单侧声明补出另一端并检出矛盾配对。
///
/// 句柄越界不在此报错：写出阶段对显式声明的 `/Popup`/`/Parent` 已有统一的
/// `unknown annotation handle` 消息，这里只跳过补出。
pub(super) fn pair_popups(annotations: &[&PdfAnnotation]) -> Result<PopupPairs, WriteError> {
    let mut pairs = PopupPairs {
        parents: vec![None; annotations.len()],
        children: vec![None; annotations.len()],
    };
    for (handle, annot) in annotations.iter().enumerate() {
        // 父声明 `/Popup` → 补出弹出框的 `/Parent`。
        if let Some(popup) = annot.popup() {
            if popup == handle {
                return Err(invalid(format!(
                    "annotation handle {handle}: /Popup must not reference the annotation itself"
                )));
            }
            if let Some(child) = annotations.get(popup) {
                if child.kind() != AnnotationKind::Popup {
                    return Err(invalid(format!(
                        "annotation handle {handle}: /Popup must reference a Popup annotation, handle {popup} is {}",
                        child.kind().name()
                    )));
                }
                match child.popup_parent() {
                    Some(parent) if parent != handle => {
                        return Err(invalid(format!(
                            "annotation handle {handle}: /Popup references popup {popup} whose /Parent points at annotation {parent}"
                        )));
                    }
                    Some(_) => {}
                    None => {
                        if let Some(existing) = pairs.parents[popup] {
                            return Err(invalid(format!(
                                "popup annotation handle {popup} is referenced by /Popup on both annotation {existing} and annotation {handle}"
                            )));
                        }
                        pairs.parents[popup] = Some(handle);
                    }
                }
            }
        }
        // 弹出框声明 `/Parent` → 补出父注解的 `/Popup`。
        if let Some(parent) = annot.popup_parent()
            && parent != handle
            && let Some(owner) = annotations.get(parent)
        {
            match owner.popup() {
                Some(popup) if popup != handle => {
                    return Err(invalid(format!(
                        "popup annotation handle {handle}: /Parent references annotation {parent} whose /Popup points at popup {popup}"
                    )));
                }
                Some(_) => {}
                None => {
                    if let Some(existing) = pairs.children[parent] {
                        return Err(invalid(format!(
                            "annotation handle {parent} is named /Parent by both popup {existing} and popup {handle}"
                        )));
                    }
                    pairs.children[parent] = Some(handle);
                }
            }
        }
    }
    for (handle, annot) in annotations.iter().enumerate() {
        if annot.kind() == AnnotationKind::Popup
            && annot.popup_parent().is_none()
            && pairs.parent_of(handle).is_none()
        {
            return Err(invalid(format!(
                "popup annotation handle {handle} requires /Parent (Table 183)"
            )));
        }
    }
    Ok(pairs)
}
