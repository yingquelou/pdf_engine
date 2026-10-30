//! 动作字典写端（ISO 32000-1 12.6 / 12.7.5）。
//!
//! 覆盖六种常用动作（GoTo、GoToR、Named、URI、Launch、JavaScript）、Hide
//! 动作（隐藏/显示注解）、Set-OCG-State 动作（切换可选内容组状态）与三种
//! 表单动作（SubmitForm、ResetForm、ImportData）。动作字典经
//! [`PdfBuilder::action`] 注册为句柄，由大纲条目或注解引用写出。
//!
//! 按族拆分：`targets` 收文件规格/远程目标/预定义动作名，`hide` 收 Hide
//! 目标契约，`ocg` 收 Set-OCG-State 的扁平 `/State` 序列。

use super::text_string_value;
use crate::core::reader::navigation::Destination;
use crate::core::types::{Array, Dict, Value};
use crate::core::writer::WriteError;

mod hide;
mod ocg;
mod targets;

#[cfg(test)]
mod tests;

pub use hide::{HideAction, HideTarget};
pub use ocg::{OcgStateOperation, OcgStateSequence, SetOcgStateAction};
pub use targets::{ActionFileSpec, ActionNamedAction, ActionRemoteDestination};

/// 动作字典（12.6.2 Table 197、12.7.5 Table 236/238/240）。
///
/// 写端支持十一种动作；未知子类型不建模，由调用方直接构造字典。
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// 本地跳转；`/D` 为显式目标数组或命名目标。
    GoTo(Destination),
    /// 跳转到其他文档。
    GoToR {
        /// 文件规格。
        file: ActionFileSpec,
        /// 远程目标。
        destination: ActionRemoteDestination,
    },
    /// 查看器预定义动作。
    Named(ActionNamedAction),
    /// 解析 URI。
    Uri(String),
    /// 启动外部文件。
    Launch {
        /// 文件规格。
        file: ActionFileSpec,
        /// 是否在新窗口启动；`None` 时省略 `/NewWindow`。
        new_window: Option<bool>,
    },
    /// JavaScript 脚本（按文本串写出）。
    JavaScript(String),
    /// 隐藏或显示注解（12.6.4.10 Table 210）。
    Hide(HideAction),
    /// 设置可选内容组状态（12.6.4.12 Table 213）。
    SetOcgState(SetOcgStateAction),
    /// 提交表单数据（12.7.5.2 Table 236）：`/F` 写成 URL 文件规格。
    SubmitForm {
        /// 提交目标 URL；须为可打印 ASCII。
        url: String,
        /// 字段全限定名列表（`/Fields`）；空表示全部字段。
        fields: Vec<String>,
        /// `/Flags`（Table 237）；0 时省略。
        flags: u32,
        /// `/URLType`（PDF 1.7）；`None` 时省略（缺省 `Browser`）。
        url_type: Option<String>,
    },
    /// 重置表单字段（12.7.5.3 Table 238）。
    ResetForm {
        /// 字段全限定名列表（`/Fields`）；空表示全部字段。
        fields: Vec<String>,
        /// `/Flags`（Table 239）；0 时省略。
        flags: u32,
    },
    /// 从 FDF 文件导入字段数据（12.7.5.4 Table 240）。
    ImportData {
        /// FDF 文件路径（`/F` 文件规格文本串）。
        file: String,
    },
}

