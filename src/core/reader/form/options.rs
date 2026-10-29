//! `/Opt` 选项数组与选择字段索引（Table 227 按钮字段 / Table 231 选择字段）。
//!
//! 选择字段每个元素是文本串或 `[导出值 显示文本]` 两元素数组（Table 231）；
//! 复选框与单选按钮字段每个元素只能是单个文本串，表示对应 Widget 的开
//! 状态（Table 227，PDF 1.4）。`/TI` 与 `/I` 是选择字段指向自身 `/Opt`
//! 的索引条目，不随字段树下传。

use super::{FieldType, Result, malformed};
use crate::core::reader::content::text_value;
use crate::core::types::{Document, Value};

/// 选项条目（`/Opt` 数组元素）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormOption {
    export: String,
    display: Option<String>,
}

impl FormOption {
    /// 导出值；按钮字段中为对应 Widget 开状态的导出值。
    pub fn export(&self) -> &str {
        &self.export
    }

    /// 显示文本；仅选择字段的 `[导出值 显示文本]` 元素提供。
    pub fn display(&self) -> Option<&str> {
        self.display.as_deref()
    }
}

/// 解析 `/Opt`：按钮字段要求单个文本串（Table 227），其余字段接受
/// 文本串或两元素 `[导出值 显示文本]` 数组（Table 231）。
pub(super) fn parse_options(
    document: &Document,
    value: &Value,
    field_type: Option<&FieldType>,
) -> Result<Vec<FormOption>> {
    let resolved = crate::core::reader::navigation::resolve(document, value)
        .map_err(|err| malformed(err.to_string()))?;
    let array = resolved
        .as_array()
        .ok_or_else(|| malformed("form field /Opt is not an array"))?;
    let button = matches!(field_type, Some(FieldType::Button));
    let mut options = Vec::with_capacity(array.0.len());
    for entry in &array.0 {
        let resolved = crate::core::reader::navigation::resolve(document, entry)
            .map_err(|err| malformed(err.to_string()))?;
        match &resolved {
            Value::String(_) | Value::HexString(_) => {
                let export = text_value(&resolved).ok_or_else(|| {
                    malformed("form field /Opt entry is not a decodable text string")
                })?;
                options.push(FormOption {
                    export,
                    display: None,
                });
            }
            Value::Array(pair) if !button => {
                if pair.0.len() != 2 {
                    return Err(malformed(
                        "choice field /Opt entries must be a text string or two text strings",
                    ));
                }
                let export = resolve_text(document, &pair.0[0], "export value")?;
                let display = resolve_text(document, &pair.0[1], "display value")?;
                options.push(FormOption {
                    export,
                    display: Some(display),
                });
            }
            Value::Array(_) => {
                return Err(malformed(
                    "button field /Opt entries must be single text strings",
                ));
            }
            _ => return Err(malformed("form field /Opt entry is not a text string")),
        }
    }
    Ok(options)
}

/// 解析 `/TI`（Table 231）：可滚动列表框首个可见选项的 `/Opt` 下标；
/// 缺省 0 保留为 `None`。
pub(super) fn parse_top_index(value: &Value, option_count: usize) -> Result<i64> {
    let index = super::parse_integer(value, "form field /TI")?;
    if option_count == 0 {
        return Err(malformed("form field /TI requires a /Opt array"));
    }
    if index < 0 || index >= option_count as i64 {
        return Err(malformed(format!(
            "form field /TI index {index} is outside the {option_count} entries in /Opt"
        )));
    }
    Ok(index)
}

/// 解析 `/I`（Table 231，PDF 1.4）：多选状态下已选中的 `/Opt` 下标数组，
/// 须严格升序且逐项落在 `/Opt` 之内。
pub(super) fn parse_alternate_indices(
    document: &Document,
    value: &Value,
    option_count: usize,
) -> Result<Vec<i64>> {
    if option_count == 0 {
        return Err(malformed("form field /I requires a /Opt array"));
    }
    let resolved = crate::core::reader::navigation::resolve(document, value)
        .map_err(|err| malformed(err.to_string()))?;
    let array = resolved
        .as_array()
        .ok_or_else(|| malformed("form field /I is not an array"))?;
    let mut indices = Vec::with_capacity(array.0.len());
    for entry in &array.0 {
        let index = super::parse_integer(entry, "form field /I entry")?;
        if index < 0 || index >= option_count as i64 {
            return Err(malformed(format!(
                "form field /I index {index} is outside the {option_count} entries in /Opt"
            )));
        }
        if let Some(previous) = indices.last().copied()
            && index <= previous
        {
            return Err(malformed(format!(
                "form field /I must list indices in strictly ascending order, {previous} then {index}"
            )));
        }
        indices.push(index);
    }
    Ok(indices)
}

fn resolve_text(document: &Document, value: &Value, role: &'static str) -> Result<String> {
    let resolved = crate::core::reader::navigation::resolve(document, value)
        .map_err(|err| malformed(err.to_string()))?;
    if !matches!(resolved, Value::String(_) | Value::HexString(_)) {
        return Err(malformed(format!(
            "choice field /Opt {role} is not a text string"
        )));
    }
    text_value(&resolved)
        .ok_or_else(|| malformed(format!("choice field /Opt {role} is not decodable")))
}
