//! Movie 动作读取（12.6.4.9 Table 209）。
//!
//! Movie 动作字典的内容等同电影激活字典（13.4 Table 296），并额外包含
//! `/S /Movie` 与 `/Annotation`（电影注解的间接引用）或 `/T`（电影注解
//! 标题）二选一的定位条目。缺少定位条目或两者同时出现均视为格式错误。
//! 播放参数复用 [`crate::core::reader::movie::parse_movie_activation`]，
//! 缺省语义与 Table 296 一致。

use crate::core::reader::content::text_value;
use crate::core::reader::movie::{MovieActivation, parse_movie_activation};
use crate::core::reader::navigation::resolve;
use crate::core::types::{Dict, Document, Value};

use super::{ActionError, malformed};

/// Movie 动作（Table 209）。
#[derive(Debug, Clone, PartialEq)]
pub struct MovieAction {
    /// `/Annotation` 电影注解的间接引用。
    pub annotation: Option<(i64, i64)>,
    /// `/T` 电影注解标题（与 `/Annotation` 二选一）。
    pub title: Option<String>,
    /// 播放参数（Table 296 条目直接出现在动作字典中）。
    pub activation: MovieActivation,
}

/// 解析 Movie 动作字典；`/S` 已由调用方识别。
pub(super) fn parse_movie_action(
    document: &Document,
    dict: &Dict,
) -> Result<MovieAction, ActionError> {
    let annotation = match dict.get("Annotation") {
        None => None,
        Some(Value::Ref(number, generation)) => {
            let reference = Value::Ref(*number, *generation);
            let resolved = resolve(document, &reference)?;
            let target = resolved.as_dict().ok_or_else(|| {
                malformed("Movie action /Annotation does not resolve to a dictionary")
            })?;
            match target.get("Subtype").and_then(Value::as_name) {
                Some("Movie") => {}
                Some(other) => {
                    return Err(malformed(format!(
                        "Movie action /Annotation targets a {other} annotation"
                    )));
                }
                None => {
                    return Err(malformed("Movie action /Annotation target has no /Subtype"));
                }
            }
            Some((*number, *generation))
        }
        Some(_) => {
            return Err(malformed(
                "Movie action /Annotation must be an indirect reference",
            ));
        }
    };
    let title = match dict.get("T") {
        None => None,
        Some(value) => {
            let text = text_value(value)
                .ok_or_else(|| malformed("Movie action /T is not a text string"))?;
            if text.is_empty() {
                return Err(malformed("Movie action /T must not be empty"));
            }
            Some(text)
        }
    };
    if annotation.is_some() == title.is_some() {
        return Err(malformed(
            "Movie action requires exactly one of /Annotation or /T",
        ));
    }
    let activation = parse_movie_activation(document, dict)
        .map_err(|error| malformed(format!("Movie action: {error}")))?;
    Ok(MovieAction {
        annotation,
        title,
        activation,
    })
}