impl Action {
    /// 返回动作子类型名称（`/S` 值）。
    pub fn subtype(&self) -> &'static str {
        match self {
            Self::GoTo(_) => "GoTo",
            Self::GoToR { .. } => "GoToR",
            Self::Named(_) => "Named",
            Self::Uri(_) => "URI",
            Self::Launch { .. } => "Launch",
            Self::JavaScript(_) => "JavaScript",
            Self::Hide(_) => "Hide",
            Self::SetOcgState(_) => "SetOCGState",
            Self::SubmitForm { .. } => "SubmitForm",
            Self::ResetForm { .. } => "ResetForm",
            Self::ImportData { .. } => "ImportData",
        }
    }

    /// 构造动作字典；`annotation_nums` 与 `ocg_nums` 分别是注解、可选内容组
    /// 句柄到对象号的映射，供 Hide 的 `/T` 与 Set-OCG-State 的 `/State`
    /// 引用解析。
    pub(crate) fn to_dict(
        &self,
        annotation_nums: &[i64],
        ocg_nums: &[i64],
    ) -> Result<Dict, WriteError> {
        let mut dict = Dict::new().insert("S", Value::Name(self.subtype().into()));
        match self {
            Self::GoTo(dest) => {
                dict = dict.insert("D", destination_to_value(dest));
            }
            Self::GoToR { file, destination } => {
                dict = dict
                    .insert("F", file.to_value())
                    .insert("D", destination.to_value());
            }
            Self::Named(action) => {
                dict = dict.insert("N", Value::Name(action.name().into()));
            }
            Self::Uri(uri) => {
                dict = dict.insert("URI", text_string_value(uri));
            }
            Self::Launch { file, new_window } => {
                dict = dict.insert("F", file.to_value());
                if let Some(flag) = new_window {
                    dict = dict.insert("NewWindow", Value::Bool(*flag));
                }
            }
            Self::JavaScript(script) => {
                dict = dict.insert("JS", text_string_value(script));
            }
            Self::Hide(hide) => {
                hide.validate(annotation_nums.len())?;
                let mut targets = Vec::with_capacity(hide.targets().len());
                for target in hide.targets() {
                    match target {
                        HideTarget::Annotation(handle) => {
                            let num = annotation_nums.get(*handle).ok_or_else(|| {
                                WriteError::InvalidObject(format!(
                                    "Hide action /T references unknown annotation handle {handle}"
                                ))
                            })?;
                            targets.push(Value::Ref(*num, 0));
                        }
                        HideTarget::FieldName(name) => targets.push(text_string_value(name)),
                    }
                }
                // 单目标按 Table 210 写成标量，多目标写成数组。
                let value = if targets.len() == 1 {
                    targets.pop().expect("validated single target")
                } else {
                    Value::Array(Array(targets))
                };
                dict = dict.insert("T", value);
                if let Some(hidden) = hide.hide() {
                    dict = dict.insert("H", Value::Bool(hidden));
                }
            }
            Self::SetOcgState(state) => {
                state.validate(ocg_nums.len())?;
                let mut items = Vec::new();
                for sequence in state.sequences() {
                    items.push(Value::Name(sequence.operation().name().into()));
                    for &handle in sequence.groups() {
                        let num = ocg_nums
                            .get(handle)
                            .copied()
                            .filter(|num| *num > 0)
                            .ok_or_else(|| {
                                WriteError::InvalidObject(format!(
                                    "SetOCGState action /State references optional content group handle {handle} with no allocated object number"
                                ))
                            })?;
                        items.push(Value::Ref(num, 0));
                    }
                }
                dict = dict.insert("State", Value::Array(Array(items)));
                if let Some(preserve) = state.preserve_radio_buttons() {
                    dict = dict.insert("PreserveRB", Value::Bool(preserve));
                }
            }
            Self::SubmitForm {
                url,
                fields,
                flags,
                url_type,
            } => {
                dict = dict.insert(
                    "F",
                    Value::Dict(
                        Dict::new()
                            .insert("FS", Value::Name("URL".into()))
                            .insert("F", Value::String(url.clone())),
                    ),
                );
                if let Some(name) = url_type {
                    dict = dict.insert("URLType", Value::Name(name.clone()));
                }
                if !fields.is_empty() {
                    dict = dict.insert("Fields", fields_value(fields));
                }
                if *flags != 0 {
                    dict = dict.insert("Flags", Value::Integer(i64::from(*flags)));
                }
            }
            Self::ResetForm { fields, flags } => {
                if !fields.is_empty() {
                    dict = dict.insert("Fields", fields_value(fields));
                }
                if *flags != 0 {
                    dict = dict.insert("Flags", Value::Integer(i64::from(*flags)));
                }
            }
            Self::ImportData { file } => {
                dict = dict.insert("F", text_string_value(file));
            }
        }
        Ok(dict)
    }
}

fn fields_value(fields: &[String]) -> Value {
    Value::Array(Array(
        fields
            .iter()
            .map(|field| text_string_value(field))
            .collect(),
    ))
}

fn destination_to_value(dest: &Destination) -> Value {
    use crate::core::reader::navigation::Destination::*;
    match dest {
        Xyz {
            page,
            left,
            top,
            zoom,
        } => {
            let mut arr = vec![Value::Integer(*page as i64), Value::Name("XYZ".into())];
            arr.push(left.map_or(Value::Null, |v| Value::Float(v)));
            arr.push(top.map_or(Value::Null, |v| Value::Float(v)));
            arr.push(zoom.map_or(Value::Null, |v| Value::Float(v)));
            Value::Array(crate::core::types::Array(arr))
        }
        Fit { page } => Value::Array(crate::core::types::Array(vec![
            Value::Integer(*page as i64),
            Value::Name("Fit".into()),
        ])),
        FitH { page, top } => Value::Array(crate::core::types::Array(vec![
            Value::Integer(*page as i64),
            Value::Name("FitH".into()),
            top.map_or(Value::Null, |v| Value::Float(v)),
        ])),
        FitV { page, left } => Value::Array(crate::core::types::Array(vec![
            Value::Integer(*page as i64),
            Value::Name("FitV".into()),
            left.map_or(Value::Null, |v| Value::Float(v)),
        ])),
        FitR { page, rect } => Value::Array(crate::core::types::Array(vec![
            Value::Integer(*page as i64),
            Value::Name("FitR".into()),
            Value::Float(rect[0]),
            Value::Float(rect[1]),
            Value::Float(rect[2]),
            Value::Float(rect[3]),
        ])),
        FitB { page } => Value::Array(crate::core::types::Array(vec![
            Value::Integer(*page as i64),
            Value::Name("FitB".into()),
        ])),
        FitBH { page, top } => Value::Array(crate::core::types::Array(vec![
            Value::Integer(*page as i64),
            Value::Name("FitBH".into()),
            top.map_or(Value::Null, |v| Value::Float(v)),
        ])),
        FitBV { page, left } => Value::Array(crate::core::types::Array(vec![
            Value::Integer(*page as i64),
            Value::Name("FitBV".into()),
            left.map_or(Value::Null, |v| Value::Float(v)),
        ])),
    }
}
