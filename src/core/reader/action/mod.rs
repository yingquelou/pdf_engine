//! 动作字典读取（ISO 32000-1 12.6 / 12.7.5）。
//!
//! 覆盖常用动作：GoTo、GoToR、GoToE、Named、URI、Launch、JavaScript、Hide、
//! SetOCGState、Transition，以及表单动作 SubmitForm、ResetForm、ImportData；
//! 未知子类型保留名称。动作值可为间接引用，`/D` 的显式目标经页面树映射为
//! 0 基页号。

use std::fmt;

use crate::core::reader::content::text_value;
use crate::core::reader::navigation::{
    NavigationError, OutlineDestination, OutlineItem, destination::parse_outline_destination,
    page_tree_index, resolve,
};
use crate::core::reader::transition::{Transition, parse_transition};
use crate::core::types::{Document, Value};

mod gotoe;
mod ocg;
mod thread;

pub use gotoe::{
    GoToEAction, GoToETarget, GoToETargetAnnotation, GoToETargetPage, GoToETargetRelation,
};
use ocg::parse_set_ocg_state;
pub use ocg::{OcgStateOperation, OcgStateSequence, SetOcgStateAction};
pub use thread::{BeadTarget, ThreadAction, ThreadTarget};

/// 动作字典（12.6.2 Table 197）。
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// 本地跳转；`/D` 为显式目标数组或命名目标。
    GoTo(OutlineDestination),
    /// 跳转到其他文档。
    GoToR {
        file: FileSpec,
        destination: RemoteDestination,
    },
    /// 在两个（可嵌套嵌入的）文档之间跳转（12.6.4.4 Table 201）。
    GoToE(GoToEAction),
    /// 查看器预定义动作（下一页等）。
    Named(NamedAction),
    /// 解析 URI。
    Uri(String),
    /// 启动外部文件。
    Launch {
        file: FileSpec,
        new_window: Option<bool>,
    },
    /// JavaScript 脚本（文本串或流解码后的 UTF-8）。
    JavaScript(String),
    /// 隐藏或显示注解（12.6.4.10 Table 210）。
    Hide(HideAction),
    /// 设置可选内容组状态（12.6.4.12 Table 213）。
    SetOCGState(SetOcgStateAction),
    /// 过渡动作（12.6.4.14 Table 215）。
    Transition(Transition),
    /// 跳转到文章线上的指定 bead（12.6.4.6 Table 205）。
    Thread(ThreadAction),
    /// 提交表单数据（12.7.5.2 Table 236）。
    SubmitForm(SubmitFormAction),
    /// 重置表单字段（12.7.5.3 Table 238）。
    ResetForm(ResetFormAction),
    /// 从 FDF 文件导入字段数据（12.7.5.4 Table 240）。
    ImportData {
        /// FDF 文件规格。
        file: FileSpec,
    },
    /// 未建模的子类型；保留 `/S` 名称，其余条目不解释。
    Unknown { subtype: String },
}

/// 表单动作的字段选择条目（SubmitForm/ResetForm `/Fields`，12.7.5.2/12.7.5.3）。
#[derive(Debug, Clone, PartialEq)]
pub enum ActionField {
    /// 字段全限定名（PDF 1.3 起的文本串）。
    Name(String),
    /// 字段字典的间接引用。
    Reference {
        /// 对象号。
        number: i64,
        /// 生成号。
        generation: i64,
    },
}

/// Submit-Form 动作（12.7.5.2 Table 236）。
#[derive(Debug, Clone, PartialEq)]
pub struct SubmitFormAction {
    /// 提交目标 URL（`/F`，URL 文件规格，7.11.5）。
    pub file: FileSpec,
    /// `/URLType`（PDF 1.7）；缺省 `Browser` 不物化。
    pub url_type: Option<String>,
    /// `/Fields` 选择条目；空表示全部字段。
    pub fields: Vec<ActionField>,
    /// `/Flags`（Table 237），缺省 0。
    pub flags: u32,
}

/// Reset-Form 动作（12.7.5.3 Table 238）。
#[derive(Debug, Clone, PartialEq)]
pub struct ResetFormAction {
    /// `/Fields` 选择条目；空表示全部字段。
    pub fields: Vec<ActionField>,
    /// `/Flags`（Table 239），缺省 0。
    pub flags: u32,
}

/// Hide 动作（12.6.4.10 Table 210）。
#[derive(Debug, Clone, PartialEq)]
pub struct HideAction {
    /// `/T` 目标；注解字典间接引用或字段全限定名文本串。
    pub targets: Vec<HideTarget>,
    /// `/H`；缺省 `true`（隐藏），`false` 为显示。
    pub hide: Option<bool>,
}

