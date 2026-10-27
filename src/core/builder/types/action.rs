//! 动作字典写端（ISO 32000-1 12.6）。
//!
//! 覆盖六种常用动作：GoTo、GoToR、Named、URI、Launch、JavaScript。动作字典
//! 经 [`PdfBuilder::action`] 注册为句柄，由大纲条目或注解引用写出。

use crate::core::reader::navigation::Destination;
use crate::core::types::{Dict, Value};

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

/// 动作字典（12.6.2 Table 197）。
///
/// 写端支持六种常用动作；未知子类型不建模，由调用方直接构造字典。
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
        }
    }

    pub(crate) fn to_dict(&self) -> Dict {
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
        }
        dict
    }
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
        let dict = action.to_dict();
        assert_eq!(dict.get("S"), Some(&Value::Name("GoTo".into())));
        assert!(dict.get("D").is_some());
    }

    #[test]
    fn uri_action_writes_string() {
        let action = Action::Uri("https://example.com".into());
        let dict = action.to_dict();
        assert_eq!(dict.get("S"), Some(&Value::Name("URI".into())));
        assert!(matches!(dict.get("URI"), Some(Value::HexString(_))));
    }

    #[test]
    fn launch_action_writes_new_window() {
        let action = Action::Launch {
            file: ActionFileSpec::new("viewer.exe"),
            new_window: Some(true),
        };
        let dict = action.to_dict();
        assert_eq!(dict.get("S"), Some(&Value::Name("Launch".into())));
        assert_eq!(dict.get("NewWindow"), Some(&Value::Bool(true)));
    }

    #[test]
    fn javascript_action_writes_script() {
        let action = Action::JavaScript("app.alert(1);".into());
        let dict = action.to_dict();
        assert_eq!(dict.get("S"), Some(&Value::Name("JavaScript".into())));
        assert!(matches!(dict.get("JS"), Some(Value::HexString(_))));
    }

    #[test]
    fn named_action_writes_name() {
        let action = Action::Named(ActionNamedAction::NextPage);
        let dict = action.to_dict();
        assert_eq!(dict.get("S"), Some(&Value::Name("Named".into())));
        assert_eq!(dict.get("N"), Some(&Value::Name("NextPage".into())));
    }

    #[test]
    fn goto_remote_action_writes_file_and_destination() {
        let action = Action::GoToR {
            file: ActionFileSpec::new("other.pdf"),
            destination: ActionRemoteDestination::Page(5),
        };
        let dict = action.to_dict();
        assert_eq!(dict.get("S"), Some(&Value::Name("GoToR".into())));
        assert!(dict.get("F").is_some());
        assert_eq!(dict.get("D"), Some(&Value::Integer(5)));
    }
}
