//! Movie 动作写端（12.6.4.9 Table 209）。
//!
//! Movie 动作播放 Movie 注解关联的电影：`/Annotation` 与 `/T` 二者恰写其
//! 一——前者按注解句柄写成间接引用（目标须为 Movie 注解），后者按标题
//! 文本串匹配文档中的 Movie 注解。激活条目与 Movie 注解 `/A` 共用
//! [`MovieActivation`]（Table 296），只写出显式声明的键。
//!
//! 句柄越界与激活条目取值在任何对象写出前由 `action_requirements`
//! 拒绝，保证零字节输出契约。

use crate::core::builder::types::{MovieActivation, text_string_value};
use crate::core::types::{Dict, Value};
use crate::core::writer::WriteError;

/// Movie 动作的目标：指向 Movie 注解的句柄或注解 `/T` 标题。
#[derive(Debug, Clone, PartialEq)]
pub enum MovieActionTarget {
    /// Movie 注解句柄（写出为该注解的间接引用）。
    Annotation(usize),
    /// Movie 注解的 `/T` 标题（按文本串写出）。
    Title(String),
}

/// Movie 动作的构建态（Table 209）。
#[derive(Debug, Clone, PartialEq)]
pub struct MovieAction {
    target: MovieActionTarget,
    activation: MovieActivation,
}

impl MovieAction {
    /// 按注解句柄指定播放目标；句柄须指向已注册的 Movie 注解。
    pub fn annotation(annotation_handle: usize) -> Self {
        Self {
            target: MovieActionTarget::Annotation(annotation_handle),
            activation: MovieActivation::new(),
        }
    }

    /// 按标题指定播放目标；匹配文档中 `/T` 相同的 Movie 注解。
    pub fn title(title: impl Into<String>) -> Self {
        Self {
            target: MovieActionTarget::Title(title.into()),
            activation: MovieActivation::new(),
        }
    }

    /// 播放参数（Table 296）。
    pub fn with_activation(mut self, activation: MovieActivation) -> Self {
        self.activation = activation;
        self
    }

    /// 播放目标。
    pub fn target(&self) -> &MovieActionTarget {
        &self.target
    }

    /// 校验目标与激活条目；注解句柄上界按注册的注解总数判定。
    pub(crate) fn validate(&self, annotation_count: usize) -> Result<(), WriteError> {
        match &self.target {
            MovieActionTarget::Annotation(handle) if *handle >= annotation_count => {
                return Err(WriteError::InvalidObject(format!(
                    "Movie action /Annotation references unknown annotation handle {handle}"
                )));
            }
            MovieActionTarget::Title(title) if title.trim().is_empty() => {
                return Err(WriteError::InvalidObject(
                    "Movie action /T title must not be empty".into(),
                ));
            }
            _ => {}
        }
        self.activation
            .validate()
            .map_err(WriteError::InvalidObject)
    }

    /// 写出除 `/S` 外的动作条目；`annotation_nums` 为注解句柄到对象号的映射。
    pub(crate) fn to_dict(&self, annotation_nums: &[i64]) -> Result<Dict, WriteError> {
        self.validate(annotation_nums.len())?;
        let mut dict = Dict::new();
        match &self.target {
            MovieActionTarget::Annotation(handle) => {
                let number = annotation_nums.get(*handle).ok_or_else(|| {
                    WriteError::InvalidObject(format!(
                        "Movie action /Annotation references unknown annotation handle {handle}"
                    ))
                })?;
                dict = dict.insert("Annotation", Value::Ref(*number, 0));
            }
            MovieActionTarget::Title(title) => {
                dict = dict.insert("T", text_string_value(title));
            }
        }
        for (key, value) in self.activation.to_dict().0 {
            dict = dict.insert(key, value);
        }
        Ok(dict)
    }
}
