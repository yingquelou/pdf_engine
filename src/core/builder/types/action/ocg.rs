//! Set-OCG-State 动作的扁平 `/State` 序列与写出前校验
//! （12.6.4.12 Table 213）。

use crate::core::writer::WriteError;

/// Set-OCG-State 动作（12.6.4.12 Table 213，PDF 1.5）。
#[derive(Debug, Clone, PartialEq)]
pub struct SetOcgStateAction {
    sequences: Vec<OcgStateSequence>,
    preserve_radio_buttons: Option<bool>,
}

impl SetOcgStateAction {
    /// 构造动作；序列按 `/State` 的处理顺序给出，至少一项。
    pub fn new(sequences: Vec<OcgStateSequence>) -> Self {
        Self {
            sequences,
            preserve_radio_buttons: None,
        }
    }

    /// 显式写 `/PreserveRB`：`true` 保持单选按钮组互斥关系（缺省即 `true`），
    /// `false` 忽略该关系。
    pub fn with_preserve_radio_buttons(mut self, preserve: bool) -> Self {
        self.preserve_radio_buttons = Some(preserve);
        self
    }

    pub(crate) fn sequences(&self) -> &[OcgStateSequence] {
        &self.sequences
    }

    pub(crate) fn preserve_radio_buttons(&self) -> Option<bool> {
        self.preserve_radio_buttons
    }

    /// 校验序列与组句柄上界（写出前的零字节契约）。
    pub(crate) fn validate(&self, ocg_count: usize) -> Result<(), WriteError> {
        if self.sequences.is_empty() {
            return Err(WriteError::InvalidObject(
                "SetOCGState action /State must contain at least one sequence".into(),
            ));
        }
        for sequence in &self.sequences {
            if sequence.groups.is_empty() {
                return Err(WriteError::InvalidObject(format!(
                    "SetOCGState action /State sequence {:?} names no optional content group",
                    sequence.operation.name()
                )));
            }
            for &group in &sequence.groups {
                if group >= ocg_count {
                    return Err(WriteError::InvalidObject(format!(
                        "SetOCGState action /State references unknown optional content group handle {group}"
                    )));
                }
            }
        }
        Ok(())
    }
}

/// `/State` 的一个序列：操作名作用于紧随其后的全部组（Table 213）。
#[derive(Debug, Clone, PartialEq)]
pub struct OcgStateSequence {
    operation: OcgStateOperation,
    groups: Vec<usize>,
}

impl OcgStateSequence {
    /// 构造序列；`groups` 为可选内容组注册句柄，至少一项。
    pub fn new(operation: OcgStateOperation, groups: Vec<usize>) -> Self {
        Self { operation, groups }
    }

    pub(crate) fn operation(&self) -> &OcgStateOperation {
        &self.operation
    }

    pub(crate) fn groups(&self) -> &[usize] {
        &self.groups
    }
}

/// 序列操作名（Table 213）。
#[derive(Debug, Clone, PartialEq)]
pub enum OcgStateOperation {
    /// 将后续组置为开启。
    On,
    /// 将后续组置为关闭。
    Off,
    /// 反转后续组的状态。
    Toggle,
}

impl OcgStateOperation {
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::On => "ON",
            Self::Off => "OFF",
            Self::Toggle => "Toggle",
        }
    }
}
