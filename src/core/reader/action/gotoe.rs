//! Embedded Go-To 动作读取（12.6.4.4 Table 201/202）。
//!
//! `/F` 是相对源根文档的目标根文档，`/D` 是目标文档内的目标，`/T` 是由
//! 目标字典（Table 202）组成的路径链。子目标在 EmbeddedFiles 名称树中
//! 用 `/N` 定位，在文件附件注解上用 `/P` + `/A` 定位；父目标不带定位条目。
//! 未知 `/R` 关系保留名称。

use crate::core::reader::content::text_value;
use crate::core::reader::navigation::resolve;
use crate::core::types::{Dict, Document, Value};

use super::{
    ActionError, FileSpec, RemoteDestination, malformed, parse_file_spec, parse_remote_destination,
};

/// 目标字典路径的最大嵌套深度，防御畸形文件的深层链。
const MAX_TARGET_DEPTH: usize = 32;

/// Embedded Go-To 动作（12.6.4.4 Table 201）。
#[derive(Debug, Clone, PartialEq)]
pub struct GoToEAction {
    /// `/F` 目标根文档；缺省表示与源文档共享根文档。
    pub file: Option<FileSpec>,
    /// `/D` 目标文档内的目标。
    pub destination: RemoteDestination,
    /// `/NewWindow`；缺省由查看器决定。
    pub new_window: Option<bool>,
    /// `/T` 目标路径；`/F` 存在时可省略。
    pub target: Option<GoToETarget>,
}

/// 目标字典（Table 202）：路径上的一个元素。
#[derive(Debug, Clone, PartialEq)]
pub struct GoToETarget {
    /// `/R` 与当前文档的关系。
    pub relation: GoToETargetRelation,
    /// `/N` EmbeddedFiles 名称树中的文件名。
    pub name: Option<String>,
    /// `/P` 当前文档中包含文件附件注解的页。
    pub page: Option<GoToETargetPage>,
    /// `/A` 该页上的文件附件注解。
    pub annotation: Option<GoToETargetAnnotation>,
    /// `/T` 路径的下一段。
    pub target: Option<Box<GoToETarget>>,
}

