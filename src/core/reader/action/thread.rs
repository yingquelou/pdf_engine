//! Thread 动作读取（12.6.4.6 Table 205）。
//!
//! `/D` 目标线程支持三种形式：线程字典间接引用、目录 `/Threads` 数组
//! 的 0 基索引、线程信息字典标题；`/B` 支持 bead 间接引用或线程内的
//! 0 基索引。`/F` 缺省表示线程在当前文件。

use crate::core::reader::content::text_value;
use crate::core::reader::navigation::resolve;
use crate::core::types::{Dict, Document, Value};

use super::{ActionError, FileSpec, malformed, parse_file_spec};

/// Thread 动作（12.6.4.6 Table 205）：跳转到文章线上的指定 bead。
#[derive(Debug, Clone, PartialEq)]
pub struct ThreadAction {
    /// `/F` 线程所在文件；缺省表示当前文件。
    pub file: Option<FileSpec>,
    /// `/D` 目标线程。
    pub thread: ThreadTarget,
    /// `/B` 目标 bead；缺省时查看器从线程首 bead 开始。
    pub bead: Option<BeadTarget>,
}

/// Thread 动作的目标线程（Table 205 `/D`）。
#[derive(Debug, Clone, PartialEq)]
pub enum ThreadTarget {
    /// 线程字典的间接引用（线程须在当前文件）。
    Reference {
        /// 对象号。
        number: i64,
        /// 生成号。
        generation: i64,
    },
    /// 目录 `/Threads` 数组中的 0 基索引。
    Index(i64),
    /// 线程信息字典的标题（`/I /T`）。
    Title(String),
}

/// Thread 动作的目标 bead（Table 205 `/B`）。
#[derive(Debug, Clone, PartialEq)]
pub enum BeadTarget {
    /// bead 字典的间接引用（线程须在当前文件）。
    Reference {
        /// 对象号。
        number: i64,
        /// 生成号。
        generation: i64,
    },
    /// bead 在线程内的 0 基索引。
    Index(i64),
}

/// 解析 Thread 动作的专有条目（`/S` 已由调用方识别）。
pub(super) fn parse_thread_action(
    document: &Document,
    dict: &Dict,
) -> Result<ThreadAction, ActionError> {
    let file = match dict.get("F") {
        None => None,
        Some(value) => Some(parse_file_spec(document, value)?),
    };
    let thread = parse_thread_target(
        document,
        dict.get("D")
            .ok_or_else(|| malformed("Thread action is missing /D"))?,
    )?;
    let bead = match dict.get("B") {
        None => None,
        Some(value) => Some(parse_bead_target(document, value)?),
    };
    Ok(ThreadAction { file, thread, bead })
}

fn parse_thread_target(document: &Document, value: &Value) -> Result<ThreadTarget, ActionError> {
    match value {
        Value::Ref(number, generation) => {
            if !matches!(resolve(document, value)?, Value::Dict(_)) {
                return Err(malformed(
                    "Thread action /D reference does not resolve to a thread dictionary",
                ));
            }
            Ok(ThreadTarget::Reference {
                number: *number,
                generation: *generation,
            })
        }
        Value::Integer(index) if *index >= 0 => Ok(ThreadTarget::Index(*index)),
        Value::String(_) | Value::HexString(_) => text_value(value)
            .map(ThreadTarget::Title)
            .ok_or_else(|| malformed("Thread action /D title is malformed")),
        _ => Err(malformed(
            "Thread action /D is neither a thread reference, a non-negative index nor a title",
        )),
    }
}

fn parse_bead_target(document: &Document, value: &Value) -> Result<BeadTarget, ActionError> {
    match value {
        Value::Ref(number, generation) => {
            if !matches!(resolve(document, value)?, Value::Dict(_)) {
                return Err(malformed(
                    "Thread action /B reference does not resolve to a bead dictionary",
                ));
            }
            Ok(BeadTarget::Reference {
                number: *number,
                generation: *generation,
            })
        }
        Value::Integer(index) if *index >= 0 => Ok(BeadTarget::Index(*index)),
        _ => Err(malformed(
            "Thread action /B is neither a bead reference nor a non-negative index",
        )),
    }
}
