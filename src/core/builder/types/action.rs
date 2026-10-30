//! 动作字典写端（ISO 32000-1 12.6 / 12.7.5）。
//!
//! 覆盖六种常用动作（GoTo、GoToR、Named、URI、Launch、JavaScript）、Hide
//! 动作（隐藏/显示注解）与三种表单动作（SubmitForm、ResetForm、
//! ImportData）。动作字典经 [`PdfBuilder::action`] 注册为句柄，由大纲条目
//! 或注解引用写出。

use crate::core::reader::navigation::Destination;
use crate::core::types::{Array, Dict, Value};
use crate::core::writer::WriteError;

use super::text_string_value;

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

/// Hide 动作（12.6.4.10 Table 210）。
#[derive(Debug, Clone, PartialEq)]
pub struct HideAction {
    targets: Vec<HideTarget>,
    hide: Option<bool>,
}

impl HideAction {
    /// 构造 Hide 动作；`targets` 至少一项。
    pub fn new(targets: Vec<HideTarget>) -> Self {
        Self {
            targets,
            hide: None,
        }
    }

    /// 显式写 `/H`：`true` 隐藏、`false` 显示；缺省按 Table 210 视为 `true`。
    pub fn with_hidden(mut self, hidden: bool) -> Self {
        self.hide = Some(hidden);
        self
    }

    pub(crate) fn targets(&self) -> &[HideTarget] {
        &self.targets
    }

    pub(crate) fn hide(&self) -> Option<bool> {
        self.hide
    }

    /// 校验目标并与注解池规模比对（写出前的零字节契约）。
    pub(crate) fn validate(&self, annotation_count: usize) -> Result<(), WriteError> {
        if self.targets.is_empty() {
            return Err(WriteError::InvalidObject(
                "Hide action /T must name at least one target".into(),
            ));
        }
        let mut names = std::collections::HashSet::new();
        let mut handles = std::collections::HashSet::new();
        for target in &self.targets {
            match target {
                HideTarget::FieldName(name) => {
                    if name.trim().is_empty() {
                        return Err(WriteError::InvalidObject(
                            "Hide action /T field name must not be empty".into(),
                        ));
                    }
                    if !names.insert(name.as_str()) {
                        return Err(WriteError::InvalidObject(format!(
                            "Hide action /T field name {name:?} is duplicated"
                        )));
                    }
                }
                HideTarget::Annotation(handle) => {
                    if *handle >= annotation_count {
                        return Err(WriteError::InvalidObject(format!(
                            "Hide action /T references unknown annotation handle {handle}"
                        )));
                    }
                    if !handles.insert(*handle) {
                        return Err(WriteError::InvalidObject(format!(
                            "Hide action /T references annotation handle {handle} twice"
                        )));
                    }
                }
            }
        }
        Ok(())
    }
}

