//! Embedded Go-To 动作写端（12.6.4.4 Table 201/202）。
//!
//! 写端生成目标字典路径链：子目标用 EmbeddedFiles 名称树的 `/N` 或
//! 文件附件注解的 `/P` + `/A` 定位，父目标不带定位条目。`/F` 与 `/T`
//! 至少声明一项（Table 201）。

use super::targets::{ActionFileSpec, ActionRemoteDestination};
use super::text_string_value;
use crate::core::types::{Dict, Value};
use crate::core::writer::WriteError;

/// Embedded Go-To 动作（12.6.4.4 Table 201，PDF 1.6）。
#[derive(Debug, Clone, PartialEq)]
pub struct GoToEAction {
    file: Option<ActionFileSpec>,
    destination: ActionRemoteDestination,
    new_window: Option<bool>,
    target: Option<GoToETarget>,
}

impl GoToEAction {
    /// 以目标文档内的目标构造；`/F` 与 `/T` 需至少补一项。
    pub fn new(destination: ActionRemoteDestination) -> Self {
        Self {
            file: None,
            destination,
            new_window: None,
            target: None,
        }
    }

    /// `/F` 目标根文档（相对源根文档）。
    pub fn with_file(mut self, file: ActionFileSpec) -> Self {
        self.file = Some(file);
        self
    }

    /// `/NewWindow`；显式声明是否在新窗口打开目标文档。
    pub fn with_new_window(mut self, flag: bool) -> Self {
        self.new_window = Some(flag);
        self
    }

    /// `/T` 目标路径链的首段。
    pub fn with_target(mut self, target: GoToETarget) -> Self {
        self.target = Some(target);
        self
    }

    pub(crate) fn validate(&self) -> Result<(), WriteError> {
        if self.file.is_none() && self.target.is_none() {
            return Err(WriteError::InvalidObject(
                "GoToE action requires /F or /T".into(),
            ));
        }
        if let Some(target) = &self.target {
            target.validate()?;
        }
        Ok(())
    }

    pub(crate) fn to_dict(&self) -> Dict {
        let mut dict = Dict::new();
        if let Some(file) = &self.file {
            dict = dict.insert("F", file.to_value());
        }
        dict = dict.insert("D", self.destination.to_value());
        if let Some(flag) = self.new_window {
            dict = dict.insert("NewWindow", Value::Bool(flag));
        }
        if let Some(target) = &self.target {
            dict = dict.insert("T", Value::Dict(target.to_dict()));
        }
        dict
    }
}

/// 目标字典（Table 202）：路径上的一个元素。
#[derive(Debug, Clone, PartialEq)]
pub struct GoToETarget {
    relation: GoToETargetRelation,
    name: Option<String>,
    page: Option<GoToETargetPage>,
    annotation: Option<GoToETargetAnnotation>,
    target: Option<Box<GoToETarget>>,
}

impl GoToETarget {
    /// 目标为当前文档的父文档（`/R /P`）。
    pub fn parent() -> Self {
        Self {
            relation: GoToETargetRelation::Parent,
            name: None,
            page: None,
            annotation: None,
            target: None,
        }
    }

    /// 目标为当前文档的子文档，位于 EmbeddedFiles 名称树（`/R /C` + `/N`）。
    pub fn child_in_name_tree(name: impl Into<String>) -> Self {
        Self {
            relation: GoToETargetRelation::Child,
            name: Some(name.into()),
            page: None,
            annotation: None,
            target: None,
        }
    }

    /// 目标为当前文档的子文档，经文件附件注解定位（`/R /C` + `/P` + `/A`）。
    pub fn child_attachment(page: GoToETargetPage, annotation: GoToETargetAnnotation) -> Self {
        Self {
            relation: GoToETargetRelation::Child,
            name: None,
            page: Some(page),
            annotation: Some(annotation),
            target: None,
        }
    }

