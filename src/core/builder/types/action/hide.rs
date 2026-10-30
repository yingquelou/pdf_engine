//! Hide 动作的目标与写出前校验（12.6.4.10 Table 210）。

use crate::core::writer::WriteError;

/// Hide 动作（12.6.4.10 Table 210）。
#[derive(Debug, Clone, PartialEq)]
pub struct HideAction {
    targets: Vec<HideTarget>,
    hide: Option<bool>,
}

impl HideAction {
    /// 构造 Hide 动作；`targets` 至少一项。
    pub fn new(targets: Vec<HideTarget>) -> Self {
        Self {
            targets,
            hide: None,
        }
    }

    /// 显式写 `/H`：`true` 隐藏、`false` 显示；缺省按 Table 210 视为 `true`。
    pub fn with_hidden(mut self, hidden: bool) -> Self {
        self.hide = Some(hidden);
        self
    }

    pub(crate) fn targets(&self) -> &[HideTarget] {
        &self.targets
    }

    pub(crate) fn hide(&self) -> Option<bool> {
        self.hide
    }

    /// 校验目标并与注解池规模比对（写出前的零字节契约）。
    pub(crate) fn validate(&self, annotation_count: usize) -> Result<(), WriteError> {
        if self.targets.is_empty() {
            return Err(WriteError::InvalidObject(
                "Hide action /T must name at least one target".into(),
            ));
        }
        let mut names = std::collections::HashSet::new();
        let mut handles = std::collections::HashSet::new();
        for target in &self.targets {
            match target {
                HideTarget::FieldName(name) => {
                    if name.trim().is_empty() {
                        return Err(WriteError::InvalidObject(
                            "Hide action /T field name must not be empty".into(),
                        ));
                    }
                    if !names.insert(name.as_str()) {
                        return Err(WriteError::InvalidObject(format!(
                            "Hide action /T field name {name:?} is duplicated"
                        )));
                    }
                }
                HideTarget::Annotation(handle) => {
                    if *handle >= annotation_count {
                        return Err(WriteError::InvalidObject(format!(
                            "Hide action /T references unknown annotation handle {handle}"
                        )));
                    }
                    if !handles.insert(*handle) {
                        return Err(WriteError::InvalidObject(format!(
                            "Hide action /T references annotation handle {handle} twice"
                        )));
                    }
                }
            }
        }
        Ok(())
    }
}

/// Hide 动作的目标（Table 210 `/T`）。
#[derive(Debug, Clone, PartialEq)]
pub enum HideTarget {
    /// 注解注册句柄（`PdfBuilder::add_annotation` 的返回值）。
    Annotation(usize),
    /// 交互表单字段的全限定名。
    FieldName(String),
}