/// Hide 动作的目标（Table 210 `/T`）。
#[derive(Debug, Clone, PartialEq)]
pub enum HideTarget {
    /// 注解注册句柄（`PdfBuilder::add_annotation` 的返回值）。
    Annotation(usize),
    /// 交互表单字段的全限定名。
    FieldName(String),
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

/// 动作字典（12.6.2 Table 197、12.7.5 Table 236/238/240）。
///
/// 写端支持十种动作；未知子类型不建模，由调用方直接构造字典。
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
            Self::SubmitForm { .. } => "SubmitForm",
            Self::ResetForm { .. } => "ResetForm",
            Self::ImportData { .. } => "ImportData",
        }
    }

    /// 构造动作字典；`annotation_nums` 为注解句柄到对象号的映射，
    /// 供 Hide 的 `/T` 引用解析。
    pub(crate) fn to_dict(&self, annotation_nums: &[i64]) -> Result<Dict, WriteError> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::reader::navigation::Destination;

    #[test]
    fn goto_action_writes_dictionary() {
        let action = Action::GoTo(Destination::Fit { page: 0 });
        let dict = action.to_dict(&[]).unwrap();
        assert_eq!(dict.get("S"), Some(&Value::Name("GoTo".into())));
        assert!(dict.get("D").is_some());
    }

    #[test]
    fn uri_action_writes_string() {
        let action = Action::Uri("https://example.com".into());
        let dict = action.to_dict(&[]).unwrap();
        assert_eq!(dict.get("S"), Some(&Value::Name("URI".into())));
        assert!(matches!(dict.get("URI"), Some(Value::HexString(_))));
    }

    #[test]
    fn launch_action_writes_new_window() {
        let action = Action::Launch {
            file: ActionFileSpec::new("viewer.exe"),
            new_window: Some(true),
        };
        let dict = action.to_dict(&[]).unwrap();
        assert_eq!(dict.get("S"), Some(&Value::Name("Launch".into())));
        assert_eq!(dict.get("NewWindow"), Some(&Value::Bool(true)));
    }

    #[test]
    fn javascript_action_writes_script() {
        let action = Action::JavaScript("app.alert(1);".into());
        let dict = action.to_dict(&[]).unwrap();
        assert_eq!(dict.get("S"), Some(&Value::Name("JavaScript".into())));
        assert!(matches!(dict.get("JS"), Some(Value::HexString(_))));
    }

    #[test]
    fn named_action_writes_name() {
        let action = Action::Named(ActionNamedAction::NextPage);
        let dict = action.to_dict(&[]).unwrap();
        assert_eq!(dict.get("S"), Some(&Value::Name("Named".into())));
        assert_eq!(dict.get("N"), Some(&Value::Name("NextPage".into())));
    }

    #[test]
    fn goto_remote_action_writes_file_and_destination() {
        let action = Action::GoToR {
            file: ActionFileSpec::new("other.pdf"),
            destination: ActionRemoteDestination::Page(5),
        };
        let dict = action.to_dict(&[]).unwrap();
        assert_eq!(dict.get("S"), Some(&Value::Name("GoToR".into())));
        assert!(dict.get("F").is_some());
        assert_eq!(dict.get("D"), Some(&Value::Integer(5)));
    }

    #[test]
    fn hide_action_writes_targets_and_flag() {
        let action = Action::Hide(
            HideAction::new(vec![
                HideTarget::Annotation(0),
                HideTarget::FieldName("group.note".into()),
            ])
            .with_hidden(false),
        );
        let dict = action.to_dict(&[20]).unwrap();
        assert_eq!(dict.get("S"), Some(&Value::Name("Hide".into())));
        let targets = dict.get("T").and_then(Value::as_array).expect("/T");
        assert_eq!(targets.0[0], Value::Ref(20, 0));
        assert!(matches!(targets.0[1], Value::HexString(_)));
        assert_eq!(dict.get("H"), Some(&Value::Bool(false)));

        let single = Action::Hide(HideAction::new(vec![HideTarget::Annotation(0)]));
        let dict = single.to_dict(&[20]).unwrap();
        assert_eq!(dict.get("T"), Some(&Value::Ref(20, 0)), "单目标写标量");
        assert!(dict.get("H").is_none(), "缺省 /H 不物化");
    }

    #[test]
    fn hide_action_rejects_invalid_targets() {
        let empty = Action::Hide(HideAction::new(Vec::new()));
        assert!(matches!(
            empty.to_dict(&[]),
            Err(WriteError::InvalidObject(_))
        ));

        let unknown = Action::Hide(HideAction::new(vec![HideTarget::Annotation(3)]));
        assert!(matches!(
            unknown.to_dict(&[20]),
            Err(WriteError::InvalidObject(_))
        ));

        let duplicated = Action::Hide(HideAction::new(vec![
            HideTarget::FieldName("name".into()),
            HideTarget::FieldName("name".into()),
        ]));
        assert!(matches!(
            duplicated.to_dict(&[]),
            Err(WriteError::InvalidObject(_))
        ));
    }
}
