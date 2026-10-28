//! 注解子类型专有条目解析（12.5.6）。
//!
//! 通用条目由父模块解析；此处只按各子类型的表格提取专有条目，
//! 读端保持宽容：条目存在即解析，不校验声明的 `/Subtype`
//! （归属校验由写端 `PdfAnnotation::validate` 负责）。
//!
//! - Text（Table 172）：`/Name`、`/Open`、`/State`、`/StateModel`
//! - Caret（Table 180）：`/Symbol`
//! - Ink（Table 182）：`/InkList` 路径

use super::{Result, entries::number, malformed};
use crate::core::types::{Dict, Value};

/// `/InkList` 路径数上限（防护不可信文件）。
const MAX_INK_PATHS: usize = 4096;
/// 单条墨迹路径的坐标数值上限（防护不可信文件）。
const MAX_INK_PATH_VALUES: usize = 8192;

/// 一次子类型专有条目解析的结果，字段由父模块并入 [`super::Annotation`]。
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct SubtypeEntries {
    pub(super) icon_name: Option<String>,
    pub(super) is_open: Option<bool>,
    pub(super) state: Option<String>,
    pub(super) state_model: Option<String>,
    pub(super) symbol: Option<String>,
    pub(super) ink_paths: Vec<Vec<f64>>,
}

/// 解析注解字典中的子类型专有条目。
pub(super) fn parse_subtype_entries(dict: &Dict) -> Result<SubtypeEntries> {
    Ok(SubtypeEntries {
        icon_name: parse_name_or_text(dict, "Name")?,
        is_open: match dict.get("Open") {
            None => None,
            Some(Value::Bool(flag)) => Some(*flag),
            Some(_) => return Err(malformed("annotation /Open must be a boolean")),
        },
        state: parse_name_or_text(dict, "State")?,
        state_model: parse_name_or_text(dict, "StateModel")?,
        symbol: parse_name_or_text(dict, "Symbol")?,
        ink_paths: parse_ink_list(dict)?,
    })
}

/// 名字或文本串条目（Text 注解 `/Name`/`/State`/`/StateModel`、Caret `/Symbol`）
/// 统一解码为 `String`；文本串按 `/DF`/UTF-16BE/PDF 文本串规则解码。
fn parse_name_or_text(dict: &Dict, key: &str) -> Result<Option<String>> {
    let Some(value) = dict.get(key) else {
        return Ok(None);
    };
    if let Some(name) = value.as_name() {
        return Ok(Some(name.to_string()));
    }
    let decoded = crate::core::reader::content::text_value(value)
        .ok_or_else(|| malformed(format!("annotation /{key} must be a name or text string")))?;
    Ok(Some(decoded))
}

/// `/InkList`：数组的数组，每条路径为 x/y 交替的偶数个有限数值。
fn parse_ink_list(dict: &Dict) -> Result<Vec<Vec<f64>>> {
    let Some(value) = dict.get("InkList") else {
        return Ok(Vec::new());
    };
    let outer = value
        .as_array()
        .ok_or_else(|| malformed("annotation /InkList is not an array"))?;
    if outer.0.len() > MAX_INK_PATHS {
        return Err(malformed("annotation /InkList has too many paths"));
    }
    let mut paths = Vec::with_capacity(outer.0.len());
    for path in outer.0.iter() {
        let inner = path
            .as_array()
            .ok_or_else(|| malformed("annotation /InkList path is not an array"))?;
        if inner.0.len() % 2 != 0 || inner.0.is_empty() {
            return Err(malformed(
                "annotation /InkList path must hold an even, nonzero number of values",
            ));
        }
        if inner.0.len() > MAX_INK_PATH_VALUES {
            return Err(malformed("annotation /InkList path has too many values"));
        }
        let mut points = Vec::with_capacity(inner.0.len());
        for item in inner.0.iter() {
            points.push(number(item, "annotation /InkList")?);
        }
        paths.push(points);
    }
    Ok(paths)
}