    /// `/T` 路径的下一段。
    pub fn with_target(mut self, target: GoToETarget) -> Self {
        self.target = Some(Box::new(target));
        self
    }

    fn validate(&self) -> Result<(), WriteError> {
        match self.relation {
            GoToETargetRelation::Child => {
                if self.page.is_some() != self.annotation.is_some() {
                    return Err(WriteError::InvalidObject(
                        "GoToE child target /P and /A must appear together".into(),
                    ));
                }
                if self.name.is_some() == self.page.is_some() {
                    return Err(WriteError::InvalidObject(
                        "GoToE child target must carry either /N or both /P and /A".into(),
                    ));
                }
            }
            GoToETargetRelation::Parent => {
                if self.name.is_some() || self.page.is_some() || self.annotation.is_some() {
                    return Err(WriteError::InvalidObject(
                        "GoToE parent target must not carry /N /P or /A".into(),
                    ));
                }
            }
        }
        if let Some(name) = &self.name
            && name.trim().is_empty()
        {
            return Err(WriteError::InvalidObject(
                "GoToE target /N must not be empty".into(),
            ));
        }
        match &self.page {
            Some(GoToETargetPage::Page(index)) if *index < 0 => {
                return Err(WriteError::InvalidObject(
                    "GoToE target /P page index must be non-negative".into(),
                ));
            }
            Some(GoToETargetPage::Named(name)) if name.trim().is_empty() => {
                return Err(WriteError::InvalidObject(
                    "GoToE target /P named destination must not be empty".into(),
                ));
            }
            _ => {}
        }
        match &self.annotation {
            Some(GoToETargetAnnotation::Index(index)) if *index < 0 => {
                return Err(WriteError::InvalidObject(
                    "GoToE target /A annotation index must be non-negative".into(),
                ));
            }
            Some(GoToETargetAnnotation::Name(name)) if name.trim().is_empty() => {
                return Err(WriteError::InvalidObject(
                    "GoToE target /A annotation name must not be empty".into(),
                ));
            }
            _ => {}
        }
        if let Some(target) = &self.target {
            target.validate()?;
        }
        Ok(())
    }

    fn to_dict(&self) -> Dict {
        let relation = match self.relation {
            GoToETargetRelation::Parent => "P",
            GoToETargetRelation::Child => "C",
        };
        let mut dict = Dict::new().insert("R", Value::Name(relation.into()));
        if let Some(name) = &self.name {
            dict = dict.insert("N", text_string_value(name));
        }
        if let Some(page) = &self.page {
            let value = match page {
                GoToETargetPage::Page(index) => Value::Integer(*index),
                GoToETargetPage::Named(name) => text_string_value(name),
            };
            dict = dict.insert("P", value);
        }
        if let Some(annotation) = &self.annotation {
            let value = match annotation {
                GoToETargetAnnotation::Index(index) => Value::Integer(*index),
                GoToETargetAnnotation::Name(name) => text_string_value(name),
            };
            dict = dict.insert("A", value);
        }
        if let Some(target) = &self.target {
            dict = dict.insert("T", Value::Dict(target.to_dict()));
        }
        dict
    }
}

/// 目标字典的相对关系（Table 202 `/R`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoToETargetRelation {
    /// 目标是当前文档的父文档（`P`）。
    Parent,
    /// 目标是当前文档的子文档（`C`）。
    Child,
}

/// 目标字典 `/P`：0 基页号或当前文档中的命名目标。
#[derive(Debug, Clone, PartialEq)]
pub enum GoToETargetPage {
    /// 0 基页号。
    Page(i64),
    /// 命名目标。
    Named(String),
}

/// 目标字典 `/A`：附件注解的索引或 `/NM` 名称。
#[derive(Debug, Clone, PartialEq)]
pub enum GoToETargetAnnotation {
    /// 页 `/Annots` 数组中的 0 基索引。
    Index(i64),
    /// 注解字典 `/NM` 的值。
    Name(String),
}
