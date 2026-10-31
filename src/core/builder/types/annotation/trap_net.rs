//! Trap 网络注解写端条目（14.11.6，Table 366）。
//!
//! Table 366 的跟踪条目只有两种互斥形式：`/LastModified`（PDF 1.4 日期），
//! 或 `/Version`（页面描述对象引用数组）+ `/AnnotStates`（与页面其余注解
//! 顺序对应的 name/null 数组）。14.11.6 正文要求 `/AP` 与 `/F` 必须存在：
//! `/F` 的 Print（4）与 ReadOnly（64）置位、其余位清零，写出恒为 68；
//! `/AP /N` 必须有正常外观，多于一个外观流时必须声明 `/AS`。
//!
//! Table 367 的表单条目（`/PCM` 等）由
//! [`FormXObject`](super::super::FormXObject) 承载，本模块只校验注解侧
//! 条目与归属；全部校验发生在对象号分配与字节写出之前。页面级「至多
//! 一个 Trap 网络且位于 `/Annots` 末尾」的约束由 assemble 阶段保证。

use crate::core::types::{PdfDate, Value};

use super::{AnnotationKind, PdfAnnotation, validate_name_value};

impl PdfAnnotation {
    /// Trap 网络修改日期 `/LastModified`（Table 366，PDF 1.4）。
    ///
    /// 与 [`Self::with_trap_network_version`] 互斥：设置日期会清除
    /// `/Version` 与 `/AnnotStates`。
    pub fn with_trap_network_last_modified(mut self, date: PdfDate) -> Self {
        self.trap_network = Some(TrapNetworkEntries {
            last_modified: Some(date),
            version: None,
            annot_states: None,
        });
        self
    }

    /// Trap 网络 `/Version` 页面描述对象引用与 `/AnnotStates` 状态数组
    /// （Table 366，PDF 1.3）。
    ///
    /// 与 [`Self::with_trap_network_last_modified`] 互斥：设置两者会清除
    /// `/LastModified`。`version` 的每一项必须是 [`Value::Ref`]；
    /// `annot_states` 的每一项为 `Some(name)`（写为 name）或 `None`
    /// （写为 null）。
    pub fn with_trap_network_version(
        mut self,
        version: Vec<Value>,
        annot_states: Vec<Option<String>>,
    ) -> Self {
        self.trap_network = Some(TrapNetworkEntries {
            last_modified: None,
            version: Some(version),
            annot_states: Some(annot_states),
        });
        self
    }

    /// Trap 网络专有跟踪条目（Table 366）。
    pub(crate) fn trap_network(&self) -> Option<&TrapNetworkEntries> {
        self.trap_network.as_ref()
    }
}

/// Trap 网络注解跟踪条目（Table 366）的构建态。
#[derive(Debug, Clone, Default)]
pub(crate) struct TrapNetworkEntries {
    /// `/LastModified` 修改日期（PDF 1.4）。
    last_modified: Option<PdfDate>,
    /// `/Version` 页面描述对象引用数组。
    version: Option<Vec<Value>>,
    /// `/AnnotStates` 与页面其余注解顺序对应的状态（None 写为 null）。
    annot_states: Option<Vec<Option<String>>>,
}

impl TrapNetworkEntries {
    /// `/LastModified` 修改日期。
    pub(crate) fn last_modified(&self) -> Option<PdfDate> {
        self.last_modified
    }

    /// `/Version` 页面描述对象引用数组。
    pub(crate) fn version(&self) -> Option<&[Value]> {
        self.version.as_deref()
    }

    /// `/AnnotStates` 状态数组。
    pub(crate) fn annot_states(&self) -> Option<&[Option<String>]> {
        self.annot_states.as_deref()
    }

    /// Table 366：`/LastModified` 与 `/Version` + `/AnnotStates` 恰有其一。
    fn validate_tracking(&self) -> Result<(), String> {
        match (&self.last_modified, &self.version, &self.annot_states) {
            (Some(_), None, None) => {}
            (None, Some(_), Some(_)) => {}
            (Some(_), _, _) => {
                return Err(
                    "TrapNet annotation /LastModified and /Version//AnnotStates are mutually \
                     exclusive"
                        .into(),
                );
            }
            _ => {
                return Err(
                    "TrapNet annotation requires either /LastModified or /Version and \
                     /AnnotStates"
                        .into(),
                );
            }
        }
        if let Some(version) = &self.version
            && version.iter().any(|entry| entry.as_ref().is_none())
        {
            return Err("TrapNet annotation /Version entries must be indirect references".into());
        }
        if let Some(states) = &self.annot_states {
            for state in states.iter().flatten() {
                validate_name_value(state, "TrapNet annotation /AnnotStates entry")?;
            }
        }
        Ok(())
    }
}

/// Trap 网络注解的整体校验：条目归属、跟踪形式、`/F` 取值与外观选择。
pub(super) fn validate(annot: &PdfAnnotation) -> Result<(), String> {
    let entries = match (annot.kind, &annot.trap_network) {
        (AnnotationKind::TrapNet, Some(entries)) => entries,
        (AnnotationKind::TrapNet, None) => {
            return Err(
                "TrapNet annotation requires either /LastModified or /Version and /AnnotStates"
                    .into(),
            );
        }
        (_, Some(_)) => {
            return Err("trap network entries are specific to TrapNet annotations".into());
        }
        (_, None) => return Ok(()),
    };
    entries.validate_tracking()?;
    // 14.11.6：`/F` 的 Print 与 ReadOnly 置位、其余位清零（即 68）；
    // 未显式声明（0）时由写出阶段恒写 68。
    if annot.flags != 0 && annot.flags != TRAP_NETWORK_FLAGS {
        return Err(format!(
            "TrapNet annotation /F must be 0 or 68 (Print and ReadOnly), got {}",
            annot.flags
        ));
    }
    if annot.appearance_normal.is_none() && annot.appearance_states.is_empty() {
        return Err("TrapNet annotation requires a normal appearance stream (/AP /N)".into());
    }
    if annot.appearance_states.len() > 1 && annot.appearance_state.is_none() {
        return Err("TrapNet annotation /AP /N has multiple appearances but no /AS".into());
    }
    Ok(())
}

/// Trap 网络注解的规范 `/F` 值：Print | ReadOnly。
pub(crate) const TRAP_NETWORK_FLAGS: u32 = 68;
