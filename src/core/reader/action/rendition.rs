//! Rendition 动作读取（12.6.4.13 Table 214）。
//!
//! 动作控制多媒体 rendition 的播放：`/OP` 定义播放/停止/暂停/继续操作，
//! `/AN` 指向承载播放的 Screen 注解，`/R` 为 rendition 对象；无 `/OP`
//! 时 `/JS` 提供 JavaScript 回退。`/OP` 取 0 或 4 时 `/R` 必填，取 0–4
//! 时 `/AN` 必填；未识别的 `/OP` 只有在 `/JS` 存在时才有效。

use crate::core::reader::content::text_value;
use crate::core::reader::navigation::resolve;
use crate::core::reader::rendition::Rendition;
use crate::core::types::{Dict, Document, Value};

use super::{ActionError, malformed};

/// rendition 动作的播放操作（Table 214 `/OP`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenditionOperation {
    /// 0：播放 `/R` 并关联到 `/AN`。
    Play,
    /// 1：停止并解除关联。
    Stop,
    /// 2：暂停。
    Pause,
    /// 3：继续。
    Resume,
    /// 4：播放 `/R`；已关联且暂停时继续。
    PlayIfPaused,
    /// 未识别的整数值（仅在 `/JS` 存在时有效）。
    Other(i64),
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
            Self::Other(value) => value,
        }
    }

    /// 是否为 Table 214 定义的操作。
    pub fn is_defined(self) -> bool {
        !matches!(self, Self::Other(_))
    }

    fn parse(value: i64) -> Self {
        match value {
            0 => Self::Play,
            1 => Self::Stop,
            2 => Self::Pause,
            3 => Self::Resume,
            4 => Self::PlayIfPaused,
            other => Self::Other(other),
        }
    }
}

/// Rendition 动作（Table 214）。
#[derive(Debug, Clone, PartialEq)]
pub struct RenditionAction {
    /// `/R` rendition 对象。
    pub rendition: Option<Rendition>,
    /// `/AN` Screen 注解的间接引用。
    pub annotation: Option<(i64, i64)>,
    /// `/OP` 播放操作。
    pub operation: Option<RenditionOperation>,
    /// `/JS` JavaScript 回退脚本。
    pub javascript: Option<String>,
}

/// 解析 Rendition 动作字典；`/S` 已由调用方识别。
pub(super) fn parse_rendition_action(
    document: &Document,
    dict: &Dict,
) -> Result<RenditionAction, ActionError> {
    let javascript = match dict.get("JS") {
        None => None,
        Some(value) => {
            let value = resolve(document, value)?;
            let script = match &value {
                Value::String(_) | Value::HexString(_) => text_value(&value),
                Value::Stream(..) => document
                    .try_stream_bytes(&value)
                    .ok()
                    .and_then(|bytes| String::from_utf8(bytes).ok()),
                _ => None,
            }
            .ok_or_else(|| {
                malformed("Rendition action /JS is not a text string or UTF-8 stream")
            })?;
            Some(script)
        }
    };
    let operation = match dict.get("OP") {
        None => None,
        Some(value) => {
            let value = resolve(document, value)?;
            match value {
                Value::Integer(number) => Some(RenditionOperation::parse(number)),
                _ => return Err(malformed("Rendition action /OP is not an integer")),
            }
        }
    };
    let annotation = match dict.get("AN") {
        None => None,
        Some(Value::Ref(number, generation)) => {
            let reference = Value::Ref(*number, *generation);
            let resolved = resolve(document, &reference)?;
            let target = resolved.as_dict().ok_or_else(|| {
                malformed("Rendition action /AN does not resolve to a dictionary")
            })?;
            match target.get("Subtype").and_then(Value::as_name) {
                Some("Screen") => {}
                Some(other) => {
                    return Err(malformed(format!(
                        "Rendition action /AN targets a {other} annotation"
                    )));
                }
                None => {
                    return Err(malformed("Rendition action /AN target has no /Subtype"));
                }
            }
            Some((*number, *generation))
        }
        Some(_) => {
            return Err(malformed(
                "Rendition action /AN must be an indirect reference",
            ));
        }
    };
    let rendition = match dict.get("R") {
        None => None,
        Some(value) => Some(
            document
                .rendition(value)
                .map_err(|error| malformed(format!("Rendition action /R: {error}")))?,
        ),
    };

    match (operation, javascript.is_some()) {
        (None, false) => {
            return Err(malformed(
                "Rendition action requires at least one of /OP or /JS",
            ));
        }
        (Some(RenditionOperation::Other(_)), false) => {
            return Err(malformed(
                "Rendition action /OP is unrecognized and no /JS fallback is present",
            ));
        }
        (Some(operation), _) if operation.is_defined() => {
            if annotation.is_none() {
                return Err(malformed(
                    "Rendition action /AN is required when /OP is present",
                ));
            }
            if matches!(
                operation,
                RenditionOperation::Play | RenditionOperation::PlayIfPaused
            ) && rendition.is_none()
            {
                return Err(malformed(
                    "Rendition action /R is required when /OP is 0 or 4",
                ));
            }
        }
        _ => {}
    }

    Ok(RenditionAction {
        rendition,
        annotation,
        operation,
        javascript,
    })
}
