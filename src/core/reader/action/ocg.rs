//! Set-OCG-State 动作的 `/State` 模型与解析（12.6.4.12 Table 213）。

use crate::core::reader::navigation::resolve;
use crate::core::types::{Document, Value};

use super::{ActionError, malformed};

/// Set-OCG-State 动作（12.6.4.12 Table 213）。
#[derive(Debug, Clone, PartialEq)]
pub struct SetOcgStateAction {
    /// `/State`：从左到右依次应用的操作序列。
    pub state: Vec<OcgStateSequence>,
    /// `/PreserveRB`；缺省 `true`（保留单选按钮关系），此处不物化。
    pub preserve_radio_buttons: Option<bool>,
}

/// `/State` 中的一个操作序列：操作名及其后的一个或多个组引用。
#[derive(Debug, Clone, PartialEq)]
pub struct OcgStateSequence {
    /// 序列起始的操作名。
    pub operation: OcgStateOperation,
    /// 该操作作用的可选内容组字典间接引用（对象号、生成号）。
    pub groups: Vec<(i64, i64)>,
}

/// Set-OCG-State 的操作名（Table 213）；未知名字保留。
#[derive(Debug, Clone, PartialEq)]
pub enum OcgStateOperation {
    /// `ON`：置为可见。
    On,
    /// `OFF`：置为隐藏。
    Off,
    /// `Toggle`：反转。
    Toggle,
    /// 非标准操作名。
    Other(String),
}

/// 解析 Set-OCG-State 的 `/State` 扁平数组：名字开启一个序列，其后直到
/// 下一个名字的组字典间接引用归入该序列（Table 213）。
pub(super) fn parse_set_ocg_state(
    document: &Document,
    value: &Value,
) -> Result<Vec<OcgStateSequence>, ActionError> {
    let value = resolve(document, value)?;
    let array = value
        .as_array()
        .ok_or_else(|| malformed("SetOCGState action /State is not an array"))?;
    let mut sequences: Vec<OcgStateSequence> = Vec::new();
    for item in &array.0 {
        if let Value::Ref(number, generation) = item {
            let sequence = sequences.last_mut().ok_or_else(|| {
                malformed("SetOCGState action /State begins with groups before an operation")
            })?;
            if !matches!(resolve(document, item)?, Value::Dict(_)) {
                return Err(malformed(
                    "SetOCGState action /State group reference does not resolve to an optional content group dictionary",
                ));
            }
            sequence.groups.push((*number, *generation));
            continue;
        }
        match resolve(document, item)? {
            Value::Name(name) => {
                if sequences
                    .last()
                    .is_some_and(|sequence| sequence.groups.is_empty())
                {
                    return Err(malformed(
                        "SetOCGState action /State has an operation without groups",
                    ));
                }
                sequences.push(OcgStateSequence {
                    operation: ocg_state_operation(&name),
                    groups: Vec::new(),
                });
            }
            _ => {
                return Err(malformed(
                    "SetOCGState action /State entry is neither an operation name nor a group reference",
                ));
            }
        }
    }
    if sequences.is_empty() {
        return Err(malformed("SetOCGState action /State array is empty"));
    }
    if sequences
        .last()
        .is_some_and(|sequence| sequence.groups.is_empty())
    {
        return Err(malformed(
            "SetOCGState action /State has an operation without groups",
        ));
    }
    Ok(sequences)
}

fn ocg_state_operation(name: &str) -> OcgStateOperation {
    match name {
        "ON" => OcgStateOperation::On,
        "OFF" => OcgStateOperation::Off,
        "Toggle" => OcgStateOperation::Toggle,
        other => OcgStateOperation::Other(other.to_owned()),
    }
}
