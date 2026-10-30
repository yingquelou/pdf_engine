//! 动作字典写端（ISO 32000-1 12.6 / 12.7.5）。
//!
//! 覆盖常用动作（GoTo、GoToR、Named、URI、Launch、JavaScript）、Sound
//! 动作（播放声音对象）、Movie 动作（播放电影注解）、Rendition 动作
//! （播放多媒体 rendition）、Hide 动作（隐藏/显示注解）、Set-OCG-State 动作
//! （切换可选内容组状态）、Transition 动作（全屏演示过渡特效）、Thread
//! 动作（跳转到文章线 bead）、Embedded Go-To 动作（嵌套嵌入文档间跳转）、
//! Go-To-3D-View 动作（为 3D 注解指定视图）与三种表单动作（SubmitForm、
//! ResetForm、ImportData）。动作字典经 [`PdfBuilder::action`] 注册为句柄，
//! 由大纲条目或注解引用写出。
//!
//! 按族拆分：`targets` 收文件规格/远程目标/预定义动作名，`hide` 收 Hide
//! 目标契约，`ocg` 收 Set-OCG-State 的扁平 `/State` 序列，`thread` 收
//! Thread 动作的线程/bead 索引契约，`gotoe` 收 Embedded Go-To 的目标
//! 字典路径链，`goto3d` 收 Go-To-3D-View 的 3D 注解目标与视图选择，
//! `sound` 收 Sound 动作的声音句柄与播放参数，`movie` 收 Movie 动作的
//! 注解目标与电影激活参数，`rendition` 收 Rendition 动作的操作/脚本依赖
//! 与 Screen 注解目标。

use super::ClipRefs;
use super::text_string_value;
use super::transition::Transition;
use crate::core::reader::navigation::Destination;
use crate::core::types::{Array, Dict, Value};
use crate::core::writer::WriteError;
use std::cell::Cell;

mod goto3d;
mod gotoe;
mod hide;
mod movie;
mod ocg;
mod rendition;
mod sound;
mod targets;
mod thread;

#[cfg(test)]
mod tests;

pub use goto3d::{GoTo3DViewAction, GoTo3DViewSelection};
pub use gotoe::{
    GoToEAction, GoToETarget, GoToETargetAnnotation, GoToETargetPage, GoToETargetRelation,
};
pub use hide::{HideAction, HideTarget};
pub use movie::{MovieAction, MovieActionTarget};
pub use ocg::{OcgStateOperation, OcgStateSequence, SetOcgStateAction};
pub use rendition::{RenditionAction, RenditionOperation};
pub use sound::SoundAction;
pub use targets::{ActionFileSpec, ActionNamedAction, ActionRemoteDestination};
pub use thread::ThreadAction;

/// 动作字典写出时的间接对象号映射。
///
/// Hide 的 `/T`、GoTo3DView 的 `/TA` 与 Rendition 的 `/AN` 用注解映射，
/// Set-OCG-State 的 `/State` 用可选内容组映射，Sound 的 `/Sound` 用声音
/// 对象映射，Rendition 的 media clip `/D` 用嵌入文件、Form XObject 与
/// section 链对象号映射；未涉及的类别传空切片。
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ActionRefs<'a> {
    /// 注解句柄 → 注解对象号。
    pub(crate) annotation_nums: &'a [i64],
    /// 可选内容组句柄 → 组对象号。
    pub(crate) ocg_nums: &'a [i64],
    /// 声音句柄 → 声音流对象号。
    pub(crate) sound_nums: &'a [i64],
    /// Form XObject 句柄 → 表单流对象号。
    pub(crate) form_nums: &'a [i64],
    /// 嵌入文件句柄 → 文件规格对象号。
    pub(crate) embedded_nums: &'a [i64],
    /// 内联 media clip section 的首级 `/D` 目标对象号。
    pub(crate) rendition_nums: &'a [i64],
    /// 内联 media clip section 的消费游标；注解与大纲共享同一次构建的游标。
    pub(crate) rendition_cursor: Option<&'a Cell<usize>>,
}

impl ActionRefs<'_> {
    /// media clip 写出所需的同一组对象号映射。
    pub(crate) fn clip_refs(&self) -> ClipRefs<'_> {
        ClipRefs {
            form_nums: self.form_nums,
            embedded_nums: self.embedded_nums,
            roots: self.rendition_nums,
            cursor: self.rendition_cursor,
        }
    }
}

