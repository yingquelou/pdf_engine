//! Go-To-3D-View 动作写端（12.6.4.15 Table 216，PDF 1.6）。
//!
//! `/TA` 指向已注册的 3D 注解句柄，`/V` 相对该注解 artwork 的 `/VA`
//! 视图表选择视图。`Inline` 内联视图字典，`Index`/`Name` 分别按 `/VA`
//! 下标与 `/IN` 内部名选择，`F`/`L` 取首末项；`N`/`P`/`D` 允许在无
//! `/VA` 时退回默认视图，故不做非空校验。
//!
//! 目标注解的子类型与 `/VA` 相对校验在 `action_requirements` 中完成，
//! 任何对象写出前拒绝非法输入（零字节输出契约）。

use crate::core::builder::types::{ThreeDView, text_string_value};
use crate::core::types::{Dict, Value};
use crate::core::writer::WriteError;

/// Go-To-3D-View 动作（12.6.4.15 Table 216）。
#[derive(Debug, Clone, PartialEq)]
pub struct GoTo3DViewAction {
    target: usize,
    view: GoTo3DViewSelection,
}

impl GoTo3DViewAction {
    /// 构造动作；`target` 为 3D 注解句柄。
    pub fn new(target: usize, view: GoTo3DViewSelection) -> Self {
        Self { target, view }
    }

    /// `/TA` 目标注解句柄。
    pub(crate) fn target(&self) -> usize {
        self.target
    }

    /// `/V` 视图选择。
    pub(crate) fn selection(&self) -> &GoTo3DViewSelection {
        &self.view
    }

    /// 只校验句柄上界；目标子类型与 `/VA` 相对校验在 `action_requirements`。
    pub(crate) fn validate(&self, annotation_count: usize) -> Result<(), WriteError> {
        if self.target >= annotation_count {
            return Err(WriteError::InvalidObject(format!(
                "GoTo3DView action /TA references unknown annotation handle {}",
                self.target
            )));
        }
        Ok(())
    }

    /// 写出 `/TA` 与 `/V`（`/S` 由 [`Action::to_dict`](super::Action::to_dict) 补充）。
    pub(crate) fn to_dict(&self, annotation_nums: &[i64]) -> Result<Dict, WriteError> {
        let num = annotation_nums.get(self.target).ok_or_else(|| {
            WriteError::InvalidObject(format!(
                "GoTo3DView action /TA references unknown annotation handle {}",
                self.target
            ))
        })?;
        Ok(Dict::new()
            .insert("TA", Value::Ref(*num, 0))
            .insert("V", self.view.to_value()))
    }
}

/// 动作 `/V` 视图选择（Table 216）；名字比注解 `/3DV` 多 `N` `P` `D`。
#[derive(Debug, Clone, PartialEq)]
pub enum GoTo3DViewSelection {
    /// 内联的 3D 视图字典（Table 304）。
    Inline(Box<ThreeDView>),
    /// `/VA` 数组下标（0 基）。
    Index(usize),
    /// 匹配某个视图 `/IN` 的文本串。
    Name(String),
    /// 名字 `F`：`/VA` 第一项。
    First,
    /// 名字 `L`：`/VA` 最后一项。
    Last,
    /// 名字 `N`：当前视图之后的下一项（来自 `/VA` 时环绕）。
    Next,
    /// 名字 `P`：当前视图之前的上一项（来自 `/VA` 时环绕）。
    Previous,
    /// 名字 `D`：默认视图。
    Default,
}

impl GoTo3DViewSelection {
    /// 相对目标 artwork 的 `/VA` 视图表校验（Inline 调 `view.validate()`）。
    pub(crate) fn validate(&self, views: &[ThreeDView], context: &str) -> Result<(), String> {
        match self {
            Self::Inline(view) => view
                .validate()
                .map_err(|message| format!("{context}: {message}")),
            Self::Index(index) if *index >= views.len() => Err(format!(
                "{context} index {index} is outside the view array ({} views)",
                views.len()
            )),
            Self::Index(_) => Ok(()),
            Self::Name(name)
                if !views
                    .iter()
                    .any(|view| view.internal_name() == Some(name.as_str())) =>
            {
                Err(format!(
                    "{context} name {name:?} does not match any view /IN"
                ))
            }
            Self::Name(_) => Ok(()),
            Self::First | Self::Last if views.is_empty() => Err(format!(
                "{context} First/Last selection requires a nonempty view array"
            )),
            // `N`/`P`/`D` 在无 `/VA` 时由查看器退回默认视图（Table 216）。
            Self::First | Self::Last | Self::Next | Self::Previous | Self::Default => Ok(()),
        }
    }

    /// 写出 `/V`；不重复范围校验（已由 `validate` 完成）。
    pub(crate) fn to_value(&self) -> Value {
        match self {
            Self::Inline(view) => Value::Dict(view.to_dict()),
            Self::Index(index) => Value::Integer(*index as i64),
            Self::Name(name) => text_string_value(name),
            Self::First => Value::Name("F".into()),
            Self::Last => Value::Name("L".into()),
            Self::Next => Value::Name("N".into()),
            Self::Previous => Value::Name("P".into()),
            Self::Default => Value::Name("D".into()),
        }
    }
}