/// Hide 动作的目标（Table 210 `/T`）。
#[derive(Debug, Clone, PartialEq)]
pub enum HideTarget {
    /// 注解字典的间接引用。
    Annotation {
        /// 对象号。
        number: i64,
        /// 生成号。
        generation: i64,
    },
    /// 交互表单字段的全限定名（12.7.3.2）。
    FieldName(String),
}

/// 文件规格（7.11.2）：优先取 `/UF`，其次 `/F` 与平台相关路径。
#[derive(Debug, Clone, PartialEq)]
pub struct FileSpec {
    /// 已解码的文件路径。
    pub file: String,
}

/// 远程文档中的目标（GoToR `/D`）。
#[derive(Debug, Clone, PartialEq)]
pub enum RemoteDestination {
    /// 远程文档中的命名目标。
    Named(String),
    /// 远程文档中的 0 基页号。
    Page(i64),
}

/// 查看器预定义动作；`Print`/`SaveAs` 等归入 [`NamedAction::Other`]。
#[derive(Debug, Clone, PartialEq)]
pub enum NamedAction {
    NextPage,
    PrevPage,
    FirstPage,
    LastPage,
    Other(String),
}

/// 动作解析中的类型化错误。
#[derive(Debug, Clone, PartialEq)]
pub enum ActionError {
    /// 结构缺失、类型错误或引用不可解析。
    Malformed(String),
    /// GoTo 目标页面不在页面树中。
    PageNotFound,
}

impl fmt::Display for ActionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed(message) => write!(f, "{message}"),
            Self::PageNotFound => write!(f, "action destination page not found"),
        }
    }
}

impl std::error::Error for ActionError {}

impl From<NavigationError> for ActionError {
    fn from(error: NavigationError) -> Self {
        match error {
            NavigationError::PageNotFound => Self::PageNotFound,
            other => Self::Malformed(other.to_string()),
        }
    }
}

