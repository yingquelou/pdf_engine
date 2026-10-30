//! 动作共用的文件规格、远程目标与查看器预定义动作名（12.6.2）。

use super::super::text_string_value;
use crate::core::types::{Dict, Value};

/// 文件规格（7.11.2）：写端只生成 `/UF` 统一文件路径。
#[derive(Debug, Clone, PartialEq)]
pub struct ActionFileSpec {
    file: String,
}

impl ActionFileSpec {
    /// 构造文件规格；路径按 UTF-16BE 文本字符串写出。
    pub fn new(file: impl Into<String>) -> Self {
        Self { file: file.into() }
    }

    pub(crate) fn to_value(&self) -> Value {
        Value::Dict(Dict::new().insert("UF", text_string_value(&self.file)))
    }
}

/// 远程文档目标（GoToR `/D`）。
#[derive(Debug, Clone, PartialEq)]
pub enum ActionRemoteDestination {
    /// 远程文档中的命名目标。
    Named(String),
    /// 远程文档中的 0 基页号。
    Page(i64),
}

impl ActionRemoteDestination {
    pub(crate) fn to_value(&self) -> Value {
        match self {
            Self::Named(name) => Value::String(name.clone()),
            Self::Page(page) => Value::Integer(*page),
        }
    }
}

/// 查看器预定义动作。
#[derive(Debug, Clone, PartialEq)]
pub enum ActionNamedAction {
    /// 下一页。
    NextPage,
    /// 上一页。
    PrevPage,
    /// 第一页。
    FirstPage,
    /// 最后一页。
    LastPage,
    /// 其他预定义动作（按名称原样写出）。
    Other(String),
}

impl ActionNamedAction {
    pub(crate) fn name(&self) -> &str {
        match self {
            Self::NextPage => "NextPage",
            Self::PrevPage => "PrevPage",
            Self::FirstPage => "FirstPage",
            Self::LastPage => "LastPage",
            Self::Other(name) => name,
        }
    }
}
