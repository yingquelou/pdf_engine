//! Printer's mark 注解写端条目（14.11.3，Table 362/363）。
//!
//! Table 362 的专有条目只有 `/MN`（name，如 `ColorBar`、
//! `RegistrationTarget`；注意不是通用注解的 `/NM` 文本串）。14.11.3
//! 正文要求 `/AP` 与 `/F` 必须存在：`/F` 的 Print（4）与 ReadOnly（64）
//! 必须置位、其余位清零，写出恒为 68；`/AP /N` 必须有正常外观，
//! 多于一个外观流时必须声明 `/AS`。
//!
//! 表单级 `/MarkStyle` `/Colorants`（Table 363）由
//! [`FormXObject`](super::super::FormXObject) 承载，本模块只校验注解侧
//! 条目与归属；全部校验发生在对象号分配与字节写出之前。

use super::{AnnotationKind, PdfAnnotation, validate_name_value};

impl PdfAnnotation {
    /// Printer's mark 标记名称 `/MN`（Table 362；name，非文本串）。
    ///
    /// 仅 PrinterMark 子类型接受；`/AP` 与 `/F` 的契约由构建期校验。
    pub fn with_printer_mark_name(mut self, name: impl Into<String>) -> Self {
        self.printer_mark = Some(PrinterMarkEntries::default().with_mark_name(name));
        self
    }

    /// 实际写出的 `/F`：Printer's mark 与 Trap 网络恒为 68
    /// （Print | ReadOnly，14.11.3 / 14.11.6）。
    pub(crate) fn written_flags(&self) -> u32 {
        match self.kind {
            AnnotationKind::PrinterMark => PRINTER_MARK_FLAGS,
            AnnotationKind::TrapNet => super::trap_net::TRAP_NETWORK_FLAGS,
            _ => self.flags(),
        }
    }

    /// Printer's mark 专有条目 `/MN`（Table 362）。
    pub(crate) fn printer_mark(&self) -> Option<&PrinterMarkEntries> {
        self.printer_mark.as_ref()
    }
}

/// Printer's mark 注解专有条目（Table 362）的构建态。
#[derive(Debug, Clone, Default)]
pub(crate) struct PrinterMarkEntries {
    /// `/MN` 标记名称（name）。
    mark_name: Option<String>,
}

impl PrinterMarkEntries {
    /// 设置 `/MN` 标记名称。
    pub(crate) fn with_mark_name(mut self, name: impl Into<String>) -> Self {
        self.mark_name = Some(name.into());
        self
    }

    /// `/MN` 标记名称。
    pub(crate) fn mark_name(&self) -> Option<&str> {
        self.mark_name.as_deref()
    }

    /// 校验 `/MN` 为合法 PDF name。
    pub(crate) fn validate(&self) -> Result<(), String> {
        if let Some(name) = &self.mark_name {
            validate_name_value(name, "PrinterMark annotation /MN")?;
        }
        Ok(())
    }
}

/// Printer's mark 注解的整体校验：条目归属、`/F` 取值与外观选择。
pub(super) fn validate(annot: &PdfAnnotation) -> Result<(), String> {
    match (annot.kind, &annot.printer_mark) {
        (AnnotationKind::PrinterMark, Some(entries)) => entries.validate()?,
        (AnnotationKind::PrinterMark, None) => {}
        (_, Some(_)) => {
            return Err("annotation /MN is specific to PrinterMark annotations".into());
        }
        (_, None) => return Ok(()),
    }
    // 14.11.3：`/F` 的 Print 与 ReadOnly 置位、其余位清零（即 68）；
    // 未显式声明（0）时由写出阶段恒写 68。
    if annot.flags != 0 && annot.flags != PRINTER_MARK_FLAGS {
        return Err(format!(
            "PrinterMark annotation /F must be 0 or 68 (Print and ReadOnly), got {}",
            annot.flags
        ));
    }
    if annot.appearance_normal.is_none() && annot.appearance_states.is_empty() {
        return Err("PrinterMark annotation requires a normal appearance stream (/AP /N)".into());
    }
    if annot.appearance_states.len() > 1 && annot.appearance_state.is_none() {
        return Err("PrinterMark annotation /AP /N has multiple appearances but no /AS".into());
    }
    Ok(())
}

/// Printer's mark 注解的规范 `/F` 值：Print | ReadOnly。
pub(crate) const PRINTER_MARK_FLAGS: u32 = 68;