impl Document {
    /// 解析动作字典；`value` 可为间接引用。
    ///
    /// `/S` 缺失或非名称、必需条目缺失、条目类型错误时返回
    /// [`ActionError::Malformed`]；GoTo 目标页不在页面树中返回
    /// [`ActionError::PageNotFound`]。未知 `/S` 返回
    /// [`Action::Unknown`]（宽容真实文件）。
    pub fn action(&self, value: &Value) -> Result<Action, ActionError> {
        let value = resolve(self, value)?;
        let dict = value
            .as_dict()
            .ok_or_else(|| malformed("action is not a dictionary"))?;
        let subtype = dict
            .get("S")
            .and_then(Value::as_name)
            .ok_or_else(|| malformed("action is missing a name /S"))?
            .to_owned();
        match subtype.as_str() {
            "GoTo" => {
                let destination = dict
                    .get("D")
                    .ok_or_else(|| malformed("GoTo action is missing /D"))?;
                let pages = page_tree_index(self)?;
                Ok(Action::GoTo(parse_outline_destination(
                    self,
                    destination,
                    &pages,
                )?))
            }
            "GoToR" => {
                let file = parse_file_spec(
                    self,
                    dict.get("F")
                        .ok_or_else(|| malformed("GoToR action is missing /F"))?,
                )?;
                let destination = parse_remote_destination(
                    self,
                    dict.get("D")
                        .ok_or_else(|| malformed("GoToR action is missing /D"))?,
                )?;
                Ok(Action::GoToR { file, destination })
            }
            "GoToE" => Ok(Action::GoToE(gotoe::parse_embedded_go_to_action(
                self, dict,
            )?)),
            "Named" => {
                let name = dict
                    .get("N")
                    .and_then(Value::as_name)
                    .ok_or_else(|| malformed("Named action is missing a name /N"))?;
                Ok(Action::Named(named_action(name)))
            }
            "URI" => {
                let uri = dict
                    .get("URI")
                    .ok_or_else(|| malformed("URI action is missing /URI"))?;
                let uri = resolve(self, uri)?;
                let uri = text_value(&uri)
                    .ok_or_else(|| malformed("URI action /URI is not a text string"))?;
                Ok(Action::Uri(uri))
            }
            "Launch" => {
                let file = parse_file_spec(
                    self,
                    dict.get("F")
                        .ok_or_else(|| malformed("Launch action is missing /F"))?,
                )?;
                let new_window = match dict.get("NewWindow") {
                    None => None,
                    Some(value) => match resolve(self, value)? {
                        Value::Bool(flag) => Some(flag),
                        _ => return Err(malformed("Launch action /NewWindow is not a boolean")),
                    },
                };
                Ok(Action::Launch { file, new_window })
            }
            "JavaScript" => {
                let script = dict
                    .get("JS")
                    .ok_or_else(|| malformed("JavaScript action is missing /JS"))?;
                let script = resolve(self, script)?;
                let script = match &script {
                    Value::String(_) | Value::HexString(_) => text_value(&script),
                    Value::Stream(..) => self
                        .try_stream_bytes(&script)
                        .ok()
                        .and_then(|bytes| String::from_utf8(bytes).ok()),
                    _ => None,
                }
                .ok_or_else(|| {
                    malformed("JavaScript action /JS is not a text string or UTF-8 stream")
                })?;
                Ok(Action::JavaScript(script))
            }
            "Hide" => {
                let targets = parse_hide_targets(
                    self,
                    dict.get("T")
                        .ok_or_else(|| malformed("Hide action is missing /T"))?,
                )?;
                let hide = match dict.get("H") {
                    None => None,
                    Some(value) => match resolve(self, value)? {
                        Value::Bool(flag) => Some(flag),
                        _ => return Err(malformed("Hide action /H is not a boolean")),
                    },
                };
                Ok(Action::Hide(HideAction { targets, hide }))
            }
            "SetOCGState" => {
                let state = parse_set_ocg_state(
                    self,
                    dict.get("State")
                        .ok_or_else(|| malformed("SetOCGState action is missing /State"))?,
                )?;
                let preserve_radio_buttons = match dict.get("PreserveRB") {
                    None => None,
                    Some(value) => match resolve(self, value)? {
                        Value::Bool(flag) => Some(flag),
                        _ => {
                            return Err(malformed(
                                "SetOCGState action /PreserveRB is not a boolean",
                            ));
                        }
                    },
                };
                Ok(Action::SetOCGState(SetOcgStateAction {
                    state,
                    preserve_radio_buttons,
                }))
            }
            "Trans" => {
                let transition = parse_transition(
                    self,
                    dict.get("Trans")
                        .ok_or_else(|| malformed("Transition action is missing /Trans"))?,
                )
                .map_err(|error| malformed(format!("Transition action: {error}")))?;
                Ok(Action::Transition(transition))
            }
            "Thread" => Ok(Action::Thread(thread::parse_thread_action(self, dict)?)),
            "SubmitForm" => {
                let file = parse_file_spec(
                    self,
                    dict.get("F")
                        .ok_or_else(|| malformed("SubmitForm action is missing /F"))?,
                )?;
                let url_type = match dict.get("URLType") {
                    None => None,
                    Some(value) => match resolve(self, value)? {
                        Value::Name(name) => Some(name),
                        _ => return Err(malformed("SubmitForm action /URLType is not a name")),
                    },
                };
                let fields = parse_action_fields(self, dict.get("Fields"))?;
                let flags = parse_action_flags(self, dict.get("Flags"), "SubmitForm action")?;
                Ok(Action::SubmitForm(SubmitFormAction {
                    file,
                    url_type,
                    fields,
                    flags,
                }))
            }
            "ResetForm" => {
                let fields = parse_action_fields(self, dict.get("Fields"))?;
                let flags = parse_action_flags(self, dict.get("Flags"), "ResetForm action")?;
                Ok(Action::ResetForm(ResetFormAction { fields, flags }))
            }
            "ImportData" => {
                let file = parse_file_spec(
                    self,
                    dict.get("F")
                        .ok_or_else(|| malformed("ImportData action is missing /F"))?,
                )?;
                Ok(Action::ImportData { file })
            }
            other => Ok(Action::Unknown {
                subtype: other.to_owned(),
            }),
        }
    }

    /// 解析大纲条目的 `/A` 动作（12.3.3 大纲条目表、12.6）；无 `/A` 返回 `None`。
    ///
    /// 动作语义完全复用 [`Document::action`]（GoTo/GoToR/GoToE/Uri/JavaScript/
    /// Hide/SetOCGState/Transition/Thread/SubmitForm/ResetForm/ImportData 等），
    /// 条目只有 `/Dest` 时同样返回 `None`。
    pub fn outline_action(&self, item: &OutlineItem) -> Result<Option<Action>, ActionError> {
        item.action
            .as_ref()
            .map(|value| self.action(value))
            .transpose()
    }
}

fn malformed(message: impl Into<String>) -> ActionError {
    ActionError::Malformed(message.into())
}

fn named_action(name: &str) -> NamedAction {
    match name {
        "NextPage" => NamedAction::NextPage,
        "PrevPage" => NamedAction::PrevPage,
        "FirstPage" => NamedAction::FirstPage,
        "LastPage" => NamedAction::LastPage,
        other => NamedAction::Other(other.to_owned()),
    }
}