/// 动作字典（12.6.2 Table 197、12.7.5 Table 236/238/240）。
///
/// 写端支持十七种动作；未知子类型不建模，由调用方直接构造字典。
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
    /// 在嵌套嵌入文档之间跳转（12.6.4.4 Table 201，PDF 1.6）。
    GoToE(GoToEAction),
    /// 查看器预定义动作。
    Named(ActionNamedAction),
    /// 解析 URI。
    Uri(String),
    /// 播放声音（12.6.4.8 Table 208，PDF 1.2）。
    Sound(SoundAction),
    /// 播放电影（12.6.4.9 Table 209，PDF 1.2）。
    Movie(MovieAction),
    /// 播放多媒体 rendition（12.6.4.13 Table 214，PDF 1.5）。
    Rendition(RenditionAction),
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
    /// 全屏演示过渡特效（12.6.4.14 Table 215，PDF 1.5）。
    Transition(Transition),
    /// 为 3D 注解指定视图（12.6.4.15 Table 216，PDF 1.6）。
    GoTo3DView(GoTo3DViewAction),
    /// 跳转到文章线上的指定 bead（12.6.4.6 Table 205，PDF 1.1）。
    Thread(ThreadAction),
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
            Self::GoToE(_) => "GoToE",
            Self::Named(_) => "Named",
            Self::Uri(_) => "URI",
            Self::Sound(_) => "Sound",
            Self::Movie(_) => "Movie",
            Self::Rendition(_) => "Rendition",
            Self::Launch { .. } => "Launch",
            Self::JavaScript(_) => "JavaScript",
            Self::Hide(_) => "Hide",
            Self::SetOcgState(_) => "SetOCGState",
            Self::Transition(_) => "Trans",
            Self::GoTo3DView(_) => "GoTo3DView",
            Self::Thread(_) => "Thread",
            Self::SubmitForm { .. } => "SubmitForm",
            Self::ResetForm { .. } => "ResetForm",
            Self::ImportData { .. } => "ImportData",
        }
    }

    /// 构造动作字典；`refs` 提供注解、可选内容组与声音句柄到对象号的
    /// 映射，供 Hide 的 `/T`、Set-OCG-State 的 `/State` 与 Sound 的
    /// `/Sound` 引用解析。
    pub(crate) fn to_dict(&self, refs: &ActionRefs<'_>) -> Result<Dict, WriteError> {
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
            Self::GoToE(action) => {
                for (key, value) in action.to_dict().0 {
                    dict = dict.insert(key, value);
                }
            }
            Self::Named(action) => {
                dict = dict.insert("N", Value::Name(action.name().into()));
            }
            Self::Uri(uri) => {
                dict = dict.insert("URI", text_string_value(uri));
            }
            Self::Sound(action) => {
                for (key, value) in action.to_dict(refs.sound_nums)?.0 {
                    dict = dict.insert(key, value);
                }
            }
            Self::Movie(action) => {
                for (key, value) in action.to_dict(refs.annotation_nums)?.0 {
                    dict = dict.insert(key, value);
                }
            }
            Self::Rendition(action) => {
                for (key, value) in action.to_dict(refs)?.0 {
                    dict = dict.insert(key, value);
                }
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
                hide.validate(refs.annotation_nums.len())?;
                let mut targets = Vec::with_capacity(hide.targets().len());
                for target in hide.targets() {
                    match target {
                        HideTarget::Annotation(handle) => {
                            let num = refs.annotation_nums.get(*handle).ok_or_else(|| {
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
                state.validate(refs.ocg_nums.len())?;
                let mut items = Vec::new();
                for sequence in state.sequences() {
                    items.push(Value::Name(sequence.operation().name().into()));
                    for &handle in sequence.groups() {
                        let num = refs
                            .ocg_nums
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
            Self::Transition(transition) => {
                dict = dict.insert("Trans", Value::Dict(transition.to_dict()?));
            }
            Self::GoTo3DView(action) => {
                for (key, value) in action.to_dict(refs.annotation_nums)?.0 {
                    dict = dict.insert(key, value);
                }
            }
            Self::Thread(thread) => {
                for (key, value) in thread.to_dict().0 {
                    dict = dict.insert(key, value);
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
            arr.push(left.map_or(Value::Null, Value::Float));
            arr.push(top.map_or(Value::Null, Value::Float));
            arr.push(zoom.map_or(Value::Null, Value::Float));
            Value::Array(crate::core::types::Array(arr))
        }
        Fit { page } => Value::Array(crate::core::types::Array(vec![
            Value::Integer(*page as i64),
            Value::Name("Fit".into()),
        ])),
        FitH { page, top } => Value::Array(crate::core::types::Array(vec![
            Value::Integer(*page as i64),
            Value::Name("FitH".into()),
            top.map_or(Value::Null, Value::Float),
        ])),
        FitV { page, left } => Value::Array(crate::core::types::Array(vec![
            Value::Integer(*page as i64),
            Value::Name("FitV".into()),
            left.map_or(Value::Null, Value::Float),
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
            top.map_or(Value::Null, Value::Float),
        ])),
        FitBV { page, left } => Value::Array(crate::core::types::Array(vec![
            Value::Integer(*page as i64),
            Value::Name("FitBV".into()),
            left.map_or(Value::Null, Value::Float),
        ])),
    }
}