/// 目标字典的相对关系（Table 202 `/R`）。
#[derive(Debug, Clone, PartialEq)]
pub enum GoToETargetRelation {
    /// 目标是当前文档的父文档（`P`）。
    Parent,
    /// 目标是当前文档的子文档（`C`）。
    Child,
    /// 其他关系名，原样保留。
    Other(String),
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

pub(super) fn parse_embedded_go_to_action(
    document: &Document,
    dict: &Dict,
) -> Result<GoToEAction, ActionError> {
    let file = match dict.get("F") {
        None => None,
        Some(value) => Some(parse_file_spec(document, value)?),
    };
    let destination = parse_remote_destination(
        document,
        dict.get("D")
            .ok_or_else(|| malformed("GoToE action is missing /D"))?,
    )?;
    let new_window = match dict.get("NewWindow") {
        None => None,
        Some(value) => match resolve(document, value)? {
            Value::Bool(flag) => Some(flag),
            _ => return Err(malformed("GoToE action /NewWindow is not a boolean")),
        },
    };
    let target = match dict.get("T") {
        None => None,
        Some(value) => Some(parse_target(document, value, 0, &mut Vec::new())?),
    };
    if target.is_none() && file.is_none() {
        return Err(malformed("GoToE action has neither /F nor /T"));
    }
    Ok(GoToEAction {
        file,
        destination,
        new_window,
        target,
    })
}

fn parse_target(
    document: &Document,
    value: &Value,
    depth: usize,
    path: &mut Vec<(i64, i64)>,
) -> Result<GoToETarget, ActionError> {
    if depth >= MAX_TARGET_DEPTH {
        return Err(malformed("GoToE target dictionaries nest too deep"));
    }
    let tracked = match value {
        Value::Ref(number, generation) => {
            if path.contains(&(*number, *generation)) {
                return Err(malformed("GoToE target dictionary refers to itself"));
            }
            path.push((*number, *generation));
            true
        }
        _ => false,
    };
    let result = parse_target_dict(document, value, depth, path);
    if tracked {
        path.pop();
    }
    result
}

fn parse_target_dict(
    document: &Document,
    value: &Value,
    depth: usize,
    path: &mut Vec<(i64, i64)>,
) -> Result<GoToETarget, ActionError> {
    let resolved = resolve(document, value)?;
    let dict = resolved
        .as_dict()
        .ok_or_else(|| malformed("GoToE /T target is not a dictionary"))?;
    let relation = match dict.get("R") {
        None => return Err(malformed("GoToE target dictionary is missing /R")),
        Some(value) => match resolve(document, value)?.as_name() {
            Some("P") => GoToETargetRelation::Parent,
            Some("C") => GoToETargetRelation::Child,
            Some(other) => GoToETargetRelation::Other(other.to_owned()),
            None => return Err(malformed("GoToE target /R is not a name")),
        },
    };
    let name = match dict.get("N") {
        None => None,
        Some(value) => Some(target_text(document, value, "GoToE target /N")?),
    };
    let page = match dict.get("P") {
        None => None,
        Some(value) => Some(match resolve(document, value)? {
            Value::Integer(page) if page >= 0 => GoToETargetPage::Page(page),
            Value::Integer(page) => {
                return Err(malformed(format!(
                    "GoToE target /P page index {page} is negative"
                )));
            }
            Value::String(_) | Value::HexString(_) => {
                GoToETargetPage::Named(target_text(document, value, "GoToE target /P")?)
            }
            _ => {
                return Err(malformed(
                    "GoToE target /P is neither a page index nor a named destination",
                ));
            }
        }),
    };
    let annotation = match dict.get("A") {
        None => None,
        Some(value) => Some(match resolve(document, value)? {
            Value::Integer(index) if index >= 0 => GoToETargetAnnotation::Index(index),
            Value::Integer(index) => {
                return Err(malformed(format!(
                    "GoToE target /A annotation index {index} is negative"
                )));
            }
            Value::String(_) | Value::HexString(_) => {
                GoToETargetAnnotation::Name(target_text(document, value, "GoToE target /A")?)
            }
            _ => {
                return Err(malformed(
                    "GoToE target /A is neither an annotation index nor an annotation name",
                ));
            }
        }),
    };
    let target = match dict.get("T") {
        None => None,
        Some(value) => Some(Box::new(parse_target(document, value, depth + 1, path)?)),
    };
    validate_target_shape(
        &relation,
        name.is_some(),
        page.is_some(),
        annotation.is_some(),
    )?;
    Ok(GoToETarget {
        relation,
        name,
        page,
        annotation,
        target,
    })
}

/// 按 Table 202 校验定位条目的必需/禁止组合。
fn validate_target_shape(
    relation: &GoToETargetRelation,
    name: bool,
    page: bool,
    annotation: bool,
) -> Result<(), ActionError> {
    match relation {
        GoToETargetRelation::Child => {
            if page != annotation {
                return Err(malformed(
                    "GoToE child target /P and /A must appear together",
                ));
            }
            if name == page {
                return Err(malformed(
                    "GoToE child target must carry either /N or both /P and /A",
                ));
            }
        }
        GoToETargetRelation::Parent => {
            if name || page || annotation {
                return Err(malformed("GoToE parent target must not carry /N /P or /A"));
            }
        }
        GoToETargetRelation::Other(_) => {}
    }
    Ok(())
}

fn target_text(document: &Document, value: &Value, context: &str) -> Result<String, ActionError> {
    let resolved = resolve(document, value)?;
    text_value(&resolved).ok_or_else(|| malformed(format!("{context} is not a text string")))
}
