//! Rendition 动作写端（12.6.4.13 Table 214，PDF 1.5）。
//!
//! `/OP` 与 `/JS` 至少写其一：`/OP` 取 Table 214 定义的五个操作之一时
//! `/AN` 必需，其中 Play（0）与 PlayIfPaused（4）还要求 `/R`；未声明
//! `/OP` 时 `/JS` 必需。`/JS` 可与 `/OP` 并存，供不识别该操作的查看器
//! 回退执行。rendition 对象随动作内联写出，其嵌入文件与 Form XObject
//! 句柄由 [`ActionRefs`] 解析。
//!
//! 句柄越界与操作依赖缺失在任何对象写出前由 `action_requirements` 拒绝，
//! 保证零字节输出契约。

use super::ActionRefs;
use crate::core::builder::types::{Rendition, text_string_value};
use crate::core::types::{Dict, Value};
use crate::core::writer::WriteError;

/// `/OP` 操作（Table 214）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenditionOperation {
    /// `0`：播放。
    Play,
    /// `1`：停止。
    Stop,
    /// `2`：暂停。
    Pause,
    /// `3`：恢复播放。
    Resume,
    /// `4`：未在播放时开始播放。
    PlayIfPaused,
}

impl RenditionOperation {
    /// `/OP` 整数值。
    pub fn value(self) -> i64 {
        match self {
            Self::Play => 0,
            Self::Stop => 1,
            Self::Pause => 2,
            Self::Resume => 3,
            Self::PlayIfPaused => 4,
        }
    }

    /// 该操作是否要求 `/R`（Table 214：`/OP` 为 0 或 4）。
    fn requires_rendition(self) -> bool {
        matches!(self, Self::Play | Self::PlayIfPaused)
    }
}

/// Rendition 动作的构建态（Table 214）。
#[derive(Debug, Clone, PartialEq)]
pub struct RenditionAction {
    rendition: Option<Rendition>,
    annotation: Option<usize>,
    operation: Option<RenditionOperation>,
    javascript: Option<String>,
}

impl RenditionAction {
    /// 播放指定 rendition；`/AN` 指向 Screen 注解句柄。
    pub fn play(annotation: usize, rendition: Rendition) -> Self {
        Self {
            rendition: Some(rendition),
            annotation: Some(annotation),
            operation: Some(RenditionOperation::Play),
            javascript: None,
        }
    }

    /// 仅在未播放时播放指定 rendition。
    pub fn play_if_paused(annotation: usize, rendition: Rendition) -> Self {
        Self {
            rendition: Some(rendition),
            annotation: Some(annotation),
            operation: Some(RenditionOperation::PlayIfPaused),
            javascript: None,
        }
    }

    /// 停止播放；`/R` 可省略。
    pub fn stop(annotation: usize) -> Self {
        Self {
            rendition: None,
            annotation: Some(annotation),
            operation: Some(RenditionOperation::Stop),
            javascript: None,
        }
    }

    /// 暂停播放。
    pub fn pause(annotation: usize) -> Self {
        Self {
            rendition: None,
            annotation: Some(annotation),
            operation: Some(RenditionOperation::Pause),
            javascript: None,
        }
    }

    /// 恢复播放。
    pub fn resume(annotation: usize) -> Self {
        Self {
            rendition: None,
            annotation: Some(annotation),
            operation: Some(RenditionOperation::Resume),
            javascript: None,
        }
    }

    /// 仅执行 JavaScript 脚本（不声明 `/OP`）。
    pub fn script(javascript: impl Into<String>) -> Self {
        Self {
            rendition: None,
            annotation: None,
            operation: None,
            javascript: Some(javascript.into()),
        }
    }

    /// 附加 `/R` rendition。
    pub fn with_rendition(mut self, rendition: Rendition) -> Self {
        self.rendition = Some(rendition);
        self
    }

    /// 附加 `/AN` Screen 注解句柄。
    pub fn with_annotation(mut self, annotation: usize) -> Self {
        self.annotation = Some(annotation);
        self
    }

    /// 附加 `/JS` 脚本；可与 `/OP` 并存。
    pub fn with_javascript(mut self, javascript: impl Into<String>) -> Self {
        self.javascript = Some(javascript.into());
        self
    }

    /// `/AN` 指向的 Screen 注解句柄。
    pub fn annotation(&self) -> Option<usize> {
        self.annotation
    }

    /// 校验操作依赖与句柄上界。
    pub(crate) fn validate(
        &self,
        annotation_count: usize,
        form_count: usize,
        embedded_count: usize,
    ) -> Result<(), WriteError> {
        match self.operation {
            Some(operation) => {
                if self.annotation.is_none() {
                    return Err(WriteError::InvalidObject(
                        "Rendition action /AN is required when /OP is present".into(),
                    ));
                }
                if operation.requires_rendition() && self.rendition.is_none() {
                    return Err(WriteError::InvalidObject(
                        "Rendition action /R is required when /OP is 0 or 4".into(),
                    ));
                }
            }
            None if self.javascript.is_none() => {
                return Err(WriteError::InvalidObject(
                    "Rendition action requires at least one of /OP or /JS".into(),
                ));
            }
            None => {}
        }
        if let Some(handle) = self.annotation
            && handle >= annotation_count
        {
            return Err(WriteError::InvalidObject(format!(
                "Rendition action /AN references unknown annotation handle {handle}"
            )));
        }
        if self.javascript.as_deref().is_some_and(str::is_empty) {
            return Err(WriteError::InvalidObject(
                "Rendition action /JS must not be empty".into(),
            ));
        }
        if let Some(rendition) = &self.rendition {
            rendition.validate(form_count, embedded_count)?;
        }
        Ok(())
    }

    /// `/R` 引用的 Form XObject 句柄（供写出前分配对象号）。
    pub(crate) fn form_handles(&self) -> Vec<usize> {
        let mut handles = Vec::new();
        if let Some(rendition) = &self.rendition {
            rendition.collect_form_handles(&mut handles);
        }
        handles
    }

    /// 写出除 `/S` 外的动作条目。
    pub(crate) fn to_dict(&self, refs: &ActionRefs<'_>) -> Result<Dict, WriteError> {
        self.validate(
            refs.annotation_nums.len(),
            refs.form_nums.len(),
            refs.embedded_nums.len(),
        )?;
        let mut dict = Dict::new();
        if let Some(operation) = self.operation {
            dict = dict.insert("OP", Value::Integer(operation.value()));
        }
        if let Some(handle) = self.annotation {
            let num = refs.annotation_nums.get(handle).ok_or_else(|| {
                WriteError::InvalidObject(format!(
                    "Rendition action /AN references unknown annotation handle {handle}"
                ))
            })?;
            dict = dict.insert("AN", Value::Ref(*num, 0));
        }
        if let Some(rendition) = &self.rendition {
            dict = dict.insert(
                "R",
                Value::Dict(rendition.to_dict(refs.form_nums, refs.embedded_nums)?),
            );
        }
        if let Some(javascript) = &self.javascript {
            dict = dict.insert("JS", text_string_value(javascript));
        }
        Ok(dict)
    }
}