fn parse_file_spec(document: &Document, value: &Value) -> Result<FileSpec, ActionError> {
    let value = resolve(document, value)?;
    match &value {
        Value::String(_) | Value::HexString(_) => text_value(&value)
            .map(|file| FileSpec { file })
            .ok_or_else(|| malformed("file specification string is malformed")),
        Value::Dict(dict) => {
            for key in ["UF", "F", "DOS", "Unix", "Mac"] {
                let Some(entry) = dict.get(key) else {
                    continue;
                };
                let entry = resolve(document, entry)?;
                if let Some(file) = text_value(&entry) {
                    return Ok(FileSpec { file });
                }
            }
            Err(malformed(
                "file specification dictionary has no usable path",
            ))
        }
        _ => Err(malformed(
            "file specification is neither a string nor a dictionary",
        )),
    }
}

fn parse_action_fields(
    document: &Document,
    value: Option<&Value>,
) -> Result<Vec<ActionField>, ActionError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let value = resolve(document, value)?;
    let array = value
        .as_array()
        .ok_or_else(|| malformed("action /Fields is not an array"))?;
    let mut fields = Vec::with_capacity(array.0.len());
    for item in &array.0 {
        match item {
            Value::Ref(number, generation) => {
                if !matches!(resolve(document, item)?, Value::Dict(_)) {
                    return Err(malformed(
                        "action /Fields reference does not resolve to a field dictionary",
                    ));
                }
                fields.push(ActionField::Reference {
                    number: *number,
                    generation: *generation,
                });
            }
            Value::String(_) | Value::HexString(_) => {
                let name = text_value(item)
                    .ok_or_else(|| malformed("action /Fields name is malformed"))?;
                fields.push(ActionField::Name(name));
            }
            _ => {
                return Err(malformed(
                    "action /Fields entry is neither a field reference nor a text string",
                ));
            }
        }
    }
    Ok(fields)
}

/// 解析 Hide 动作的 `/T`：单个目标或目标数组（Table 210）。
fn parse_hide_targets(document: &Document, value: &Value) -> Result<Vec<HideTarget>, ActionError> {
    let value = resolve(document, value)?;
    match &value {
        Value::Array(array) => {
            if array.0.is_empty() {
                return Err(malformed("Hide action /T array is empty"));
            }
            array
                .0
                .iter()
                .map(|item| parse_hide_target(document, item))
                .collect()
        }
        _ => Ok(vec![parse_hide_target(document, &value)?]),
    }
}

fn parse_hide_target(document: &Document, value: &Value) -> Result<HideTarget, ActionError> {
    match value {
        Value::Ref(number, generation) => {
            if !matches!(resolve(document, value)?, Value::Dict(_)) {
                return Err(malformed(
                    "Hide action /T reference does not resolve to an annotation dictionary",
                ));
            }
            Ok(HideTarget::Annotation {
                number: *number,
                generation: *generation,
            })
        }
        Value::String(_) | Value::HexString(_) => text_value(value)
            .map(HideTarget::FieldName)
            .ok_or_else(|| malformed("Hide action /T field name is malformed")),
        _ => Err(malformed(
            "Hide action /T entry is neither an annotation reference nor a text string",
        )),
    }
}

fn parse_action_flags(
    document: &Document,
    value: Option<&Value>,
    context: &str,
) -> Result<u32, ActionError> {
    let Some(value) = value else {
        return Ok(0);
    };
    match resolve(document, value)? {
        Value::Integer(flags) if (0..=i64::from(u32::MAX)).contains(&flags) => Ok(flags as u32),
        _ => Err(malformed(format!(
            "{context} /Flags is not a non-negative 32-bit integer"
        ))),
    }
}

fn parse_remote_destination(
    document: &Document,
    value: &Value,
) -> Result<RemoteDestination, ActionError> {
    let value = resolve(document, value)?;
    match &value {
        Value::Name(name) => Ok(RemoteDestination::Named(name.clone())),
        Value::String(_) | Value::HexString(_) => text_value(&value)
            .map(RemoteDestination::Named)
            .ok_or_else(|| malformed("GoToR destination string is malformed")),
        Value::Integer(page) => Ok(RemoteDestination::Page(*page)),
        Value::Array(array) => {
            let first = array
                .0
                .first()
                .ok_or_else(|| malformed("GoToR destination array is empty"))?;
            let first = resolve(document, first)?;
            match &first {
                Value::Integer(page) => Ok(RemoteDestination::Page(*page)),
                Value::Name(name) => Ok(RemoteDestination::Named(name.clone())),
                Value::String(_) | Value::HexString(_) => text_value(&first)
                    .map(RemoteDestination::Named)
                    .ok_or_else(|| malformed("GoToR destination array name is malformed")),
                _ => Err(malformed(
                    "GoToR destination array does not start with a page number or name",
                )),
            }
        }
        _ => Err(malformed(
            "GoToR destination is not a name, string, page number or array",
        )),
    }
}

#[cfg(test)]
mod outline_tests;
#[cfg(test)]
mod tests;
